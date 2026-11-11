//! Load a chrome region from a Spore package and render it.
//!
//! This is the host side of `[chrome]`. `soil-spore` validates a manifest that
//! claims a region, and the fallback rule says a broken Spore must never leave a
//! hole in the desktop; until now nothing implemented either half. This crate is
//! the smallest thing that does:
//!
//! 1. read `spore.toml` and take the region claim from it, not from a path,
//! 2. resolve the region's Lua the way the manifest says (`[chrome.sources]`,
//!    else `[ui] entry`),
//! 3. run it in a Lua VM with the canvas ABI, a `require` for the UI SDK, and
//!    `__soil.invoke` gated by `[grants]`,
//! 4. hand back the scene, or an error the caller can fall back on.
//!
//! The point is not to be Canvas. It is to make a chrome Spore loadable and
//! checkable outside a compositor, so a package can be proven before a session
//! depends on it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use mlua::{Lua, Table, Value};

/// Chrome regions a Spore may claim, mirroring `soil_spore::CHROME_REGIONS`.
pub const REGIONS: [&str; 7] = [
    "top-bar",
    "dock",
    "launcher",
    "switcher",
    "control-center",
    "notifications",
    "attention",
];

/// `mlua::Error` is not `Send + Sync`, so `anyhow` cannot absorb it with `?`.
/// Converting to a string keeps one error type across the host.
fn lua(error: mlua::Error) -> anyhow::Error {
    anyhow!("{error}")
}

/// A Spore package that draws a slice of the shell.
#[derive(Debug, Clone)]
pub struct ChromeSpore {
    pub dir: PathBuf,
    pub id: String,
    pub version: String,
    pub regions: Vec<String>,
    /// Region -> Lua file, resolved and verified to exist.
    pub sources: BTreeMap<String, PathBuf>,
    /// The authored view (`[ui] entry`), which is what `compose` reads.
    pub entry: Option<PathBuf>,
    /// Manifest grants, as `soil:time` style keys.
    pub grants: BTreeSet<String>,
    /// Build-time dependencies and the exact version each was built against.
    pub deps: BTreeMap<String, String>,
}

impl ChromeSpore {
    /// Read a package directory.
    ///
    /// # Errors
    /// Fails when the manifest is unreadable, malformed, claims no region, or
    /// names a region's Lua that is not on disk.
    pub fn load(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join("spore.toml");
        let text = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?;
        let manifest: Manifest =
            toml::from_str(&text).with_context(|| format!("parse {}", manifest_path.display()))?;

        let chrome = manifest
            .chrome
            .ok_or_else(|| anyhow!("{} claims no chrome region", manifest.spore.id))?;
        if chrome.regions.is_empty() {
            bail!("{} declares `[chrome]` but no regions", manifest.spore.id);
        }
        for region in &chrome.regions {
            if !REGIONS.contains(&region.as_str()) {
                bail!(
                    "{} claims unknown chrome region {region:?}; known: {}",
                    manifest.spore.id,
                    REGIONS.join(", ")
                );
            }
        }

        let entry = manifest.ui.as_ref().map(|ui| ui.entry.clone());
        let entry_path = entry
            .as_ref()
            .map(|relative| dir.join(relative))
            .filter(|path| path.is_file());
        let mut sources = BTreeMap::new();
        for region in &chrome.regions {
            let relative = chrome
                .sources
                .get(region)
                .cloned()
                .or_else(|| entry.clone())
                .ok_or_else(|| {
                    anyhow!(
                        "{} claims {region} with no Lua: add [ui] entry or [chrome.sources]",
                        manifest.spore.id
                    )
                })?;
            let path = dir.join(&relative);
            if !path.is_file() {
                bail!(
                    "{} declares {relative} for {region}, but it is missing",
                    manifest.spore.id
                );
            }
            sources.insert(region.clone(), path);
        }

        Ok(Self {
            dir: dir.to_path_buf(),
            id: manifest.spore.id,
            version: manifest.spore.version,
            regions: chrome.regions,
            sources,
            entry: entry_path,
            grants: manifest.grants.into_keys().collect(),
            deps: manifest.deps,
        })
    }

