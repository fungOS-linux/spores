//! Render every built-in pad to one stereo WAV, for listening and review.
//!
//!   cargo run --example render_demo -- sfx-demo.wav
//!
//! This is the content the defaults ship, not a host path: no VFS, no appdata,
//! no room. It is the fastest way to hear a synth change without a desktop.

use std::io::Write;

fn main() -> std::io::Result<()> {
    let rate = 48_000u32;
    let channels = 2u16;
    let mut pcm: Vec<i16> = Vec::new();

    for builtin in spore_sfx::synth::BUILTINS {
        let mono = spore_sfx::synth::render(builtin.id, rate).expect("builtin renders");
        for sample in mono {
            let value = (sample * 0.9 * f32::from(i16::MAX)) as i16;
            for _ in 0..channels {
                pcm.push(value);
            }
        }
        // A quarter second of silence keeps the pads audibly separate.
        let silence = (rate / 4 * u32::from(channels)) as usize;
        pcm.resize(pcm.len() + silence, 0);
    }

    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sfx-demo.wav".into());
    let mut file = std::fs::File::create(&path)?;
    file.write_all(&out)?;
    println!("{path}: {} bytes", out.len());
    Ok(())
}
