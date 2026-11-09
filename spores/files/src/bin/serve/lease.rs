//! Publish this service into Mycelium's application-owner catalog.
//!
//! Mycelium only observes `application-services.json` during a scan, and the
//! gateway resolves a route's `service_kind` from that catalog. The process
//! that owns the listener owns the lease: it rewrites one observation on a
//! timer and stops renewing when it exits, so a dead service disappears after
//! the 300-second TTL instead of leaving a stale endpoint behind.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

/// The schema Mycelium's `local_services` reader accepts.
pub const SCHEMA_VERSION: u64 = 1;
/// Mycelium rejects an observation whose validity is longer than five minutes.
pub const MAX_TTL_SECS: u64 = 300;
const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
const MAX_OBSERVATION_BYTES: usize = 8 * 1024;
const MAX_OBSERVATIONS: usize = 128;
const MAX_ENDPOINTS: usize = 32;
const SCAN_TIMEOUT: Duration = Duration::from_secs(60);
static TEMP: AtomicU64 = AtomicU64::new(0);

/// One application-owned service lease.
#[derive(Clone, Debug)]
pub struct Lease {
    /// `$MYCELIUM_HOME/application-services.json`.
    pub path: PathBuf,
    /// Stable across renewals; this is how a renewal replaces its own entry.
    pub service_id: String,
    /// The gateway route's `service_kind`, e.g. `files`.
    pub kind: String,
    /// The local Mycelium node id. It is also the observation's observer.
    pub node_id: String,
    /// Human-readable node name carried as `attributes.node_label`.
    pub label: Option<String>,
    /// HTTP endpoints a peer can dial. Loopback only reaches the local gateway.
    pub endpoints: Vec<String>,
    /// Validity in seconds, clamped to 1..=300.
    pub ttl_secs: u64,
    /// The application identity carried in `attributes.application`.
    pub application: String,
    /// The application version carried in `attributes.version`.
    pub version: String,
}

/// The other half of publication: a scan is what makes Mycelium read the file.
#[derive(Clone, Debug)]
pub struct Scan {
    pub bin: PathBuf,
    pub every: Duration,
}

impl Lease {
    /// Publish using the current wall clock.
    pub fn publish(&self) -> Result<(), String> {
        self.publish_at(now())
    }

    /// Publish at an explicit time so tests do not sleep.
    pub fn publish_at(&self, now: u64) -> Result<(), String> {
        if !label(&self.service_id) {
            return Err(format!(
                "service id `{}` is not a bounded lowercase label",
                self.service_id
            ));
        }
        if !label(&self.kind) {
            return Err(format!(
                "service kind `{}` is not a bounded lowercase label",
                self.kind
            ));
        }
        if self.node_id.is_empty()
            || !self
                .node_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err(format!("node id `{}` is not a node identity", self.node_id));
        }
        if self.endpoints.is_empty() || self.endpoints.len() > MAX_ENDPOINTS {
            return Err(format!(
                "lease needs 1..={MAX_ENDPOINTS} endpoints, got {}",
                self.endpoints.len()
            ));
        }
        for endpoint in &self.endpoints {
            validate_endpoint(endpoint)?;
        }

        let ttl = self.ttl_secs.clamp(1, MAX_TTL_SECS);
        let mut document = read_document(&self.path)?;
        let observations = document
            .get_mut("observations")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("{}: observations is not an array", self.path.display()))?;
        // Writing into a file myceliumd will reject would make this lease
        // invisible without saying why. Refuse instead of clobbering another
        // application's broken document.
        validate_existing(observations, &self.node_id, now)?;

        // A renewal replaces exactly the entry this process owns. Every other
        // application's observation is preserved value for value. Two writers
        // publishing in the same instant can lose one observation; the next
        // timer restores it, and a briefly missing lease is safer than one that
        // is never refreshed.
        observations.retain(|observation| {
            observation.get("service_id").and_then(Value::as_str) != Some(self.service_id.as_str())
        });
        if observations.len() >= MAX_OBSERVATIONS {
            return Err(format!(
                "{} already holds {MAX_OBSERVATIONS} observations",
                self.path.display()
            ));
        }

        let mut attributes = Map::new();
        attributes.insert("node_id".into(), json!(self.node_id));
        if let Some(label) = self.label.as_deref().filter(|label| !label.is_empty()) {
            attributes.insert("node_label".into(), json!(label));
        }
        attributes.insert("application".into(), json!(self.application));
        attributes.insert("version".into(), json!(self.version));

        let observation = json!({
            "schema_version": SCHEMA_VERSION,
            "service_id": &self.service_id,
            "observed_at": now,
            "expires_at": now.saturating_add(ttl),
            "provenance": {"provider": "application", "observer": &self.node_id},
            "confidence": "strong",
            "value": {
                "kind": &self.kind,
                "endpoints": &self.endpoints,
                "attributes": attributes,
            },
        });
        if serde_json::to_vec(&observation)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_OBSERVATION_BYTES
        {
            return Err("lease observation exceeds 8 KiB".into());
        }
        observations.push(observation);
        write_document(&self.path, &document)
    }
}