    /// The Lua this Spore draws `region` with.
    #[must_use]
    pub fn source_for(&self, region: &str) -> Option<&Path> {
        self.sources.get(region).map(PathBuf::as_path)
    }

    /// Whether the manifest grants `capability`, e.g. `soil:time`.
    #[must_use]
    pub fn grants(&self, capability: &str) -> bool {
        self.grants.contains(capability)
    }
}

#[derive(serde::Deserialize)]
struct Manifest {
    spore: SporeMeta,
    #[serde(default)]
    ui: Option<Ui>,
    #[serde(default)]
    grants: BTreeMap<String, String>,
    #[serde(default)]
    deps: BTreeMap<String, String>,
    #[serde(default)]
    chrome: Option<Chrome>,
}

#[derive(serde::Deserialize)]
struct SporeMeta {
    id: String,
    version: String,
}

#[derive(serde::Deserialize)]
struct Ui {
    entry: String,
}

#[derive(serde::Deserialize)]
struct Chrome {
    regions: Vec<String>,
    #[serde(default)]
    sources: BTreeMap<String, String>,
}

/// One drawn primitive, exactly as the Lua ABI emitted it.
pub type Scene = Vec<serde_json::Value>;

/// A Lua VM wired to the canvas ABI, the UI SDK and a grant-checked `__soil`.
pub struct Host {
    lua: Lua,
    width: f32,
    height: f32,
}

impl Host {
    /// Build a VM for a surface of `width` x `height`.
    ///
    /// # Errors
    /// Fails when the Lua state cannot be created or the ABI cannot be installed.
    pub fn new(
        width: f32,
        height: f32,
        sdk: Option<&Path>,
        grants: &BTreeSet<String>,
    ) -> Result<Self> {
        let host = Self {
            lua: Lua::new(),
            width,
            height,
        };
        host.install_canvas().map_err(lua)?;
        host.install_runtime(sdk, grants).map_err(lua)?;
        Ok(host)
    }

    /// Load a snapshot file into this host's Lua.
    ///
    /// A table belongs to the Lua state that made it, so the fixture has to be
    /// evaluated here rather than in a scratch VM and handed across.
    ///
    /// # Errors
    /// Fails when the file cannot be read or does not return a table.
    pub fn load_snapshot(&self, path: Option<&Path>) -> Result<Table> {
        let Some(path) = path else {
            return self.lua.create_table().map_err(lua);
        };
        let source =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let value: Value = self
            .lua
            .load(&source)
            .set_name(path.display().to_string())
            .eval()
            .map_err(lua)
            .with_context(|| format!("{} must return a table", path.display()))?;
        match value {
            Value::Table(table) => Ok(table),
            other => bail!(
                "{} returned {}, expected a table",
                path.display(),
                other.type_name()
            ),
        }
    }

    /// Render `spore` with `snapshot` and return its primitives.
    ///
    /// # Errors
    /// Fails when the Lua does not load, does not expose `render`, or raises.
    pub fn render(&self, spore: &ChromeSpore, region: &str, snapshot: Table) -> Result<Scene> {
        let path = spore
            .source_for(region)
            .ok_or_else(|| anyhow!("{} does not claim {region}", spore.id))?;
        let source =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

        let module: Value = self
            .lua
            .load(&source)
            .set_name(path.display().to_string())
            .eval()
            .map_err(lua)
            .with_context(|| format!("{} does not return a module", path.display()))?;

        let render: mlua::Function = match &module {
            Value::Table(table) => table
                .get::<mlua::Function>("render")
                .map_err(lua)
                .with_context(|| {
                    format!(
                        "{} returns a table with no render(snapshot)",
                        path.display()
                    )
                })?,
            Value::Function(function) => function.clone(),
            other => bail!(
                "{} returned {} instead of a module or a function",
                path.display(),
                other.type_name()
            ),
        };

        self.reset_scene().map_err(lua)?;
        fill_clock(&self.lua, &snapshot).map_err(lua)?;
        render.call::<()>(snapshot).map_err(lua)?;
        self.scene()
    }

