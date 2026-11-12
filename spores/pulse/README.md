# pulse

One console for the two unattended edges of the system: the crons **Pulse**
fires on a schedule, and the webhooks that arrive without being asked. The
second pane reads Confluence's ingress because that is the other thing an
operator checks when a page looks quiet.

```text
pulse.cron  ─┐
pulse.run   ─┼─▶ snapshot ─▶ ui/console.lua ─▶ Canvas / pulse.myc
pulse.target─┤
confluence.hook      ─┤
confluence.delivery  ─┘
```

## First, what did not exist

There was no `pulse` Spore in this repository, none in `ajmwagar/spores`, and
no Pulse scheduler crate anywhere in the soil/fungOS trees. "Pulse" up to now
named two other things: Canvas's frame-pulse contract (`canvas-lua-modules.md`:
"Pulse time is supplied by the scheduler") and Alfalfa's per-project dashboard
stream. So the schedule half of this Spore has no producer yet; the contracts
below are the shape that producer should write.

The Confluence console was also planned rather than built: `soil-confluence`'s
README marks the console **planned**, and its ingress does not write delivery
rows yet. The ingress pane here is live and reads the records that console
would have read; until the ingress writes them, the pane shows the seeded
records in `examples/records`.

## Why one window

A cron list on its own answers "what is scheduled?". The question someone
actually asks at 03:10 is "did it run, and did anything arrive?". Splitting
those into two apps makes the reader cross-reference two windows to answer one
question, so this Spore owns both panes and keeps the grant split explicit:
`soil:records.read` for the ingress records, `soil:records.write` for cron
edits. No secret is in any record: `confluence.hook` carries the credential
**name**, never the material.

## The view

`ui/console.lua` is a pure projection of one snapshot. Every control is an
intent (`{ kind = ... }`), never a callback: the host applies it with the same
reducer the CLI uses and re-renders. Nothing in the view reads a clock, a file,
a socket, or a global it did not receive.

| Pane | Left | Right |
|---|---|---|
| `crons` | health, schedule, next fire | schedule editor, presets, weekdays, target, runs |
| `ingress` | health, 24-hour counts | provider, credential name, rejections, deliveries |

Schedules are edited by stepping the hour and minute, toggling weekdays, and
choosing presets. There is no text field because no host in this system delivers
keystrokes to a Lua module; a typed schedule arrives through the same
`pulse.cron.schedule` intent the CLI accepts (`pulse set … --schedule`), and the
editor uses it as soon as a host provides one.

## Run it

```sh
cargo build -p spore-pulse

# The records stand-in: one JSONL file per collection, `soil:records` entries.
./target/debug/pulse --records ~/.local/share/fungos/pulse list
./target/debug/pulse --records ~/.local/share/fungos/pulse hooks

# Create or edit one. Every edit is a new entry, never a rewrite.
./target/debug/pulse --records ~/.local/share/fungos/pulse set nightly-backup \
  --name "Nightly backup" --schedule "0 3 * * *" \
  --target dev.fungos.worklog#compact
./target/debug/pulse --records ~/.local/share/fungos/pulse toggle digest
./target/debug/pulse validate "*/15 * * * *" --now 1767326400000

# The snapshot the view reads (and the committed fixture).
./target/debug/pulse --records ~/.local/share/fungos/pulse snapshot \
  --now 1767326400000 --select nightly-backup > /tmp/pulse.lua

# A picture without a compositor.
FUNGOS_SDK=~/src/fungOS/fungos-sdk lua tools/shot.lua \
  ui/console.lua /tmp/pulse.lua ../themes/graphite.lua /tmp/pulse.json 1182 1569
```

`pulse-serve` is the browser host and the reference intent router:

```sh
./target/debug/pulse-serve \
  --records ~/.local/share/fungos/pulse \
  --sdk ~/src/fungOS/fungos-sdk \
  --bind 127.0.0.1:8806
```

It runs the same Lua view through `tools/shot.lua`, serves the scene document,
and posts presses back through `Store::apply`. The gateway routes `pulse.myc` to
it. The page polls the scene every three seconds, so a CLI edit appears without
a reload.

## Records

One entry per `soil:records` write, keyed `{ collection, id }`, with the
`schema`, `revision`, `origin`, `updated_at_ms`, `deleted` and `body` the host
contract defines (`docs/records.md`). The JSONL file is a stand-in until a Soil
host implements records; the schema does not change when it does.

