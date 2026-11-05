# Contacts Spore

A read-only contacts directory over one `soil:records` collection.

- Host opens the `fungos-records` contacts book as collection `contact`.
- Spore scans records, projects vCard-shaped JSON, and filters by a query.
- Output JSON feeds `ui/contacts.lua` through the Canvas layout host.
- The Spore has only `soil:records.read`; no write, sync, network, clock, or
  filesystem authority.

Build:

```sh
cargo build --manifest-path spores/contacts/Cargo.toml --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/spore_contacts.wasm spores/contacts/module.wasm
```

Run the host against the local records book:

```sh
cargo run -p contacts-host -- spores/contacts/module.wasm "alex" > /tmp/contacts.json
```

The host reads `RECORDS_ROOT`, `RECORDS_CONTACTS_BOOK`, and
`RECORDS_COLLECTION`; defaults are the local contacts book and `contact`.