    fn reset_scene(&self) -> mlua::Result<()> {
        let scene: Table = self.lua.globals().get("__scene")?;
        scene.set("nodes", self.lua.create_table()?)
    }

    fn scene(&self) -> Result<Scene> {
        let scene: Table = self.lua.globals().get("__scene").map_err(lua)?;
        let nodes: Table = scene.get("nodes").map_err(lua)?;
        let mut out = Vec::new();
        for value in nodes.sequence_values::<Table>() {
            out.push(lua_to_json(&value.map_err(lua)?)?);
        }
        Ok(out)
    }

    fn install_canvas(&self) -> mlua::Result<()> {
        let lua = &self.lua;
        let scene = lua.create_table()?;
        scene.set("schema", 1)?;
        scene.set("nodes", lua.create_table()?)?;
        lua.globals().set("__scene", &scene)?;

        let canvas = lua.create_table()?;
        let rect_scene = scene.clone();
        canvas.set(
            "rect",
            lua.create_function(move |lua, spec: Table| {
                push_node(lua, &rect_scene, "rect", &spec)
            })?,
        )?;
        let text_scene = scene;
        canvas.set(
            "text",
            lua.create_function(move |lua, spec: Table| {
                push_node(lua, &text_scene, "text", &spec)
            })?,
        )?;
        canvas.set(
            "measure_text",
            lua.create_function(|lua, (text, style): (String, Option<Table>)| {
                let size = style
                    .and_then(|style| style.get::<Option<f32>>("size").ok().flatten())
                    .unwrap_or(10.0);
                let table = lua.create_table()?;
                table.set("width", text.chars().count() as f32 * size * 0.55)?;
                table.set("height", size)?;
                Ok(table)
            })?,
        )?;
        lua.globals().set("canvas", canvas)?;

        let widget = lua.create_table()?;
        widget.set("hit", lua.create_function(|_, _: Table| Ok(()))?)?;
        lua.globals().set("widget", widget)?;

        let module = lua.create_table()?;
        let viewport = lua.create_table()?;
        viewport.set("width", self.width)?;
        viewport.set("height", self.height)?;
        module.set("viewport", viewport)?;
        let theme = lua.create_table()?;
        theme.set("body", 16.0)?;
        theme.set("small", 13.0)?;
        theme.set("micro", 10.0)?;
        theme.set("hero", 24.0)?;
        theme.set("radius", 10.0)?;
        module.set("theme", theme)?;
        lua.globals().set("module", module)
    }

    fn install_runtime(&self, sdk: Option<&Path>, grants: &BTreeSet<String>) -> mlua::Result<()> {
        let lua = &self.lua;

        // An authored view imports the SDK at build time; a composed artifact
        // has its imports inlined and needs nothing. Resolving here keeps both
        // loadable, and a missing SDK fails at the import, not at startup.
        match sdk {
            Some(sdk_dir) => {
                let sdk_dir = sdk_dir.to_path_buf();
                lua.globals().set(
                    "require",
                    lua.create_function(move |lua, name: String| {
                        let path = sdk_dir
                            .join("lua")
                            .join(format!("{}.lua", name.replace('.', "/")));
                        let source = std::fs::read_to_string(&path).map_err(|error| {
                            mlua::Error::runtime(format!("{name}: {}: {error}", path.display()))
                        })?;
                        lua.load(&source)
                            .set_name(path.display().to_string())
                            .eval::<Value>()
                    })?,
                )?;
            }
            None => {
                lua.globals().set(
                    "require",
                    lua.create_function(|_, name: String| -> mlua::Result<Value> {
                        Err(mlua::Error::runtime(format!(
                            "`{name}` is not composed into this artifact and no SDK is loaded"
                        )))
                    })?,
                )?;
            }
        }

        // Every ABI is installed; only granted calls succeed. A denial is a
        // refusal at the call site that names the grant, not a missing table.
        let granted = grants.clone();
        let soil = lua.create_table()?;
        let denied = granted.clone();
        soil.set(
            "invoke",
            lua.create_function(
                move |_,
                      (grant, interface, function, _args): (String, String, String, Table)|
                      -> mlua::Result<()> {
                    if !denied.contains(&grant) {
                        return Err(mlua::Error::runtime(format!(
                            "denied: {interface}::{function} needs the `{grant}` grant"
                        )));
                    }
                    Err(mlua::Error::runtime(format!(
                        "unimplemented: {interface}::{function} (`{grant}` is granted)"
                    )))
                },
            )?,
        )?;
        soil.set(
            "granted",
            lua.create_function(move |_, capability: String| Ok(granted.contains(&capability)))?,
        )?;
        lua.globals().set("__soil", soil)
    }
}