| Collection | Schema | Key | Body |
|---|---|---|---|
| `pulse.cron` | `fungos.pulse/cron@1` | cron id | `name`, `schedule`, `target`, `enabled`, `created_at_ms`, `updated_at_ms` |
| `pulse.run` | `fungos.pulse/run@1` | run id | `cron_id`, `started_at_ms`, `finished_at_ms`, `outcome`, `detail` |
| `pulse.target` | `fungos.pulse/target@1` | dispatch target | `label`, `detail` |
| `confluence.hook` | `fungos.confluence/hook@1` | slug | `name`, `provider`, `tenant`, `kind`, `credential`, `created_at_ms` |
| `confluence.delivery` | `fungos.confluence/delivery@1` | delivery id | `slug`, `delivery_id`, `status`, `reason`, `bytes`, `received_at_ms`, `node`, `event_type` |

The key is authoritative; a body's `id`/`slug` is filled from it on read. A
cron's `target` must be a registered `pulse.target`: the console never invents a
dispatch target, because a target the scheduler cannot resolve is a cron that
fails at 03:00.

### Schedules

Five fields: `minute hour day month weekday`. `*`, `n`, `a-b`, `a/n`, `a-b/n`
and comma lists; no names. Day of month and day of week are **ANDed** (not
Vixie's OR): `0 9 1 * 1` is the first, when it is a Monday. Times are **UTC**
until `soil:time` offers a zone. Next-fire search is bounded at ten years; a
schedule that cannot fire in that window is `never fires`, not a guess.

### Health

Cron (`pulse.cron` + `pulse.run`):

| State | Rule |
|---|---|
| `paused` | `enabled = false` |
| `invalid` | the schedule does not parse; the problems are in the row |
| `unschedulable` | no fire in the next ten years |
| `overdue` | a fire was due more than five minutes ago, the cron existed then, and no run started at it |
| `failing` | the last two runs both failed |
| `idle` | enabled and valid, no runs yet |
| `ok` | none of the above |

Webhook (`confluence.hook` + `confluence.delivery`):

| State | Rule |
|---|---|
| `unknown` | no delivery recorded |
| `failing` | the last delivery was rejected, or three rejected in 24 hours |
| `degraded` | any rejection in 24 hours |
| `silent` | no delivery in seven days |
| `healthy` | an accepted delivery in the window, no rejections |

A rejection is stored with its reason (`401: signature mismatch`), so the pane
can say *why* instead of counting red rows.

## Tests

```sh
cargo test -p spore-pulse          # schedules, health, reducer, Lua emitter
lua tests/ui_fixture.lua           # the view against the committed snapshot

# The browser page needs a scene, the way pulse-serve builds one:
FUNGOS_SDK=~/src/fungOS/fungos-sdk lua ../../tools/shot.lua \
  ui/console.lua ui/snapshot.example.lua ../themes/graphite.lua /tmp/pulse.json 1182 1569
node tests/page_smoke.mjs /tmp/pulse.json
```

The Lua fixture needs `FUNGOS_SDK` (the checkout with `lua/fungos/ui.lua`); the
Rust test skips the render when it is absent rather than passing silently.

## Known gaps

- **No Pulse scheduler.** Nothing fires a cron or writes `pulse.run` yet. The
  console is the editor and the reader; the executor is the missing half.
- **The ingress does not write deliveries.** `soil-confluence` verifies and
  answers but records nothing, so the ingress pane is seeded with
  `examples/records` until it does. The clean producer is the guest writing
  `confluence.delivery` entries through `soil:records`; the host does not link a
  records book today, and the deployed ingress (`confluence.myc`, agora-one)
  would need that change too. The console's read side is already the shape the
  `soil-confluence` README calls for.
- **No wasm module in this package.** The intents are applied by `pulse` and
  `pulse-serve`; a Spore module over `soil:records` is the next step. No Canvas
  host delivers presses today either, which is why the browser host exists.
- **The composed module is 115 KB**, over `tend check`'s 64 KiB preflight (Canvas
  itself has rendered 95 KB modules). The browser host renders the view through
  `tools/shot.lua`; shipping to Canvas needs the snapshot trimmed or the SDK
  module split.
- **The store is a file.** It stands in for `soil:records`; compaction and
  tombstones beyond append-only are the host's job.
- **Ordering is per store, and the log only grows.** Same limit as the worklog.
