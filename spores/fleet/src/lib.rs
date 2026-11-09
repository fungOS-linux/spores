//! The fleet projection: mycelium peer facts in, one view snapshot out.
//!
//! This is the semantics half of the Spore. The view in `ui/fleet.lua` never
//! counts, sorts or decides what "stale" means; it paints what this module
//! returns. Keeping the classification here means the browser host, a Canvas
//! snapshot and a fixture rendered in CI cannot disagree about which peer is
//! live.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A peer is live when the daemon heard from it within fifteen minutes. This is
/// the same window `mycelium fleet status` uses for a fresheness read, and it is
/// deliberately generous: a sleepy laptop should not become an incident.
pub const LIVE_SECS: i64 = 15 * 60;

/// One row of `mycelium fleet status --json`.
///
/// Every field is optional because the CLI is a projection too: an older
/// installed daemon omits fields a newer normalizer expects. A missing field is
/// unknown, never a zero.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct StatusRow {
    pub node_id: Option<String>,
    pub hostname: Option<String>,
    pub site: Option<String>,
    pub platform: Option<String>,
    pub architecture: Option<String>,
    pub daemon_version: Option<String>,
    pub last_seen: Option<i64>,
    pub load_average: Option<Vec<f64>>,
    pub ssh_listening: Option<bool>,
    pub ssh_verdict: Option<String>,
    pub ssh_certificate: Option<String>,
    pub disk_available_bytes: Option<u64>,
    pub disk_state: Option<String>,
    pub update_state: Option<String>,
    pub release_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Live,
    Stale,
    Unknown,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub host: String,
    pub site: String,
    pub release: Option<String>,
    pub age_secs: i64,
    pub load: f64,
    pub ssh: bool,
    pub auto_update: bool,
    pub state: State,
    /// The site string repeats the hostname. That is a real inventory drift (a
    /// host enrolled without a site), and hiding it would make the site list
    /// look healthier than the fleet is.
    pub site_suspect: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub schema: u32,
    pub title: String,
    pub subtitle: String,
    pub status: String,
    pub captured: String,
    pub counts: Vec<Stat>,
    pub graph: Graph,
    pub sites: Table,
    pub attention: Table,
    pub health: Gauge,
    pub facts: Table,
    pub footer: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stat {
    pub label: String,
    pub value: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Graph {
    pub status: String,
    pub caption: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub legend: Vec<Legend>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub ink: String,
    pub state: String,
    /// Which way the label leaves the dot. A host in a row hangs below; a site
    /// on a spine sits beside. The library draws what it is told rather than
    /// guessing a shape it did not lay out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_position: Option<String>,
    /// Draw a hairline across the plot on this node's row. A layered graph is a
    /// set of rows; the rail is what makes nine of them read as a table of
    /// sites instead of a cloud with a left margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rail: Option<bool>,
    /// Draw the soft disc behind the dot. Only a live row earns one: a halo on
    /// every peer is 23 nodes of glow and a wall that cannot afford its table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halo: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub ink: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Legend {
    pub ink: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub key: String,
    pub label: String,
    pub width: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gauge {
    pub title: String,
    pub value: i64,
    pub max: i64,
    pub status: String,
    pub thresholds: Vec<Threshold>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Threshold {
    pub value: i64,
    pub ink: String,
}

#[derive(Debug, Clone)]
pub struct TopologySummary {
    pub nodes: usize,
    pub segments: usize,
    pub conflicts: usize,
    pub leases: usize,
}

/// Reduce the daemon's topology document to the counts a wall can read. The
/// document itself is megabytes of device detail; none of it belongs in a scene
/// that has to cross a socket.
pub fn summarize(topology: &Value) -> TopologySummary {
    fn objects(value: &Value) -> usize {
        value.as_object().map_or(0, serde_json::Map::len)
    }
    fn arrays(value: &Value) -> usize {
        value.as_array().map_or(0, Vec::len)
    }
    TopologySummary {
        nodes: objects(&topology["nodes"]),
        segments: objects(&topology["segments"]),
        conflicts: arrays(&topology["conflicts"]),
        leases: arrays(&topology["leases"]),
    }
}

fn ceil_div(value: i64, by: i64) -> i64 {
    (value + by - 1) / by
}

/// ISO 8601 UTC without a date crate. The algorithm is Howard Hinnant's
/// `civil_from_days`, the same one the standard library's calendar examples use.
pub fn iso8601_utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{m:02}-{d:02} {:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60
    )
}

fn human_age(seconds: i64) -> String {
    if seconds < 90 {
        "just now".to_string()
    } else if seconds < 3600 {
        format!("{}m", ceil_div(seconds, 60))
    } else if seconds < 86_400 {
        format!("{}h", ceil_div(seconds, 3600))
    } else {
        format!("{}d", ceil_div(seconds, 86_400))
    }
}

/// Semver as a tuple of three numbers. A release the daemon reports but this
/// cannot parse sorts below every parseable one, which makes it visible as
/// drift instead of silently winning the comparison.
fn version_key(value: &str) -> (u64, u64, u64) {
    let mut parts = value.split('.').map(|part| part.parse::<u64>().ok());
    (
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    )
}

const SITE_INK: [&str; 6] = ["info", "success", "warning", "primary", "danger", "muted"];

fn field(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "unknown".to_string())
}

/// A label that fits the column it was laid out in. The full hostname lives in
/// the row data and the attention table, so shortening here costs the graph a
/// suffix, not the reader the identity.
fn short_label(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let prefix: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn short_id(id: &str) -> String {
    // An anonymous row has no identity to shorten; its ordinal is the only
    // thing that distinguishes it, and two rows labelled "anonymou" are two
    // rows an operator cannot tell apart.
    if let Some(rest) = id.strip_prefix("anonymous-") {
        return format!("unnamed-{rest}");
    }
    if id.len() <= 10 {
        id.to_string()
    } else {
        id[..8].to_string()
    }
}

fn peers(status: &[StatusRow], now: i64) -> Vec<Peer> {
    // One row per node identity. The daemon keeps history, so the same peer can
    // appear twice with different `last_seen`; the newest row is the peer.
    let mut newest: Vec<(String, StatusRow)> = Vec::new();
    for (index, row) in status.iter().enumerate() {
        let key = row
            .node_id
            .clone()
            .unwrap_or_else(|| format!("anonymous-{index}"));
        match newest.iter_mut().find(|(seen, _)| *seen == key) {
            Some((_, kept)) if kept.last_seen.unwrap_or(0) < row.last_seen.unwrap_or(0) => {
                *kept = row.clone();
            }
            Some(_) => {}
            None => newest.push((key, row.clone())),
        }
    }

    let mut out = Vec::with_capacity(newest.len());
    for (key, row) in newest {
        let host = field(&row.hostname);
        let site = field(&row.site);
        let site_suspect = row
            .site
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(&host));
        let age_secs = match row.last_seen {
            Some(seen) => (now - seen).max(0),
            None => i64::MAX / 4,
        };
        let state = if row.hostname.is_none() {
            State::Unknown
        } else if age_secs > LIVE_SECS {
            State::Stale
        } else {
            State::Live
        };
        out.push(Peer {
            id: key,
            host,
            site,
            release: row
                .release_version
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            age_secs,
            load: row
                .load_average
                .as_ref()
                .and_then(|values| values.first())
                .copied()
                .unwrap_or(0.0),
            ssh: row.ssh_listening.unwrap_or(false),
            auto_update: row.update_state.as_deref() == Some("active"),
            state,
            site_suspect,
        });
    }
    // Stable order: site, then host. A live view that reorders itself every poll
    // is a view nobody can point at.
    out.sort_by(|left, right| {
        (left.site.to_lowercase(), left.host.to_lowercase())
            .cmp(&(right.site.to_lowercase(), right.host.to_lowercase()))
    });
    out
}

struct SiteGroup {
    name: String,
    peers: Vec<Peer>,
    ink: &'static str,
}

fn group_sites(peers: &[Peer]) -> Vec<SiteGroup> {
    let mut names: Vec<String> = peers.iter().map(|peer| peer.site.clone()).collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    // `unassigned` is a gap in the inventory, not a place. It sorts last and
    // never takes an accent colour away from a real site.
    names.sort_by_key(|name| name.eq_ignore_ascii_case("unassigned"));
    names
        .into_iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let members: Vec<Peer> = peers
                .iter()
                .filter(|peer| peer.site.eq_ignore_ascii_case(&name))
                .cloned()
                .collect();
            if members.is_empty() {
                return None;
            }
            let ink = if name.eq_ignore_ascii_case("unassigned") {
                "muted"
            } else {
                SITE_INK[index % SITE_INK.len()]
            };
            Some(SiteGroup {
                name,
                peers: members,
                ink,
            })
        })
        .collect()
}

