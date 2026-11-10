# hue

A CLI Spore for Philips Hue. It owns Hue semantics — targets, scenes, and the
sRGB-to-CIE-xy conversion the bridge expects — and nothing else. The host owns
the connection and the origin allowlist.

```sh
hue --bridge http://192.168.1.10 --key "$HUE_KEY" status
hue --bridge http://192.168.1.10 --key "$HUE_KEY" on
hue --bridge http://192.168.1.10 --key "$HUE_KEY" --light 3 color '#ff8800'
hue --bridge http://192.168.1.10 --key "$HUE_KEY" --group 1 scene Relax
```

`--json` prints the bridge's reply instead of the one-line summary, which is
what an agent wants when it needs to inspect state.

## Capability

Only `soil:http`, and only for origins the host was told to allow:

```sh
soil-http --allow http://192.168.1.10:80 hue/module.wasm status
```

Hue's v1 API carries the application key in the URL path, so `--key` is an
argument rather than a host credential. That key is LAN-scoped; it is not a
Hue cloud token. The host still refuses any origin it was not given, and caps
request bodies and responses at the policy budget.

## Build

```sh
cargo build --target wasm32-wasip2 --release -p spore-hue
cp target/wasm32-wasip2/release/spore_hue.wasm spores/hue/module.wasm
```

Native tests use a fake transport and need no bridge:

```sh
cargo test -p spore-hue
```
