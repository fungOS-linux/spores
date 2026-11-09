//! A minimal `wasi:audio` probe for the built-in pads.
//!
//! A component is a `cdylib`, not a binary: it has no `main`, only the world's
//! exports. This crate exists so the audio path can be exercised through the
//! existing `wasi-audio` harness (local device, or `--isochrone-destination`)
//! without a full Spore runner.

wit_bindgen::generate!({ path: "wit", world: "probe", generate_all });

use wasi::audio::playback;

struct Component;

export!(Component);

impl Guest for Component {
    fn run() -> u32 {
        match play() {
            Ok(()) => 0,
            Err((code, error)) => {
                eprintln!("sfx-probe: {error}");
                code
            }
        }
    }
}

fn play() -> Result<(), (u32, String)> {
    let rate = 48_000u32;
    let output = playback::open(playback::Format {
        sample_rate: rate,
        channels: 2,
    })
    .map_err(|error| (2, format!("open: {error:?}")))?;

    let mut frames = Vec::new();
    for builtin in spore_sfx::synth::BUILTINS {
        let mono = spore_sfx::synth::render(builtin.id, rate)
            .ok_or_else(|| (6, format!("{} did not render", builtin.id)))?;
        for sample in mono {
            let value = (sample * 0.9).clamp(-1.0, 1.0);
            frames.push(value);
            frames.push(value);
        }
        let silence = (rate / 4) as usize;
        for _ in 0..silence {
            frames.push(0.0);
            frames.push(0.0);
        }
    }

    let mut offset = 0;
    while offset < frames.len() {
        let accepted = output
            .write(&frames[offset..])
            .map_err(|error| (3, format!("write: {error:?}")))? as usize;
        if accepted == 0 {
            // 50 ms is shorter than this host's ring; a `NotReady` here means
            // "still full", not "broken". Retry rather than failing a probe
            // that is simply ahead of the device.
            match output.drain(500) {
                Ok(()) | Err(playback::Error::NotReady) => {}
                Err(error) => return Err((4, format!("drain: {error:?}"))),
            }
        } else {
            offset += accepted * 2;
        }
    }
    output
        .drain(1_000)
        .map_err(|error| (4, format!("drain: {error:?}")))?;
    output
        .close()
        .map_err(|error| (5, format!("close: {error:?}")))?;
    Ok(())
}
