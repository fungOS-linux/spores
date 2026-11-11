# spotify

A Spotify Connect remote Spore. It is aimed at [RASpotify](https://github.com/roberto-butti/raspotify)
on a home Pi, but it drives any device the Spotify Web API lists.

```sh
spotify devices
spotify --device raspotify play
spotify --device raspotify volume 40
spotify --device raspotify status
spotify play https://open.spotify.com/album/1DFixLWuPkv3KT3TnV35m3
```

`--device NAME` matches a device name by case-insensitive substring and
**transfers playback to it first** when it is idle, which is what makes a
sleeping Raspberry Pi speaker wake up. Without `--device`, commands act on
whatever is already active.

## Authentication

The Spore never sees a token. It asks the host for the `default` credential
handle; the host adds `Authorization: Bearer …`. `soil-http` reads it from an
environment variable so it stays out of `ps`:

```sh
export SPOTIFY_TOKEN="$(your-token-helper)"
soil-http --allow https://api.spotify.com:443 \
  --bearer-env SPOTIFY_TOKEN \
  spores/spotify/module.wasm devices
```

The token needs `user-read-playback-state`, `user-modify-playback-state`, and
`user-read-currently-playing`. It expires after an hour; refresh it outside the
Spore. A `401` from Spotify is reported with that hint.

## Capability

Only `soil:http`, only `https://api.spotify.com:443`. No sockets, no secrets, no
filesystem, no clock.

## Build

```sh
cargo build --target wasm32-wasip2 --release -p spore-spotify
cp target/wasm32-wasip2/release/spore_spotify.wasm spores/spotify/module.wasm
```

Native tests use a fake transport:

```sh
cargo test -p spore-spotify
```
