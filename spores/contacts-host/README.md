# contacts-host

Wasmtime host for `spores/contacts`. It links `soil:records` from
`soil-records-host`, opens one records book, calls `snapshot(query)`, and writes
JSON to stdout.

```sh
cargo run -p contacts-host -- spores/contacts/module.wasm "acme"
```

Environment:

- `RECORDS_ROOT` — default `~/.local/share/fungos/records`
- `RECORDS_CONTACTS_BOOK` — default `personal/avery/contacts`
- `RECORDS_COLLECTION` — default `contact`
- `CONTACTS_SPORE` — override the component path
- `CONTACTS_QUERY` — query when no positional argument is supplied
