//! Built-in pads: small, deterministic SFX rendered in the guest.
//!
//! A built-in is not a fallback for a missing asset. It is the guarantee that a
//! fresh install has a sound for every default alert, and that the alert path
//! still works when the workspace is empty or a sample was revoked. Samples are
//! user content; built-ins are part of the Spore.
//!
//! Every pad is mono at the requested rate, peak-normalised, and enveloped at
//! both edges. A pad that clicks is a bug in the pad, not in the mixer.
//!
//! The definitions live in `ui/synth.lua` and are loaded at runtime. The copy
//! in this crate is the fallback that guarantees a fresh install has sound.

use std::cell::RefCell;

use crate::lua::{self, Value};

pub const PEAK: f32 = 0.62;
const DEFAULT_SOURCE: &str = include_str!("../ui/synth.lua");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "blip",
        label: "Blip",
        category: "ui",
    },
    Builtin {
        id: "ping",
        label: "Ping",
        category: "ui",
    },
    Builtin {
        id: "pop",
        label: "Pop",
        category: "ui",
    },
    Builtin {
        id: "tick",
        label: "Tick",
        category: "ui",
    },
    Builtin {
        id: "coin",
        label: "Coin",
        category: "alert",
    },
    Builtin {
        id: "chime",
        label: "Chime",
        category: "alert",
    },
    Builtin {
        id: "rise",
        label: "Rise",
        category: "alert",
    },
    Builtin {
        id: "drop",
        label: "Drop",
        category: "alert",
    },
    Builtin {
        id: "alarm",
        label: "Alarm",
        category: "alert",
    },
    Builtin {
        id: "zap",
        label: "Zap",
        category: "fx",
    },
    Builtin {
        id: "thud",
        label: "Thud",
        category: "fx",
    },
    Builtin {
        id: "noise",
        label: "Noise",
        category: "fx",
    },
];

