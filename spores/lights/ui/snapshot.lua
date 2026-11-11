-- A representative frame using the real bridge's names, groups, and scenes on
-- 2026-11-11. On/brightness values are set to show both lit and idle states;
-- the host replaces this with live state. The shape is the contract documented
-- in ui/API.md. Keep the ids bridge-owned: they are what actions target.
return {
  title = "Lights",
  selected = "81",
  bridge = {
    name = "Hue Bridge",
    model = "BSB002",
    online = true,
    lights = 7,
  },
  rooms = {
    { id = "81", name = "Living room", on = 3, total = 4, bri_pct = 60,
      lights = { "3", "8", "7", "4" } },
    { id = "85", name = "Bedroom", on = 0, total = 2, bri_pct = 68,
      lights = { "2", "1" } },
    { id = "83", name = "Kitchen", on = 0, total = 1, bri_pct = 31,
      lights = { "6" } },
    { id = "82", name = "TV", on = 3, total = 5, bri_pct = 60,
      lights = { "4", "3", "2", "7", "8" } },
    { id = "84", name = "Home", on = 3, total = 7, bri_pct = 60,
      lights = { "4", "3", "2", "7", "6", "1", "8" } },
    { id = "200", name = "Music area", on = 3, total = 5, bri_pct = 60,
      lights = { "8", "2", "7", "4", "3" } },
    { id = "201", name = "House Party", on = 3, total = 7, bri_pct = 60,
      lights = { "8", "6", "1", "4", "3", "2", "7" } },
  },
  scenes = {
    { id = "qRFrxxoQ9xCWxv1h", name = "Nighttime", group_id = "81", group = "Living room" },
    { id = "D-Gq0yErvufgJwX1", name = "Read", group_id = "81", group = "Living room" },
    { id = "T177KorSdwbqrRDY", name = "Relax", group_id = "81", group = "Living room" },
    { id = "doQ8rOJ8yoPayIGm", name = "Energize", group_id = "83", group = "Kitchen" },
    { id = "Y60KnC0Xdh7g3E8Z", name = "Sleepy", group_id = "83", group = "Kitchen" },
    { id = "kwVoFJ4Y44MzLDYg", name = "Tokyo", group_id = "82", group = "TV" },
    { id = "T177KorSdwbqrRDY", name = "Relax", group_id = "84", group = "Home" },
    { id = "HnS0-fpExJ73E82D", name = "Movie", group_id = "84", group = "Home" },
    { id = "RaKPtD-5ZKJEXRtm", name = "Dinner", group_id = "84", group = "Home" },
    { id = "IgVdjpa4wpsqUPgB", name = "Rest", group_id = "84", group = "Home" },
  },
  lights = {
    { id = "1", name = "Window Side", on = false, bri = 143, temperature_k = 2242 },
    { id = "2", name = "Chair and a Half Lamp", on = false, bri = 203, temperature_k = 2242 },
    { id = "3", name = "Window Lamp", on = true, bri = 79, temperature_k = 2242 },
    { id = "4", name = "Interrogation Lamp", on = false, bri = 79, temperature_k = 2242 },
    { id = "6", name = "East Counter", on = false, bri = 79, temperature_k = 2242 },
    { id = "7", name = "Buddha", on = true, bri = 160, temperature_k = 2242 },
    { id = "8", name = "Window", on = true, bri = 220, temperature_k = 2242 },
  },
}