fn graph(peers: &[Peer], sites: &[SiteGroup], live: i64) -> Graph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let count = sites.len().max(1) as f64;
    // A layered graph, not a ring: one row per site, hosts in a row to the
    // right. A fleet is read by site and by hostname, and eight hosts on a ring
    // collide while eight in a row do not. There is no fleet root node: every
    // row already belongs to the same fleet, and a hub would only add eight
    // crossing lines to say so.
    let widest = sites
        .iter()
        .map(|site| site.peers.len())
        .max()
        .unwrap_or(1)
        .max(3) as f64;
    let step_x = 0.66 / widest;
    for (index, site) in sites.iter().enumerate() {
        let y = 1.0 - (index as f64 + 0.5) / count;
        let live_here = site
            .peers
            .iter()
            .filter(|peer| peer.state == State::Live)
            .count();
        let state = if live_here > 0 { "live" } else { "stale" };
        let site_id = format!("site:{}", site.name);
        nodes.push(GraphNode {
            id: site_id.clone(),
            label: short_label(&site.name, 16),
            x: 0.20,
            y,
            ink: site.ink.into(),
            state: state.into(),
            // The site name is the row's left column; hosts start to its right.
            // A label on the right would land on the first host dot.
            label_position: Some("left".into()),
            rail: Some(true),
            halo: Some(live_here > 0),
        });
        for (member_index, peer) in site.peers.iter().enumerate() {
            let id = format!("peer:{}", peer.id);
            let ink = if peer.state == State::Live {
                site.ink
            } else {
                "muted"
            };
            // An anonymous row has no hostname to print twice; its ordinal is
            // the only label that tells the two dots apart.
            let label = if peer.state == State::Unknown {
                // `unnamed-0` is a name, and nine characters is one more than
                // the row can hold at eight hosts.
                short_id(&peer.id).replace("unnamed-", "anon-")
            } else {
                short_label(&peer.host, 8)
            };
            nodes.push(GraphNode {
                id: id.clone(),
                label,
                x: 0.32 + member_index as f64 * step_x,
                y,
                ink: ink.into(),
                state: peer.state.as_str().into(),
                label_position: Some("below".into()),
                rail: None,
                halo: Some(peer.state == State::Live),
            });
            edges.push(GraphEdge {
                source: site_id.clone(),
                target: id,
                ink: ink.into(),
                state: peer.state.as_str().into(),
            });
        }
    }
    Graph {
        status: format!("{live} live"),
        caption: format!("{} peers · {} sites", peers.len(), sites.len()),
        nodes,
        edges,
        legend: vec![
            Legend {
                ink: "success".into(),
                label: "live".into(),
            },
            Legend {
                ink: "muted".into(),
                label: "stale".into(),
            },
        ],
    }
}

