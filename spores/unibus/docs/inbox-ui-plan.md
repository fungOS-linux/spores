# Inbox UI plan

Status: **M0 mock UI landed.** The new view is `ui/inbox.lua`, built on
`ui/lib/ui.lua` and the snapshot in `examples/inbox_mock.lua`. Preview with
`tools/preview_inbox.lua`.

The split stays the same:

```text
Spore (Rust)                    Lua UI
records + unibus metadata  →    ui/inbox.lua
counts, flags, actions          filters, rows, detail, copy
                                ↓
                                Canvas scene
```

The UI is a pure function of the snapshot. It holds no state and never performs
an action; it returns visible affordances. The host routes a press back to the
Spore as an action id.

## M0 — mock UI (done)

- Header: account, sync, compose, search, more.
- Summary: unread, needs attention, support open, agent drafts.
- Rail: accounts, filters, agent status.
- List: unread dot, subject, preview, sender, labels, status, assignee, time.
- Detail: subject, participants, actions, agent brief, thread, draft approval.
- Footer: keyboard hints and provider.
- Mock data covers mail, chat, support, CI failure, security, billing, draft.
- `lua tests/ui_fixture.lua` and `cargo test -p spore-unibus` gate parse/render.

Known limits: no scrolling, fixed 8-unit frame, no focus state, no action
routing, no real timestamps/relative-time formatting, no width adaptation.

## M1 — layout and navigation

- Virtualize the thread list; keep header/footer fixed and scroll only the list.
- Detail pane scrolls independently; keep the action row sticky.
- Narrow mode (phone): one pane at a time, list → detail with back.
- Wide mode: rail + list + detail; optional collapsed rail.
- Density toggle: comfortable / compact.
- Focus model: `selected_id`, `focus` (`rail`, `list`, `detail`, `composer`),
  keyboard cursor distinct from open thread.
- Empty/loading/offline/error states.
- Stable row heights for virtual scrolling.
- Snapshot gains `layout` hints (`width_class`, `density`, `scroll`, `focus`).
- Actions: `select`, `open`, `back`, `next`, `previous`, `focus`.

## M2 — core inbox actions

- Read/unread, archive, trash, spam, snooze, star/pin, label.
- Multi-select and bulk actions.
- Search: query, filters, saved searches; `/` and `⌘K`.
- Filter rail becomes live: All / Unread / Attention / Support / Chat / Drafts.
- Sort: newest, oldest, priority, sender.
- Optimistic UI with a pending state; host is authoritative.
- Action ids in the snapshot (`action = { id, verb, target, payload }`).
- Spore validates every action against the thread's capabilities.

## M3 — thread, composer, attachments

- Message bodies and attachments are references, never snapshot bytes.
- Reply / reply-all / forward; recipients, cc/bcc, subject.
- Draft persistence; agent draft + human edit + approval.
- Signatures, quoted text, plain/rich toggle.
- Attachment picker by `Target`; no fetch in the UI.
- Send is a separate grant; UI shows "approval required" until granted.
- Outbox states: queued, sending, sent, failed, retry.
- Undo send window.
- Full headers, raw source, authentication results (SPF/DKIM/DMARC) on demand.

## M4 — support and triage

- Queue model: support, billing, auth, infra; per-queue counts and SLAs.
- Status: open, pending, waiting, resolved, snoozed.
- Assignee, priority, tags, due-at, first-response time.
- Canned replies and macros; insert into draft, never auto-send.
- Escalation: human, on-call, agent; unassign on leave.
- Conversation timeline: messages, notes, assignments, status changes.
- SLA warning states in the list and detail.
- Customer/contact context: prior threads, plan, recent events.
- Agent brief with citations to messages; no unsupported claims.

## M5 — agent layer

- "Needs attention" scoring with an explanation, not a mystery badge.
- Suggested reply with confidence and source messages.
- Draft approval queue; diff between agent draft and human edit.
- Action proposals: archive, label, assign, resolve; require approval by policy.
- Activity log per thread: what the agent read, drafted, changed, sent.
- Guardrails: no send without grant; no external fetch without capability;
  no secrets in prompts/snapshots.
- Agent state in the rail: idle, reading, drafting, waiting for approval, error.
- Evaluation fixtures: real thread + expected triage + expected actions.

## M6 — chat

- Channels and DMs; unread/mention counts.
- Threaded replies, reactions, attachments by target.
- Search across channels.
- Same detail/actions model as mail where possible; chat-specific composer.
- Presence is host-provided, never invented by the UI.

## M7 — host integration

- `soil:unibus` publishes `email.message.v1`, `email.delta.v1`,
  `chat.message.v1`, `chat.delta.v1` metadata.
- `soil:records` stores accounts, inboxes, threads, messages, labels, drafts,
  assignments, cursors.
- AgentMail adapter maps inbox/message/thread JSON into Unibus frames.
- Sync: `changes`/`apply`, tombstones, conflict policy.
- Offline queue for actions; replay on reconnect.
- Idempotency keys for every mutation.
- No bodies or attachments in Unibus; only references and bounded previews.

## M8 — send

- Separate `soil:unibus.publish` + transport grant.
- Recipient policy, rate limits, approval gates.
- Outbox and retries with provider idempotency.
- Audit record for every send: actor, draft, approval, provider id.
- Bounce/complaint handling back into the inbox.

## Snapshot shape (target)

```text
revision, account{...}, counts{...}, filters[...], threads[...]
thread {
  id, collection, kind, title, sender, participants, preview, time,
  unread, urgent, selected, status, assignee, priority, labels,
  message_count, due_at, capabilities{...}, actions[...],
  messages[...], draft{...}, agent{...}
}
layout { width_class, density, focus, scroll }
```

Rules:

- No raw MIME, bodies, attachments, credentials, or provider tokens.
- Times are RFC3339 or a preformatted label; UI never calls a clock.
- Counts are precomputed by the Spore.
- Actions are ids; the UI never mutates state.
- Capabilities decide whether an affordance is shown.

## UI details backlog

- Icons: replace glyphs with host icons when available; keep names stable.
- Type scale and spacing tokens from the host theme, not hardcoded.
- Color: role-based only; no view names a hex value.
- Relative time: "2m", "Tue", "Oct 22"; computed by the Spore.
- Sender display: name, address, avatar/initials, VIP.
- Unread: dot plus weight, not color alone.
- Urgent: icon plus label, not color alone.
- Selection vs focus vs unread must be distinguishable.
- Long subjects: clamp to two lines in the list, full in detail.
- Empty body/preview fallbacks.
- Attachments: count, type, size, safe preview.
- Accessibility: roles, labels, focus order, screen-reader text.
- Keyboard: j/k, g/G, enter, esc, r, a, e, #, s, /, ⌘K, ?.
- Error: provider down, auth expired, sync conflict, action rejected.
- Loading: skeleton rows, no layout jump.
- Performance: virtual list, bounded text, no per-frame allocation.

## Decisions to make

1. Is the default surface phone-width or desktop-width? This changes M1 first.
2. Does the rail collapse on phone, or become a bottom tab bar?
3. Is detail a third pane, a sheet, or a full-screen route?
4. Which actions require approval: send only, or also archive/assign?
5. Is chat in this Spore or a sibling? Shared shell, separate snapshot?
6. Do we show agent confidence numerically or only as a label?
7. Is search local records only, or provider search too?

## Near-term order

1. M1 scrolling/focus/narrow layout.
2. M2 read/unread/archive/assign + action routing.
3. M3 composer/draft approval.
4. M4 support queues/SLA.
5. M5 agent brief/approval/activity.
6. M7 real Unibus/records + AgentMail.