/// Publish immediately, then renew before the TTL can lapse. The optional scan
/// runs beside the renewal so a slow scan can never starve the lease.
pub fn start(lease: Lease, scan: Option<Scan>, refresh: Duration) {
    thread::Builder::new()
        .name("files-lease".into())
        .spawn(move || {
            let ttl = lease.ttl_secs.clamp(1, MAX_TTL_SECS);
            // A refresh at the TTL boundary leaves the old lease valid for a
            // full timer tick. Stay comfortably inside it.
            let refresh = refresh.clamp(
                Duration::from_secs(5),
                Duration::from_secs(ttl.saturating_sub(10).max(5)),
            );
            if let Err(error) = lease.publish() {
                eprintln!("files-serve: lease: {error}");
            } else {
                eprintln!(
                    "files-serve: leasing kind={} node={} at {}",
                    lease.kind,
                    lease.node_id,
                    lease.path.display()
                );
            }
            if let Some(scan) = scan {
                // Publish happens above, before this thread can read the file.
                thread::Builder::new()
                    .name("files-lease-scan".into())
                    .spawn(move || {
                        loop {
                            if let Err(error) = run_scan(&scan.bin) {
                                eprintln!("files-serve: lease scan: {error}");
                            }
                            thread::sleep(scan.every.max(Duration::from_secs(30)));
                        }
                    })
                    .ok();
            }
            loop {
                thread::sleep(refresh);
                if let Err(error) = lease.publish() {
                    eprintln!("files-serve: lease: {error}");
                }
            }
        })
        .ok();
}

/// Run one bounded `mycelium scan`, without letting a stuck daemon block the
/// renewer. The scan is the only public way to make myceliumd read the file.
pub fn run_scan(bin: &Path) -> Result<(), String> {
    let mut child = Command::new(bin)
        .args(["scan", "--json"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("start {} scan: {error}", bin.display()))?;
    let deadline = Instant::now() + SCAN_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!("{} scan exited {status}", bin.display()));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} scan timed out", bin.display()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(250)),
            Err(error) => return Err(format!("wait for {} scan: {error}", bin.display())),
        }
    }
}

/// Find the CLI the daemon ships with, without assuming a `PATH` entry.
pub fn default_bin() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("MYCELIUM_HOME") {
        let candidate = Path::new(&home).join("bin/mycelium");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let home = std::env::var_os("HOME")?;
    let candidate = Path::new(&home).join(".mycelium/bin/mycelium");
    candidate.is_file().then_some(candidate)
}