fn sites_table(sites: &[SiteGroup]) -> Table {
    let rows = sites
        .iter()
        .map(|site| {
            let total = site.peers.len() as i64;
            let live = site
                .peers
                .iter()
                .filter(|peer| peer.state == State::Live)
                .count() as i64;
            let stale = site
                .peers
                .iter()
                .filter(|peer| peer.state == State::Stale)
                .count() as i64;
            let unknown = site
                .peers
                .iter()
                .filter(|peer| peer.state == State::Unknown)
                .count() as i64;
            let mut state = if live == 0 {
                "crit"
            } else if stale + unknown > 0 {
                "warn"
            } else {
                "ok"
            };
            if site.peers.iter().any(|peer| peer.site_suspect) && state == "ok" {
                state = "warn";
            }
            let mut drift = match (stale, unknown) {
                (0, 0) => "—".to_string(),
                (stale, 0) => format!("{stale} stale"),
                (0, unknown) => format!("{unknown} unknown"),
                (stale, unknown) => format!("{stale} stale · {unknown} unknown"),
            };
            // A site that repeats its hostname was enrolled without one. That
            // is a gap in the inventory, and the row says so rather than
            // counting it as a place with one peer.
            if site.peers.iter().any(|peer| peer.site_suspect) {
                drift = format!("{drift} · site=host");
            }
            serde_json::json!({
                "site": &site.name,
                "live": format!("{live}/{total}"),
                "drift": drift,
                "state": state,
            })
        })
        .collect();
    Table {
        columns: vec![
            Column {
                key: "site".into(),
                label: "Site".into(),
                width: 1.6,
                align: None,
                kind: None,
            },
            Column {
                key: "live".into(),
                label: "Live".into(),
                width: 0.8,
                align: Some("right".into()),
                kind: None,
            },
            Column {
                key: "drift".into(),
                label: "Drift".into(),
                width: 1.4,
                align: None,
                kind: None,
            },
            Column {
                key: "state".into(),
                label: "State".into(),
                width: 0.8,
                align: None,
                kind: Some("status".into()),
            },
        ],
        rows,
    }
}

