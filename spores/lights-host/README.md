# lights-host

A loopback browser host for the [`lights`](../lights) Lua view. The host owns the
bridge connection, uses the existing `hue` Spore for every read and write, and
renders the view through `tools/shot.lua`. The browser sees one scene document
and hit regions; it never sees the bridge key or the bridge address.

```sh
# from a checkout with the soil-http binary built
cargo run -p lights-host -- \
  --bridge http://192.168.10.15 \
  --soil-http ../soil/target/debug/soil-http \
  --module spores/hue/module.wasm \
  --sdk . \
  --bind 127.0.0.1:8814
```

Open `http://127.0.0.1:8814/`. The default credential is
`geode://home/hue/bridge`, leased under `hue.control` through
`~/.geode/geode.sock`.

## Boundaries

- The server binds loopback and has no authentication. Do not expose it to a
  network; a request can turn lights on or off.
- `GET /scene.json?w=1200&h=820` renders the view from a 2-second snapshot cache.
- `POST /action` accepts the `lights.*` intents documented in
  [`../lights/ui/API.md`](../lights/ui/API.md).
- The host spawns the `hue` component for state (`status`, `groups`, `lights`,
  `scenes`) and for writes (`on`, `off`, `bri`, `scene-id`). It does not
  reimplement Hue semantics or handle the Geode key.

## Tests

```sh
cargo test -p lights-host
```

Unit tests cover origin normalization, Lua serialization, and the brightness
percentage conversion. The live write path is exercised against the bridge with
`POST /action`; the view itself has a headless fixture test in
[`../lights/tests/ui_fixture.lua`](../lights/tests/ui_fixture.lua).