/// Read the local node identity through the same CLI the gateway uses.
pub fn local_node_id(bin: &Path) -> Result<String, String> {
    let output = Command::new(bin)
        .args(["node", "status", "--json"])
        .output()
        .map_err(|error| format!("run {}: {error}", bin.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} node status exited {}: {}",
            bin.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let document: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse {} node status: {error}", bin.display()))?;
    document
        .pointer("/node/hello/node_id")
        .and_then(Value::as_str)
        .filter(|node| !node.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{} node status has no node id", bin.display()))
}

/// The short hostname, used only as a display label.
pub fn hostname() -> String {
    if let Ok(output) = Command::new("hostname").arg("-s").output() {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !value.is_empty() {
                return value;
            }
        }
    }
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Mirror the owner-lease rules myceliumd applies to the whole document. The
/// file is shared by every local application, so one stale or malformed entry
/// makes every entry invisible; preserving it silently is worse than failing.
fn validate_existing(observations: &[Value], node: &str, now: u64) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for (index, observation) in observations.iter().enumerate() {
        let invalid = |detail: &str| format!("application lease {index}: {detail}");
        if observation.get("schema_version").and_then(Value::as_u64) != Some(SCHEMA_VERSION) {
            return Err(invalid("unsupported schema version"));
        }
        let service_id = observation
            .get("service_id")
            .and_then(Value::as_str)
            .filter(|value| label(value))
            .ok_or_else(|| invalid("invalid service id"))?;
        if !seen.insert(service_id.to_owned()) {
            return Err(invalid("duplicate service id"));
        }
        if observation
            .pointer("/provenance/observer")
            .and_then(Value::as_str)
            != Some(node)
        {
            return Err(invalid("provenance observer is not the local node"));
        }
        if observation
            .pointer("/value/attributes/node_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != node)
        {
            return Err(invalid("attributes.node_id is not the local node"));
        }
        match observation.get("confidence").and_then(Value::as_str) {
            Some("strong" | "derived" | "weak" | "ambiguous") => {}
            _ => return Err(invalid("invalid confidence")),
        }
        match observation.pointer("/value/attributes") {
            None => {}
            Some(Value::Object(attributes)) if attributes.len() <= 32 => {}
            Some(_) => return Err(invalid("invalid attributes")),
        }
        let observed_at = observation
            .get("observed_at")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("missing observed_at"))?;
        let expires_at = observation
            .get("expires_at")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("missing expires_at"))?;
        if observed_at > now || expires_at <= observed_at || expires_at - observed_at > MAX_TTL_SECS
        {
            return Err(invalid("invalid validity window"));
        }
        match observation.pointer("/value/kind").and_then(Value::as_str) {
            Some(kind) if label(kind) => {}
            _ => return Err(invalid("invalid service kind")),
        }
        let endpoints = observation
            .pointer("/value/endpoints")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("missing endpoints"))?;
        if endpoints.is_empty() || endpoints.len() > MAX_ENDPOINTS {
            return Err(invalid("invalid endpoint count"));
        }
        for endpoint in endpoints {
            let endpoint = endpoint
                .as_str()
                .ok_or_else(|| invalid("endpoint is not a string"))?;
            validate_endpoint(endpoint)?;
        }
        if serde_json::to_vec(observation)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_OBSERVATION_BYTES
        {
            return Err(invalid("observation exceeds 8 KiB"));
        }
    }
    Ok(())
}

fn read_document(path: &Path) -> Result<Value, String> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > MAX_DOCUMENT_BYTES {
                return Err(format!(
                    "{} exceeds {MAX_DOCUMENT_BYTES} bytes",
                    path.display()
                ));
            }
            let document: Value = serde_json::from_slice(&bytes)
                .map_err(|error| format!("{}: invalid lease document: {error}", path.display()))?;
            if document.get("schema_version").and_then(Value::as_u64) != Some(SCHEMA_VERSION) {
                return Err(format!(
                    "{}: unsupported lease schema version",
                    path.display()
                ));
            }
            let observations = document
                .get("observations")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{}: observations is not an array", path.display()))?;
            if observations.len() > MAX_OBSERVATIONS {
                return Err(format!(
                    "{}: more than {MAX_OBSERVATIONS} observations",
                    path.display()
                ));
            }
            Ok(document)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({
            "schema_version": SCHEMA_VERSION,
            "observations": [],
        })),
        Err(error) => Err(format!("read {}: {error}", path.display())),
    }
}

