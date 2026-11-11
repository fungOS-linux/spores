# hue

A CLI Spore for Philips Hue. It owns Hue semantics — targets, scenes, and the
sRGB-to-CIE-xy conversion the bridge expects — and nothing else. The host owns
the connection and the origin allowlist.

```sh
hue --bridge http://192.168.1.10 pair
hue --bridge http://192.168.1.10 --credential geode://home/hue/bridge status
hue --bridge http://192.168.1.10 --credential geode://home/hue/bridge on
hue --bridge http://192.168.1.10 --credential geode://home/hue/bridge --light 3 color '#ff8800'
hue --bridge http://192.168.1.10 --credential geode://home/hue/bridge --group 1 scene Relax
```

`--json` prints the bridge's reply instead of the one-line summary, which is
what an agent wants when it needs to inspect state. `pair` with `--json` is
intended for the one-time key-capture step.

## Pairing

Press the physical link button on the bridge, then run:

```sh
hue --bridge http://192.168.1.10 --json pair | jq -r '.[0].success.username'
```

The bridge returns the application key once. Store it in Geode, never in the
spore or a record:

```sh
printf %s "$HUE_KEY" | geoded --store ~/.geode/store.db \
  --key-file ~/.geode/store.key \
  --put geode://home/hue/bridge --purpose hue.control
```

## Capability

Only `soil:http`, and only for origins the host was told to allow. The key
stays in Geode; the host leases it and substitutes it into the v1 URL path. The
Spore only sees the opaque `geode://home/hue/bridge` handle.

```sh
soil-http --allow http://192.168.1.10:80 \
  --geode-socket ~/.geode/geode.sock --geode-purpose hue.control \
  spores/hue/module.wasm \
  --bridge http://192.168.1.10 --credential geode://home/hue/bridge status
```

`--key` remains for native tests and one-off debugging. It is not the shipping
path: a v1 key in an argument is visible to peers on the same host.

## Build

```sh
cargo build --target wasm32-wasip2 --release -p spore-hue
cp target/wasm32-wasip2/release/spore_hue.wasm spores/hue/module.wasm
```

Native tests use a fake transport and need no bridge:

```sh
cargo test -p spore-hue
```