fn attention_table(peers: &[Peer], topology: Option<&TopologySummary>) -> Table {
    let mut findings: Vec<(u8, Value)> = Vec::new();
    for peer in peers {
        match peer.state {
            State::Unknown => findings.push((
                0,
                serde_json::json!({
                    "host": short_id(&peer.id),
                    "finding": "daemon has no hostname",
                    "level": "crit",
                }),
            )),
            State::Stale => findings.push((
                1,
                serde_json::json!({
                    "host": &peer.host,
                    "finding": format!("{} silent in {}", human_age(peer.age_secs), peer.site),
                    "level": "warn",
                }),
            )),
            State::Live => {}
        }
    }
    if let Some(topology) = topology
        && topology.conflicts > 0
    {
        findings.push((
            0,
            serde_json::json!({
                "host": "mesh",
                "finding": format!("{} topology conflicts", topology.conflicts),
                "level": "crit",
            }),
        ));
    }
    findings.sort_by_key(|(rank, row)| (*rank, row["finding"].as_str().unwrap_or("").to_string()));
    let rows = findings.into_iter().take(4).map(|(_, row)| row).collect();
    Table {
        columns: vec![
            Column {
                key: "host".into(),
                label: "Host".into(),
                width: 1.1,
                align: None,
                kind: None,
            },
            Column {
                key: "finding".into(),
                label: "Finding".into(),
                width: 2.4,
                align: None,
                kind: None,
            },
            Column {
                key: "level".into(),
                label: "".into(),
                width: 0.5,
                align: None,
                kind: Some("status".into()),
            },
        ],
        rows,
    }
}

fn facts_table(topology: Option<&TopologySummary>) -> Table {
    let rows = match topology {
        Some(topology) => vec![
            serde_json::json!({ "fact": "topology nodes", "value": topology.nodes }),
            serde_json::json!({ "fact": "segments", "value": topology.segments }),
            serde_json::json!({ "fact": "conflicts", "value": topology.conflicts }),
        ],
        None => vec![serde_json::json!({ "fact": "topology", "value": "not captured" })],
    };
    Table {
        columns: vec![
            Column {
                key: "fact".into(),
                label: "Fact".into(),
                width: 1.6,
                align: None,
                kind: None,
            },
            Column {
                key: "value".into(),
                label: "Observed".into(),
                width: 0.8,
                align: Some("right".into()),
                kind: None,
            },
        ],
        rows,
    }
}

