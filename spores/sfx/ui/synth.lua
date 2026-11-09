-- The built-in soundboard. This file is data, not a Lua program: the guest
-- reads it from the workspace at `sfx/synth.lua` and falls back to the copy
-- embedded in the Spore. Tweak a frequency, envelope, or duration here; no
-- Rust rebuild is required.
--
-- Supported fields:
--   wave       sine | square | saw | noise
--   from/to    frequency in Hz (from == to is a steady tone)
--   ms         duration in milliseconds
--   attack_ms  fade-in (default 2)
--   release_ms fade-out (default 8)
--   decay_ms   exponential half-life; 0 or omitted disables it
--   partials   additive list of { freq = Hz, gain = number }
--   segments   list of { wave, from, to, ms }
--   alternate  { from, to, ms } alternating tones for the pad duration
--   click      { ms, gain } deterministic noise at the attack
--   smooth     0..1 low-pass coefficient for noise pads
--   highpass   true to first-difference noise
return {
  version = 1,
  pads = {
    {
      id = "blip", label = "Blip", category = "ui",
      wave = "sine", from = 523.25, to = 523.25, ms = 80,
      attack_ms = 5, release_ms = 15, decay_ms = 55,
    },
    {
      id = "ping", label = "Ping", category = "ui",
      ms = 140, decay_ms = 70,
      partials = {
        { freq = 1175, gain = 1.0 },
        { freq = 2350, gain = 0.35 },
      },
    },
    {
      id = "pop", label = "Pop", category = "ui",
      wave = "sine", from = 260, to = 90, ms = 90, decay_ms = 35,
    },
    {
      id = "tick", label = "Tick", category = "ui",
      wave = "noise", ms = 22, highpass = true, decay_ms = 6,
    },
    {
      id = "coin", label = "Coin", category = "alert",
      decay_ms = 160,
      segments = {
        { wave = "square", from = 988, to = 988, ms = 55 },
        { wave = "square", from = 1319, to = 1319, ms = 110 },
      },
    },
    {
      id = "chime", label = "Chime", category = "alert",
      ms = 720, decay_ms = 220,
      partials = {
        { freq = 523.25, gain = 1.0 },
        { freq = 659.25, gain = 0.62 },
        { freq = 783.99, gain = 0.4 },
        { freq = 1046.5, gain = 0.22 },
      },
    },
    {
      id = "rise", label = "Rise", category = "alert",
      wave = "sine", from = 220, to = 880, ms = 240,
    },
    {
      id = "drop", label = "Drop", category = "alert",
      wave = "sine", from = 880, to = 220, ms = 240,
    },
    {
      id = "alarm", label = "Alarm", category = "alert",
      ms = 700, alternate = { from = 660, to = 880, ms = 125 },
    },
    {
      id = "zap", label = "Zap", category = "fx",
      wave = "saw", from = 1600, to = 180, ms = 180,
      click = { ms = 40, gain = 0.25 }, decay_ms = 120,
    },
    {
      id = "thud", label = "Thud", category = "fx",
      wave = "sine", from = 92, to = 92, ms = 240,
      click = { ms = 12, gain = 0.45 }, decay_ms = 90,
    },
    {
      id = "noise", label = "Noise", category = "fx",
      wave = "noise", ms = 140, smooth = 0.72, decay_ms = 55,
    },
  },
}
