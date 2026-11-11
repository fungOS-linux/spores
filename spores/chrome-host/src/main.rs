//! `chrome-host`: render a chrome region from a Spore package.
//!
//! ```text
//! chrome-host spores/divebar --sdk ../fungos-sdk --scene /tmp/top-bar.json
//! chrome-host spores/divebar --region top-bar --snapshot ui/snapshot.lua
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use chrome_host::{ChromeSpore, Host, compose};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("chrome-host: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut command = "render".to_owned();
    let mut spore_dir: Option<PathBuf> = None;
    if let Some(first) = args.next() {
        if first == "compose" || first == "render" {
            command = first;
            spore_dir = args.next().map(PathBuf::from);
        } else {
            spore_dir = Some(PathBuf::from(first));
        }
    }
    let mut region = "top-bar".to_owned();
    let mut sdk = std::env::var("FUNGOS_SDK").ok().map(PathBuf::from);
    let mut snapshot_path: Option<PathBuf> = None;
    let mut scene_path: Option<PathBuf> = None;
    let mut width = 1200.0_f32;
    let mut height = 44.0_f32;
    let mut fixture = true;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--region" => {
                region = args
                    .next()
                    .ok_or_else(|| anyhow!("--region needs a value"))?
            }
            "--sdk" => sdk = args.next().map(PathBuf::from),
            "--snapshot" => snapshot_path = args.next().map(PathBuf::from),
            "--scene" | "--out" => scene_path = args.next().map(PathBuf::from),
            "--width" => {
                width = args
                    .next()
                    .ok_or_else(|| anyhow!("--width needs a value"))?
                    .parse()?
            }
            "--height" => {
                height = args
                    .next()
                    .ok_or_else(|| anyhow!("--height needs a value"))?
                    .parse()?
            }
            // A package's ui/snapshot.lua is a fixture. Worth having for a quick
            // look, worth refusing when the point is live state.
            "--no-snapshot" => fixture = false,
            other => {
                if spore_dir.is_some() {
                    bail!("unknown flag {other}");
                }
                spore_dir = Some(PathBuf::from(other));
            }
        }
    }

    let spore_dir = spore_dir.ok_or_else(|| anyhow!("usage: chrome-host <spore-dir> [--region top-bar] [--sdk <dir>] [--snapshot <file.lua>] [--scene <out.json>]"))?;
    let spore = ChromeSpore::load(&spore_dir)?;
    if !spore.regions.iter().any(|claimed| claimed == &region) {
        bail!("{} claims {:?}, not {region}", spore.id, spore.regions);
    }

    if let Some(path) = &sdk
        && !path.join("lua/fungos/ui.lua").is_file()
    {
        bail!("{} has no lua/fungos/ui.lua", path.display());
    }

    if command == "compose" {
        let sdk = sdk.as_ref().ok_or_else(|| {
            anyhow!("compose needs the SDK it inlines: pass --sdk <fungos-sdk checkout>")
        })?;
        let composed = compose(&spore, &region, sdk)?;
        let out =
            scene_path.unwrap_or_else(|| spore.dir.join("dist").join(format!("{region}.lua")));
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(&out, &composed).with_context(|| format!("write {}", out.display()))?;
        println!(
            "{} {} · {region} · composed {} bytes to {}",
            spore.id,
            spore.version,
            composed.len(),
            out.display()
        );
        return Ok(());
    }

    let snapshot_path = snapshot_path.or_else(|| {
        if !fixture {
            return None;
        }
        let candidate = spore.dir.join("ui").join("snapshot.lua");
        if candidate.is_file() {
            eprintln!(
                "chrome-host: using the package fixture {}; pass --no-snapshot for live state",
                candidate.display()
            );
            return Some(candidate);
        }
        None
    });

    let host = Host::new(width, height, sdk.as_deref(), &spore.grants)?;
    let snapshot = host.load_snapshot(snapshot_path.as_deref())?;
    let scene = host.render(&spore, &region, snapshot)?;

    println!(
        "{} {} · {region} · {} nodes",
        spore.id,
        spore.version,
        scene.len()
    );
    if let Some(path) = scene_path {
        write_scene(&path, &scene)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn write_scene(path: &Path, scene: &[serde_json::Value]) -> Result<()> {
    let document = serde_json::json!({ "schema": 1, "nodes": scene });
    std::fs::write(path, serde_json::to_vec_pretty(&document)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