/// Fold peer rows and an optional topology document into one view snapshot.
///
/// `now` is a parameter rather than a clock read so a fixture, a test and the
/// wall can all ask the same question at the same instant.
pub fn build(status: &[StatusRow], topology: Option<&Value>, now: i64) -> Snapshot {
    let peers = peers(status, now);
    let sites = group_sites(&peers);
    let summary = topology.map(summarize);
    let total = peers.len() as i64;
    let live = peers
        .iter()
        .filter(|peer| peer.state == State::Live)
        .count() as i64;
    let stale = peers
        .iter()
        .filter(|peer| peer.state == State::Stale)
        .count() as i64;
    let unknown = peers
        .iter()
        .filter(|peer| peer.state == State::Unknown)
        .count() as i64;
    let ssh = peers.iter().filter(|peer| peer.ssh).count() as i64;
    let updating = peers.iter().filter(|peer| peer.auto_update).count() as i64;
    let recorded = peers.iter().filter(|peer| peer.release.is_some()).count() as i64;
    let latest = peers
        .iter()
        .filter_map(|peer| peer.release.as_deref())
        .max_by_key(|release| version_key(release));
    let on_latest = latest.map_or(0, |latest| {
        peers
            .iter()
            .filter(|peer| peer.release.as_deref() == Some(latest))
            .count() as i64
    });
    let unassigned = peers
        .iter()
        .filter(|peer| peer.site.eq_ignore_ascii_case("unassigned"))
        .count() as i64;
    let captured = iso8601_utc(now);
    let status = if total == 0 {
        "unknown"
    } else if stale + unknown == 0 {
        "live"
    } else if live > 0 {
        "degraded"
    } else {
        "stale"
    };

    let counts = vec![
        Stat {
            label: "Peers live".into(),
            value: live,
            unit: Some(format!("of {total}")),
            delta: Some(if stale + unknown == 0 {
                "all accounted for".to_string()
            } else {
                format!("{stale} stale · {unknown} unknown")
            }),
            status: if live == 0 {
                "crit".into()
            } else if stale + unknown > 0 {
                "warn".into()
            } else {
                "live".into()
            },
        },
        Stat {
            label: "Sites".into(),
            value: sites.len() as i64,
            unit: Some("observed".into()),
            delta: Some(if unassigned > 0 {
                format!("{unassigned} unassigned")
            } else {
                "every peer has a site".to_string()
            }),
            status: if unassigned > 0 {
                "warn".into()
            } else {
                "ok".into()
            },
        },
        Stat {
            label: "SSH reachable".into(),
            value: ssh,
            unit: Some("hosts".into()),
            delta: Some(format!("{updating} on auto-update")),
            status: if ssh == 0 { "crit".into() } else { "ok".into() },
        },
        Stat {
            label: "Topology conflicts".into(),
            value: summary.as_ref().map_or(0, |summary| summary.conflicts) as i64,
            unit: summary.as_ref().map(|_| "reported".to_string()),
            delta: summary
                .as_ref()
                .map(|summary| format!("{} leases", summary.leases)),
            status: if summary
                .as_ref()
                .is_some_and(|summary| summary.conflicts > 0)
            {
                "crit".into()
            } else {
                "ok".into()
            },
        },
    ];

    let graph = graph(&peers, &sites, live);
    let attention = attention_table(&peers, summary.as_ref());
    let coverage = if total == 0 {
        0
    } else {
        recorded * 100 / total
    };
    let health = Gauge {
        title: "Release coverage".into(),
        value: recorded,
        max: total.max(1),
        status: if coverage < 60 {
            "warn".into()
        } else if coverage < 90 {
            "ok".into()
        } else {
            "live".into()
        },
        thresholds: vec![
            Threshold {
                value: 60,
                ink: "warning".into(),
            },
            Threshold {
                value: 90,
                ink: "success".into(),
            },
        ],
    };
    let latest_text = latest.unwrap_or("unrecorded");

    Snapshot {
        schema: 1,
        title: "FLEET".into(),
        subtitle: "mycelium · fungOS".into(),
        status: status.into(),
        captured: captured.clone(),
        counts,
        graph,
        sites: sites_table(&sites),
        attention,
        health,
        facts: facts_table(summary.as_ref()),
        footer: format!(
            "fleet.myc · mycelium fleet status · {total} peers · latest release {latest_text} ({on_latest}) · {captured}"
        ),
    }
}

// ---- Lua projection ----------------------------------------------------

fn is_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|char| char.is_ascii_alphanumeric() || char == '_')
}

fn lua_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for char in value.chars() {
        match char {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            char if (char as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", char as u32)),
            char => out.push(char),
        }
    }
    out.push('"');
    out
}