pub fn find(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.id == id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wave {
    Sine,
    Square,
    Saw,
    Noise,
}

#[derive(Clone, Debug)]
struct Segment {
    wave: Wave,
    from: f32,
    to: f32,
    ms: u32,
}

#[derive(Clone, Debug)]
struct Pad {
    id: String,
    wave: Wave,
    from: f32,
    to: f32,
    ms: u32,
    attack_ms: u32,
    release_ms: u32,
    decay_ms: u32,
    partials: Vec<(f32, f32)>,
    segments: Vec<Segment>,
    alternate: Option<(f32, f32, u32)>,
    click: Option<(u32, f32)>,
    smooth: f32,
    highpass: bool,
}

thread_local! {
    static PADS: RefCell<Option<Vec<Pad>>> = const { RefCell::new(None) };
}

/// Install pad definitions from a Lua data document. A parse error leaves the
/// previous definitions in place; the caller can show the named reason.
pub fn install(source: &str) -> Result<(), String> {
    let pads = parse_definitions(source)?;
    PADS.with(|current| *current.borrow_mut() = Some(pads));
    Ok(())
}

/// Re-read the embedded definitions. Tests use this to isolate a case; a host
/// can use it to drop a workspace override.
pub fn reset() {
    PADS.with(|current| *current.borrow_mut() = None);
}

fn parse_definitions(source: &str) -> Result<Vec<Pad>, String> {
    let root = lua::parse(source)?;
    let version = number(root.field("version")).unwrap_or(1.0);
    if version != 1.0 {
        return Err(format!("unsupported synth version {version}"));
    }
    let entries = root
        .field("pads")
        .ok_or("synth document has no pads")?
        .array()
        .ok_or("pads must be an array")?;
    let mut pads = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = text(entry.field("id")).ok_or("pad has no id")?;
        if find(&id).is_none() {
            return Err(format!("unknown built-in pad {id:?}"));
        }
        let wave = match text(entry.field("wave")).as_deref() {
            Some("sine") | None => Wave::Sine,
            Some("square") => Wave::Square,
            Some("saw") => Wave::Saw,
            Some("noise") => Wave::Noise,
            Some(other) => return Err(format!("{id}: unknown wave {other:?}")),
        };
        let from = number(entry.field("from")).unwrap_or(440.0) as f32;
        let to = number(entry.field("to")).unwrap_or(f64::from(from)) as f32;
        let ms = number(entry.field("ms")).unwrap_or(120.0).max(1.0) as u32;
        let partials = match entry.field("partials") {
            Some(value) => value
                .array()
                .ok_or("partials must be an array")?
                .into_iter()
                .filter_map(|partial| {
                    Some((
                        number(partial.field("freq"))? as f32,
                        number(partial.field("gain"))? as f32,
                    ))
                })
                .collect(),
            None => Vec::new(),
        };
        let segments = match entry.field("segments") {
            Some(value) => value
                .array()
                .ok_or("segments must be an array")?
                .into_iter()
                .map(|segment| {
                    let wave = match text(segment.field("wave")).as_deref() {
                        Some("square") => Wave::Square,
                        Some("saw") => Wave::Saw,
                        Some("noise") => Wave::Noise,
                        _ => Wave::Sine,
                    };
                    let freq = number(segment.field("from")).unwrap_or(440.0) as f32;
                    Ok(Segment {
                        wave,
                        from: freq,
                        to: number(segment.field("to")).unwrap_or(f64::from(freq)) as f32,
                        ms: number(segment.field("ms")).unwrap_or(80.0).max(1.0) as u32,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            None => Vec::new(),
        };
        let alternate = entry.field("alternate").map(|value| {
            (
                number(value.field("from")).unwrap_or(660.0) as f32,
                number(value.field("to")).unwrap_or(880.0) as f32,
                number(value.field("ms")).unwrap_or(125.0).max(1.0) as u32,
            )
        });
        let click = entry.field("click").map(|value| {
            (
                number(value.field("ms")).unwrap_or(12.0) as u32,
                number(value.field("gain")).unwrap_or(0.25) as f32,
            )
        });
        pads.push(Pad {
            id,
            wave,
            from,
            to,
            ms,
            attack_ms: number(entry.field("attack_ms")).unwrap_or(2.0) as u32,
            release_ms: number(entry.field("release_ms")).unwrap_or(8.0) as u32,
            decay_ms: number(entry.field("decay_ms")).unwrap_or(0.0) as u32,
            partials,
            segments,
            alternate,
            click,
            smooth: number(entry.field("smooth")).unwrap_or(0.0) as f32,
            highpass: matches!(entry.field("highpass"), Some(Value::Bool(true))),
        });
    }
    for builtin in BUILTINS {
        if !pads.iter().any(|pad| pad.id == builtin.id) {
            return Err(format!("missing built-in pad {}", builtin.id));
        }
    }
    Ok(pads)
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Str(text) => Some(text.clone()),
        _ => None,
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => Some(*number),
        _ => None,
    }
}

pub fn duration_ms(id: &str, rate: u32) -> Option<u64> {
    let rate = rate.max(1);
    render(id, rate).map(|samples| (samples.len() as u64 * 1_000) / u64::from(rate))
}

/// Render one built-in pad. `None` means the id is not a built-in; the caller
/// then looks for a sample pad with that id.
pub fn render(id: &str, rate: u32) -> Option<Vec<f32>> {
    let rate = rate.max(1);
    PADS.with(|current| {
        let mut current = current.borrow_mut();
        if current.is_none() {
            let pads = parse_definitions(DEFAULT_SOURCE).ok()?;
            *current = Some(pads);
        }
        let pads = current.as_ref()?;
        let pad = pads.iter().find(|pad| pad.id == id)?;
        Some(render_pad(pad, rate))
    })
}

fn frames(rate: u32, ms: u32) -> usize {
    ((u64::from(rate) * u64::from(ms)) / 1_000) as usize
}

fn t(index: usize, rate: u32) -> f32 {
    index as f32 / rate as f32
}

fn sine(freq: f32, time: f32) -> f32 {
    (std::f32::consts::TAU * freq * time).sin()
}

fn square(freq: f32, time: f32) -> f32 {
    if sine(freq, time) >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

fn saw(freq: f32, time: f32) -> f32 {
    2.0 * (freq * time).fract() - 1.0
}

/// A linear frequency sweep with a phase accumulator. A sweep is not
/// `sin(2π f(t) t)`: that formula changes the *period* discontinuously and
/// chirps in the wrong direction.
fn sweep(rate: u32, from: f32, to: f32, ms: u32, wave: fn(f32, f32) -> f32) -> Vec<f32> {
    let count = frames(rate, ms).max(1);
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let progress = index as f32 / count as f32;
        let freq = from + (to - from) * progress;
        out.push(wave(freq, phase));
        phase += freq / rate as f32;
    }
    out
}

/// Deterministic white noise. A soundboard that sounds different on every
/// press cannot be screenshotted, tested, or trusted.
struct Noise(u32);

impl Noise {
    fn new(seed: u32) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let value = (self.0 >> 8) as f32 / 16_777_216.0;
        value * 2.0 - 1.0
    }
}

fn edge_envelope(samples: &mut [f32], rate: u32, attack_ms: u32, release_ms: u32) {
    let count = samples.len();
    if count == 0 {
        return;
    }
    let attack = frames(rate, attack_ms).max(1).min(count);
    let release = frames(rate, release_ms).max(1).min(count);
    for (index, sample) in samples.iter_mut().take(attack).enumerate() {
        *sample *= index as f32 / attack as f32;
    }
    for index in 0..release {
        let at = count - 1 - index;
        samples[at] *= index as f32 / release as f32;
    }
}

fn normalize(samples: &mut [f32], peak: f32) {
    let max = samples
        .iter()
        .fold(0.0f32, |max, value| max.max(value.abs()));
    if max <= f32::EPSILON {
        return;
    }
    let scale = peak / max;
    for sample in samples {
        *sample *= scale;
    }
}

fn finish(mut samples: Vec<f32>, rate: u32, attack_ms: u32, release_ms: u32) -> Vec<f32> {
    edge_envelope(&mut samples, rate, attack_ms, release_ms);
    normalize(&mut samples, PEAK);
    samples
}

fn decay(samples: &mut [f32], rate: u32, half_life_ms: u32) {
    if half_life_ms == 0 {
        return;
    }
    let half = frames(rate, half_life_ms).max(1) as f32;
    for (index, sample) in samples.iter_mut().enumerate() {
        *sample *= 0.5f32.powf(index as f32 / half);
    }
}

fn render_pad(pad: &Pad, rate: u32) -> Vec<f32> {
    let mut out = if !pad.segments.is_empty() {
        let mut out = Vec::new();
        for segment in &pad.segments {
            out.extend(tone(
                segment.wave,
                segment.from,
                segment.to,
                segment.ms,
                rate,
            ));
        }
        out
    } else if let Some((from, to, segment_ms)) = pad.alternate {
        let count = frames(rate, pad.ms).max(1);
        let segment = frames(rate, segment_ms).max(1);
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let freq = if (index / segment) % 2 == 0 { from } else { to };
            out.push(sine(freq, t(index, rate)));
        }
        out
    } else if !pad.partials.is_empty() {
        let count = frames(rate, pad.ms).max(1);
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let time = t(index, rate);
            out.push(
                pad.partials
                    .iter()
                    .map(|(freq, gain)| gain * sine(*freq, time))
                    .sum(),
            );
        }
        out
    } else if pad.wave == Wave::Noise {
        let count = frames(rate, pad.ms).max(1);
        let mut noise = Noise::new(seed(&pad.id));
        let mut smoothed = 0.0;
        let mut previous = 0.0;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let value = noise.next();
            let value = if pad.highpass {
                let high = value - previous;
                previous = value;
                high
            } else {
                smoothed = smoothed * pad.smooth + value * (1.0 - pad.smooth);
                smoothed
            };
            out.push(value);
        }
        out
    } else {
        tone(pad.wave, pad.from, pad.to, pad.ms, rate)
    };

    if let Some((click_ms, click_gain)) = pad.click {
        let count = out.len().min(frames(rate, click_ms));
        let mut noise = Noise::new(seed(&pad.id).rotate_left(7));
        for sample in out.iter_mut().take(count) {
            *sample += click_gain * noise.next();
        }
    }
    decay(&mut out, rate, pad.decay_ms);
    finish(out, rate, pad.attack_ms, pad.release_ms)
}

