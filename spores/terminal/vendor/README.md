# libghostty-vt static archives

`libghostty-vt.a` and `libsoilghostty.a` are linked into the Spore for
`wasm32-wasip2`. They are checked in so a normal `cargo build` does not need
Zig or a Ghostty checkout.

- Ghostty commit: `e95bed63330cc921f3cf2fe3d14e024396b09d43` (main)
- Zig: `0.17.1`
- Target: `wasm32-freestanding`
- Optimize: `ReleaseSmall`

Rebuild:

```sh
git clone --depth 1 https://github.com/ghostty-org/ghostty /tmp/ghostty
cd /tmp/ghostty
zig build -Demit-lib-vt -Dtarget=wasm32-freestanding -Doptimize=ReleaseSmall

cp zig-out/lib/libghostty-vt.a \
  "$SPORES/spores/terminal/vendor/libghostty-vt.a"

zig cc -target wasm32-freestanding -Os -I include \
  -c "$SPORES/spores/terminal/native/ghostty_shim.c" \
  -o /tmp/ghostty_shim.o
zig ar rcs "$SPORES/spores/terminal/vendor/libsoilghostty.a" /tmp/ghostty_shim.o
```

The shim exposes a flat cell ABI; the Spore never sees Ghostty structs.
`ghostty-vt.wasm` is intentionally not checked in; it is not used by the
component.