/// The host owns the wall clock, so a view never formats one: a fixture may
/// supply `ambient.clock`, and when it does not the platform supplies it.
fn platform_clock() -> Option<String> {
    let output = std::process::Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn fill_clock(lua: &Lua, snapshot: &Table) -> mlua::Result<()> {
    let ambient: Table = match snapshot.get::<Value>("ambient")? {
        Value::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            snapshot.set("ambient", &table)?;
            table
        }
    };
    let now = lua.create_table()?;
    now.set("epoch_ms", epoch_ms())?;
    ambient.set("now", now)?;
    let has_clock = matches!(ambient.get::<Value>("clock")?, Value::String(_));
    if !has_clock && let Some(clock) = platform_clock() {
        ambient.set("clock", clock)?;
    }
    Ok(())
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn push_node(lua: &Lua, scene: &Table, kind: &str, spec: &Table) -> mlua::Result<()> {
    let node = lua.create_table()?;
    node.set("type", kind)?;
    for key in ["id", "ink", "text", "opacity", "radius"] {
        if let Ok(value) = spec.get::<Value>(key)
            && !matches!(value, Value::Nil)
        {
            node.set(key, value)?;
        }
    }
    // The ABI passes translation and scale flat; a scene document keeps them in
    // one transform, which is what `soil_scene` readers expect.
    let transform = lua.create_table()?;
    for key in ["translation", "scale"] {
        if let Ok(value) = spec.get::<Value>(key)
            && !matches!(value, Value::Nil)
        {
            transform.set(key, value)?;
        }
    }
    node.set("transform", transform)?;
    if let Ok(Value::Table(rgb)) = spec.get::<Value>("rgb") {
        node.set("rgb", rgb)?;
    }
    let nodes: Table = scene.get("nodes")?;
    nodes.raw_set(nodes.raw_len() + 1, node)?;
    Ok(())
}

fn lua_to_json(value: &Table) -> Result<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for pair in value.pairs::<Value, Value>() {
        let (key, value) = pair.map_err(lua)?;
        let key = match key {
            Value::String(text) => text.to_str().map_err(lua)?.to_owned(),
            other => bail!("node key is {}, expected a string", other.type_name()),
        };
        map.insert(key, single(value)?);
    }
    Ok(serde_json::Value::Object(map))
}

fn single(value: Value) -> Result<serde_json::Value> {
    Ok(match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(flag) => serde_json::Value::Bool(flag),
        Value::Integer(number) => serde_json::json!(number),
        Value::Number(number) => serde_json::json!(number),
        Value::String(text) => serde_json::Value::String(text.to_str().map_err(lua)?.to_owned()),
        Value::Table(table) => {
            let len = table.raw_len();
            let numeric =
                len > 0 && (1..=len).all(|index| table.contains_key(index).unwrap_or(false));
            if numeric {
                let mut out = Vec::new();
                for index in 1..=len {
                    out.push(single(table.get(index).map_err(lua)?)?);
                }
                serde_json::Value::Array(out)
            } else {
                lua_to_json(&table)?
            }
        }
        other => bail!("cannot encode {}", other.type_name()),
    })
}