fn tone(wave: Wave, from: f32, to: f32, ms: u32, rate: u32) -> Vec<f32> {
    match wave {
        Wave::Sine => sweep(rate, from, to, ms, sine),
        Wave::Square => sweep(rate, from, to, ms, square),
        Wave::Saw => sweep(rate, from, to, ms, saw),
        Wave::Noise => {
            let count = frames(rate, ms).max(1);
            let mut noise = Noise::new(seed(&format!("{from}-{to}")));
            (0..count).map(|_| noise.next()).collect()
        }
    }
}

fn seed(id: &str) -> u32 {
    let mut hash = 2_166_136_261u32;
    for byte in id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_renders_bounded_and_click_free() {
        for builtin in BUILTINS {
            let samples = render(builtin.id, 48_000).expect("builtin renders");
            assert!(!samples.is_empty(), "{} is empty", builtin.id);
            assert!(
                samples.iter().all(|sample| sample.is_finite()),
                "{} has a non-finite sample",
                builtin.id
            );
            let peak = samples
                .iter()
                .fold(0.0f32, |max, value| max.max(value.abs()));
            assert!(peak <= PEAK + 1e-6, "{} peaks at {peak}", builtin.id);
            assert_eq!(samples[0], 0.0, "{} starts with a click", builtin.id);
            assert_eq!(
                samples[samples.len() - 1],
                0.0,
                "{} ends with a click",
                builtin.id
            );
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        for builtin in BUILTINS {
            assert_eq!(
                render(builtin.id, 48_000),
                render(builtin.id, 48_000),
                "{} is not deterministic",
                builtin.id
            );
        }
    }

    #[test]
    fn an_unknown_id_is_not_a_builtin() {
        assert!(render("not-a-pad", 48_000).is_none());
        assert!(find("not-a-pad").is_none());
    }

    #[test]
    fn a_lower_rate_shortens_the_buffer_not_the_duration() {
        let high = render("blip", 48_000).unwrap();
        let low = render("blip", 24_000).unwrap();
        assert_eq!(high.len(), low.len() * 2);
        assert_eq!(duration_ms("blip", 48_000), Some(80));
    }
}
