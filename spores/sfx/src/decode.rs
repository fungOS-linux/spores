//! Sample decoding and the small format conversions the board needs.
//!
//! The guest owns decoding because `wasi:audio` is a PCM sink. The host never
//! sees an MP3, a path, or a codec: it receives interleaved f32 frames.

use std::io::Cursor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// A decoded sample, still at the rate the file declared.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Unsupported(String),
    Malformed(String),
    Empty,
    TooLong,
    TooLarge,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Unsupported(reason) => write!(formatter, "unsupported: {reason}"),
            DecodeError::Malformed(reason) => write!(formatter, "malformed: {reason}"),
            DecodeError::Empty => write!(formatter, "the sample decoded to no frames"),
            DecodeError::TooLong => write!(formatter, "the sample is longer than the 30 s limit"),
            DecodeError::TooLarge => {
                write!(formatter, "the sample is larger than the 32 MiB limit")
            }
        }
    }
}

pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SECONDS: u32 = 30;

/// Decode MP3 or WAV bytes to mono f32. The hint is the workspace path; only
/// the extension is used, never as authority.
pub fn decode(bytes: &[u8], hint: Option<&str>) -> Result<Decoded, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::Empty);
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(DecodeError::TooLarge);
    }

    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let mut hint_value = Hint::new();
    if let Some(path) = hint {
        if let Some(extension) = path.rsplit('.').next() {
            if extension.len() <= 5 {
                hint_value.with_extension(extension);
            }
        }
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint_value,
            source,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|error| DecodeError::Unsupported(error.to_string()))?;
    let mut format = probed.format;

    let (track_id, codec_params) = {
        let track = format
            .tracks()
            .iter()
            .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| DecodeError::Unsupported("no audio track".into()))?;
        (track.id, track.codec_params.clone())
    };
    let rate = codec_params.sample_rate.unwrap_or(48_000).max(1);
    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|error| DecodeError::Unsupported(error.to_string()))?;

    let max_frames =
        usize::try_from(u64::from(rate) * u64::from(MAX_SECONDS)).unwrap_or(usize::MAX);
    let mut mono = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(Error::ResetRequired) => break,
            Err(error) => return Err(DecodeError::Malformed(error.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                let spec = *buffer.spec();
                let channels = spec.channels.count().max(1);
                let mut samples = SampleBuffer::<f32>::new(buffer.capacity() as u64, spec);
                samples.copy_interleaved_ref(buffer);
                for frame in samples.samples().chunks(channels) {
                    mono.push(frame.iter().sum::<f32>() / channels as f32);
                }
            }
            // A damaged frame is a gap, not a reason to refuse the whole
            // sample: a soundboard would rather play 95% than nothing.
            Err(Error::DecodeError(_)) => continue,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(error) => return Err(DecodeError::Malformed(error.to_string())),
        }
        if mono.len() > max_frames {
            return Err(DecodeError::TooLong);
        }
    }

    if mono.is_empty() {
        return Err(DecodeError::Empty);
    }
    mono.truncate(max_frames);
    Ok(Decoded {
        samples: mono,
        sample_rate: rate,
    })
}

/// Linear resampling is enough for short SFX: the error is in the first few ms
/// and a band-limited converter would add a dependency and latency for it.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    let from = from.max(1);
    let to = to.max(1);
    if samples.is_empty() || from == to {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let out_len = ((samples.len() as f64) * f64::from(to) / f64::from(from)).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let fraction = (position - left as f64) as f32;
        let a = samples[left.min(samples.len() - 1)];
        let b = samples[(left + 1).min(samples.len() - 1)];
        out.push(a + (b - a) * fraction);
    }
    out
}

/// A short fade at both ends, for samples that arrive with a hard edge.
pub fn fade_edges(samples: &mut [f32], rate: u32, ms: u32) {
    let count = samples.len();
    if count == 0 {
        return;
    }
    let fade = ((u64::from(rate) * u64::from(ms)) / 1_000).max(1) as usize;
    let fade = fade.min(count / 2).max(1);
    for index in 0..fade {
        let gain = index as f32 / fade as f32;
        samples[index] *= gain;
        samples[count - 1 - index] *= gain;
    }
}

pub fn interleave(mono: &[f32], channels: u8, gain: f32) -> Vec<f32> {
    let channels = channels.max(1);
    let mut out = Vec::with_capacity(mono.len() * usize::from(channels));
    for sample in mono {
        let value = (sample * gain).clamp(-1.0, 1.0);
        for _ in 0..channels {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_wav_decodes_to_mono_at_its_own_rate() {
        let decoded = decode(&wav(&[0, 16_384, -16_384, 8_192], 8_000), Some("blip.wav"))
            .expect("wav decodes");
        assert_eq!(decoded.sample_rate, 8_000);
        assert_eq!(decoded.samples.len(), 4);
        assert!((decoded.samples[1] - 0.5).abs() < 0.01);
        assert!((decoded.samples[2] + 0.5).abs() < 0.01);
    }

    #[test]
    fn a_missing_or_empty_input_is_a_named_error() {
        assert_eq!(decode(&[], None), Err(DecodeError::Empty));
        assert!(matches!(
            decode(b"not audio", None),
            Err(DecodeError::Unsupported(_)) | Err(DecodeError::Malformed(_))
        ));
    }

    #[test]
    fn resampling_keeps_the_duration_and_is_idempotent() {
        let input: Vec<f32> = (0..441).map(|index| index as f32 / 441.0).collect();
        let out = resample(&input, 44_100, 48_000);
        assert_eq!(out.len(), 480);
        assert_eq!(resample(&input, 44_100, 44_100), input);
        assert!((out[0] - input[0]).abs() < 1e-6);
        assert!((out[out.len() - 1] - input[input.len() - 1]).abs() < 1e-3);
    }

    #[test]
    fn edge_fades_reach_zero_and_interleaving_is_bounded() {
        let mut samples = vec![1.0; 48];
        fade_edges(&mut samples, 48_000, 1);
        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[samples.len() - 1], 0.0);
        let frames = interleave(&[2.0, -2.0, 0.5], 2, 0.5);
        assert_eq!(frames, vec![1.0, 1.0, -1.0, -1.0, 0.25, 0.25]);
    }
}
