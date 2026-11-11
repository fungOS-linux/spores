# Store

The storefront Spore. It answers "what is available, what am I running, and what
would changing it cost?" — and it does not answer "install it". Installing is a
host capability with its own verification and consent; this package draws the
surface that asks.

This is the composition slice: `ui/store.lua` renders a snapshot shaped like the
store's records and emits intents. The module that reads the signed index and
calls `soil:spore.install` is the next revision, which is why `spore.toml`
denies those two grants today instead of declaring them.

The design it implements lives in
[`fungOS-linux/soil/docs/spore-store.md`](https://github.com/fungOS-linux/soil/blob/master/docs/spore-store.md):
the package/listing/service split, categories, screenshots as CIDs, ratings
bound to a digest, the update classes, and multiple sources.

## Views

```text
browse    categories, an editor's-choice hero, featured cards, and an app grid
listing   one app: screenshots, description, capabilities, ratings, reviews
updates   every installed app and what applying its update would cost
```

## Check

```sh
luac -p ui/store.lua
lua tests/ui_fixture.lua      # prints "store ui: ok"
lua tools/compose_store.lua /tmp/modules
lua ../../tools/check-module-contract.lua /tmp/modules/spore-dev-store.lua
```

## Real data

The view takes a snapshot; `soil-store` builds one from a signed catalog and the
packages installed on this machine:

```sh
soil-store plan     --index index.json --spores-root ~/.local/share/fungos/spores
soil-store snapshot --index index.json --spores-root ~/.local/share/fungos/spores \
  -o /tmp/store.snapshot.lua
lua tools/compose_store.lua /tmp/modules /tmp/store.snapshot.lua
```

`plan` is the same classification the view renders, so "does this update need a
reboot?" has one answer, not a UI guess and a CLI guess.

## Preview without Canvas

```sh
FUNGOS_SDK="$PWD" lua tools/shot.lua spores/store/ui/store.lua /dev/null \
  spores/themes/graphite.lua /tmp/store.json 1440 1600
python3 tools/shot.py /tmp/store.json /tmp/store.png 1440 1600
```

Run from the repository root. `shot.lua` takes the view, a snapshot (a path, or
`/dev/null` for the view's own fixture), a theme and the scene it writes;
`shot.py` rasterises that scene. It is a preview, not a host: the real check is
Canvas, where `reload_module` followed by `surface_scene` reports the nodes and
controls the wall actually received.

## Grants

`soil:canvas` only. `soil:spore.catalog`, `soil:spore.install` and `soil:http`
are denied until there is a module to use them.

License: MIT.