fn write_document(path: &Path, document: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(document).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(format!(
            "lease document would exceed {MAX_DOCUMENT_BYTES} bytes"
        ));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("application-services.json");
    let temp = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| format!("create {}: {error}", temp.display()))?;
    let result = (|| -> Result<(), String> {
        file.write_all(&bytes)
            .map_err(|error| format!("write {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("sync {}: {error}", temp.display()))?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("chmod {}: {error}", temp.display()))?;
        }
        fs::rename(&temp, path).map_err(|error| format!("rename {}: {error}", temp.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let (scheme, rest) = endpoint
        .split_once("://")
        .ok_or_else(|| format!("endpoint `{endpoint}` has no scheme"))?;
    if !matches!(scheme, "http" | "https") {
        return Err(format!("endpoint `{endpoint}` must be http or https"));
    }
    if rest.is_empty()
        || rest.starts_with('/')
        || rest.contains('@')
        || rest.contains('?')
        || rest.contains('#')
        || rest.chars().any(char::is_whitespace)
    {
        return Err(format!("endpoint `{endpoint}` is not a bare host:port URL"));
    }
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err(format!("endpoint `{endpoint}` has no host"));
    }
    Ok(())
}

/// The label alphabet Mycelium's owner-lease validator accepts.
fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.:/".contains(&byte)
        })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_file() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "files-lease-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("application-services.json")
    }

    fn lease(path: &Path) -> Lease {
        Lease {
            path: path.to_path_buf(),
            service_id: "service/abc123/application/files".into(),
            kind: "files".into(),
            node_id: "abc123".into(),
            label: Some("studio".into()),
            endpoints: vec!["http://127.0.0.1:8803".into()],
            ttl_secs: 300,
            application: "dev.fungos.files".into(),
            version: "0.1.0".into(),
        }
    }

    fn read(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn publish_preserves_another_applications_observation() {
        let path = temp_file();
        let other = json!({
            "schema_version": SCHEMA_VERSION,
            "service_id": "service/abc123/application/other",
            "observed_at": 900,
            "expires_at": 1_200,
            "provenance": {"provider": "application", "observer": "abc123"},
            "confidence": "strong",
            "value": {"kind": "other", "endpoints": ["http://127.0.0.1:9999"], "attributes": {}},
        });
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "schema_version": SCHEMA_VERSION,
                "observations": [other.clone()],
            }))
            .unwrap(),
        )
        .unwrap();
        lease(&path).publish_at(1_000).unwrap();
        let document = read(&path);
        let observations = document["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(
            observations
                .iter()
                .find(|observation| observation["service_id"] == "service/abc123/application/other"),
            Some(&other)
        );
        let ours = observations
            .iter()
            .find(|observation| observation["service_id"] == "service/abc123/application/files")
            .unwrap();
        assert_eq!(ours["observed_at"], 1_000);
        assert_eq!(ours["expires_at"], 1_300);
        assert_eq!(ours["value"]["endpoints"][0], "http://127.0.0.1:8803");
        assert_eq!(ours["value"]["attributes"]["node_id"], "abc123");
        assert_eq!(ours["provenance"]["observer"], "abc123");
    }

    #[test]
    fn renewal_replaces_only_our_entry() {
        let path = temp_file();
        lease(&path).publish_at(1_000).unwrap();
        lease(&path).publish_at(1_100).unwrap();
        let document = read(&path);
        let observations = document["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0]["observed_at"], 1_100);
        assert_eq!(observations[0]["expires_at"], 1_400);
    }

    #[test]
    fn ttl_is_capped_and_the_file_stays_private() {
        let path = temp_file();
        let mut lease = lease(&path);
        lease.ttl_secs = 9_999;
        lease.publish_at(1_000).unwrap();
        let document = read(&path);
        assert_eq!(
            document["observations"][0]["expires_at"].as_u64().unwrap()
                - document["observations"][0]["observed_at"].as_u64().unwrap(),
            MAX_TTL_SECS
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn rejects_credentials_queries_and_empty_endpoints() {
        let path = temp_file();
        for endpoint in [
            "http://user:secret@host:1",
            "http://host:1/path?query=1",
            "ftp://host:1",
            "not-a-url",
        ] {
            let mut lease = lease(&path);
            lease.endpoints = vec![endpoint.into()];
            assert!(lease.publish_at(1_000).is_err(), "{endpoint}");
        }
        let mut lease = lease(&path);
        lease.endpoints.clear();
        assert!(lease.publish_at(1_000).is_err());
    }

    #[test]
    fn a_malformed_document_is_not_clobbered() {
        let path = temp_file();
        fs::write(&path, b"not json").unwrap();
        assert!(lease(&path).publish_at(1_000).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not json");
    }

    #[test]
    fn an_invalid_observation_is_not_clobbered() {
        let path = temp_file();
        let original = json!({
            "schema_version": SCHEMA_VERSION,
            "observations": [{
                "schema_version": SCHEMA_VERSION,
                "service_id": "service/other/application/files",
                "observed_at": 2_000,
                "expires_at": 2_300,
                "provenance": {"provider": "application", "observer": "other"},
                "confidence": "strong",
                "value": {"kind": "files", "endpoints": ["http://127.0.0.1:1"], "attributes": {}},
            }],
        });
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        assert!(lease(&path).publish_at(1_000).is_err());
        assert_eq!(read(&path), original);
    }
}