fn lua_value(value: &Value, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);
    match value {
        Value::Null => "nil".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => lua_string(text),
        Value::Array(items) => {
            if items.is_empty() {
                return "{}".into();
            }
            let flat = items
                .iter()
                .map(|item| lua_value(item, depth + 1))
                .collect::<Vec<_>>()
                .join(", ");
            if !flat.contains('\n') && flat.len() <= 96 {
                return format!("{{ {flat} }}");
            }
            let body = items
                .iter()
                .map(|item| format!("{inner}{},", lua_value(item, depth + 1)))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{{\n{body}\n{pad}}}")
        }
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".into();
            }
            let pairs = |depth: usize| {
                map.iter()
                    .map(|(key, item)| {
                        let key = if is_identifier(key) {
                            key.clone()
                        } else {
                            format!("[{}]", lua_string(key))
                        };
                        format!("{key} = {}", lua_value(item, depth))
                    })
                    .collect::<Vec<_>>()
            };
            let flat = pairs(depth + 1).join(", ");
            if !flat.contains('\n') && flat.len() <= 96 {
                return format!("{{ {flat} }}");
            }
            let body = pairs(depth + 1)
                .iter()
                .map(|pair| format!("{inner}{pair},"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{{\n{body}\n{pad}}}")
        }
    }
}

/// The snapshot as a Lua chunk, which is what `tools/shot.lua` loads. JSON would
/// need a decoder in the sandbox; a Lua literal is the sandbox's own language and
/// stays inspectable when someone asks what the wall was told.
pub fn to_lua(snapshot: &Snapshot) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(snapshot)?;
    Ok(format!(
        "-- generated by `fleet-serve`; do not edit\nreturn {}\n",
        lua_value(&value, 0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(host: &str, site: &str, age: i64) -> StatusRow {
        StatusRow {
            node_id: Some(host.to_string()),
            hostname: Some(host.to_string()),
            site: Some(site.to_string()),
            last_seen: Some(1_000_000 - age),
            ..StatusRow::default()
        }
    }

    #[test]
    fn classifies_live_stale_and_unknown() {
        let rows = vec![
            row("a", "home", 10),
            row("b", "home", LIVE_SECS + 1),
            StatusRow {
                node_id: Some("ghost".into()),
                last_seen: Some(1_000_000),
                ..StatusRow::default()
            },
        ];
        let snapshot = build(&rows, None, 1_000_000);
        assert_eq!(snapshot.counts[0].value, 1);
        let states: Vec<&str> = snapshot
            .graph
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("peer:"))
            .map(|node| node.state.as_str())
            .collect();
        assert!(states.contains(&"live"));
        assert!(states.contains(&"stale"));
        assert!(states.contains(&"unknown"));
    }

    #[test]
    fn newest_row_wins_for_a_repeated_node() {
        let mut old = row("a", "home", 900);
        old.last_seen = Some(1);
        let mut new = row("a", "home", 1);
        new.last_seen = Some(999);
        let snapshot = build(&[old, new], None, 1_000);
        assert_eq!(snapshot.counts[0].value, 1);
        assert_eq!(snapshot.sites.rows.len(), 1);
    }

    #[test]
    fn summarizes_topology_without_copying_it() {
        let topology = serde_json::json!({
            "nodes": { "a": {}, "b": {} },
            "segments": { "s": {} },
            "conflicts": [{}, {}, {}],
            "leases": [{}],
        });
        let summary = summarize(&topology);
        assert_eq!(summary.nodes, 2);
        assert_eq!(summary.segments, 1);
        assert_eq!(summary.conflicts, 3);
        assert_eq!(summary.leases, 1);
    }

    #[test]
    fn emits_a_loadable_lua_chunk() {
        let snapshot = build(&[row("a", "home", 1)], None, 1_000_000);
        let lua = to_lua(&snapshot).unwrap();
        assert!(lua.starts_with("-- generated by `fleet-serve`; do not edit\nreturn {"));
        assert!(lua.contains("label = \"Peers live\""));
        assert!(lua.contains("title = \"FLEET\""));
    }

    #[test]
    fn formats_utc_without_a_date_crate() {
        assert_eq!(iso8601_utc(0), "1970-01-01 00:00Z");
        assert_eq!(iso8601_utc(1_700_000_000), "2023-11-14 22:13Z");
    }
}