/// Compose a region's Lua into one self-contained module.
///
/// The host does not compose at runtime, and Canvas's scene VM has no `require`
/// global at all, so a package that ships only its authored view is not loadable
/// by a compositor. This is the build step: every `require` is inlined, the
/// snapshot is *not*, and the result returns the view module so a host can call
/// `render(snapshot)` with live state.
///
/// ```sh
/// chrome-host compose spores/divebar --sdk ../fungos-sdk --out dist/top-bar.lua
/// ```
///
/// # Errors
/// Fails when the region source or one of its imports cannot be read, or when the
/// graph has a cycle the composer cannot linearise.
pub fn compose(spore: &ChromeSpore, region: &str, sdk: &Path) -> Result<String> {
    // Compose always starts from the authored view, never from a previously
    // composed artifact: composing the artifact would inline an already-inlined
    // SDK, and the output would grow on every run.
    let path = spore
        .entry
        .as_deref()
        .or_else(|| spore.source_for(region))
        .ok_or_else(|| anyhow!("{} has no view to compose for {region}", spore.id))?;
    let source =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    // Transitive: a SDK module may itself import another.
    let mut order: Vec<(String, String)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = imports_of(&source);
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let file = sdk
            .join("lua")
            .join(format!("{}.lua", name.replace('.', "/")));
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("{name}: {}", file.display()))?;
        for nested in imports_of(&text) {
            if !seen.contains(&nested) {
                stack.push(nested);
            }
        }
        order.push((name, text));
    }
    order.sort_by(|left, right| left.0.cmp(&right.0));

    // A composed artifact pins its imports by construction, so the manifest may
    // state the version it was built against and the builder holds it to that.
    let sdk_version = sdk_version(sdk)?;
    if let Some(declared) = spore.deps.get("fungos.ui")
        && declared != &sdk_version
    {
        bail!(
            "{} declares fungos.ui {declared}, but {} is {sdk_version}",
            spore.id,
            sdk.display()
        );
    }

    let mut out = String::new();
    out.push_str(&format!(
        "-- chrome-composed: 1\n\
         -- Composed by chrome-host from {} {} (region {region}).\n\
         -- Imports: fungos.ui {sdk_version}\n\
         -- Do not edit: run `chrome-host compose` again. No snapshot is baked in,\n\
         -- so a host calls render(snapshot) with live state.\n\
         --\n\
         -- Self-contained: every import is inlined, and the `require` below is a\n\
         -- local parameter, so a host that deletes the global still runs this.\n\n",
        spore.id, spore.version
    ));
    out.push_str("local __modules = {\n");
    for (name, text) in &order {
        out.push_str(&format!("[{name:?}] = function(require)\n{text}\nend,\n"));
    }
    out.push_str("}\n\nlocal __cache = {}\nlocal function __require(name)\n  if __cache[name] then return __cache[name] end\n  local loader = __modules[name]\n  if not loader then error(\"composed module has no \" .. tostring(name)) end\n  local module = loader(__require)\n  __cache[name] = module\n  return module\nend\n\n");
    out.push_str("local view = (function(require)\n");
    out.push_str(&source);
    out.push_str("\nend)(__require)\n\nreturn view\n");
    Ok(out)
}

/// Module names a source imports, in source order.
fn imports_of(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while let Some(found) = source[index..].find("require") {
        let start = index + found + "require".len();
        index = start;
        let rest = &source[start..];
        let Some(open) = rest.find('(') else { continue };
        let after = &rest[open + 1..];
        let trimmed = after.trim_start();
        let Some(quote) = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let Some(end) = trimmed[1..].find(quote) else {
            continue;
        };
        let name = &trimmed[1..1 + end];
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            continue;
        }
        out.push(name.to_owned());
    }
    let _ = bytes;
    out
}

/// The `fungos.ui` version a checkout declares.
fn sdk_version(sdk: &Path) -> Result<String> {
    let path = sdk.join("lua/fungos/ui.lua");
    let source =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let marker = "ui.version";
    let start = source
        .find(marker)
        .ok_or_else(|| anyhow!("{} declares no ui.version", path.display()))?;
    let rest = &source[start + marker.len()..];
    let quote = rest
        .find(['"', '\''])
        .ok_or_else(|| anyhow!("{} has a malformed ui.version", path.display()))?;
    let after = &rest[quote + 1..];
    let end = after
        .find(['"', '\''])
        .ok_or_else(|| anyhow!("{} has a malformed ui.version", path.display()))?;
    Ok(after[..end].to_owned())
}
