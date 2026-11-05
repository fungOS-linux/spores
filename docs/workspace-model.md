# Workspace model

Status: **design only.** Nothing here is scheduled. It names what Canvas already
does so the vocabulary stops shifting, and records the pieces that do not exist
yet.

## The four levels

The desktop metaphor already has these; the code has three of them under other
names.

| Level | Meaning | Today |
|---|---|---|
| **Server** | one app process: one runtime directory, one control socket, one set of displays | implicit |
| **Session** | an identity: a project, its theme, its saved arrangement | `projects` in `workspaces.json` |
| **Window** | a named arrangement with its own layout | a `pages` entry (`active_page`) |
| **Pane** | a region holding one content source | a `surface` |
| **Screen** | a physical output | `displays`, `DisplayTarget` |

A session holds windows. A window holds panes in a layout (split, stack, canvas).
A pane names exactly one content source. A screen shows one window from one
session at a time.

## Addresses

A pane needs an address that survives being moved, and a window needs one that
survives a restart:

```text
project:window:pane
spore-dev:main:md
agentsmith:board:summary
```

Rules:

- `project` is a session id, `window` is a page name, `pane` is a surface id.
- Short forms resolve against the active session: `:main:md`, `:md`.
- Addresses are for people and the control plane, not for content. A spore still
  sees only its own normalized input and its own `source_id`.

## Verbs

```sh
canvasctl session list
canvasctl session switch spore-dev
canvasctl window list
canvasctl window new main
canvasctl pane list
canvasctl pane focus :md
canvasctl pane move :md to agentsmith:board
canvasctl pane split :md vertical 0.5
```

Plus a prefix chord and a command palette (`fungos/command`) for the same
actions, because the control socket is for agents and the chord is for hands.

## Pane movement

Moving a pane moves its **content source and its pane-local state**, not its
rectangle: the destination window's layout decides geometry. That is the one
behaviour worth copying from tmux, and the one that requires a real separation:

- today a `Surface` is `{ id, title, content, inputs }` **and** its placement
  lives in the same document, so content and geometry are welded together;
- moving a pane needs the content source to be separable from its placement.

The minimum change: a window's document references panes by address, and a pane
record stores `{ address, content, inputs }` independently of `Layout`.

## Pane nesting

**A pane does not split.** A window splits into panes; a pane is one content
source. Nested splits inside a pane would be a second layout engine beside the
one Canvas already owns, and every rule in `surface-windowing.md` about
occlusion, capture and input assumes one geometry owner.

If a view needs internal structure, that is the view's own layout in Lua, not a
new pane.

## What this changes in code

1. Rename the document concept: a session document holds windows; `pages` become
   windows with names. Keep a serde alias so existing `workspaces.json` loads.
2. Add the address type and make `canvasctl` accept it everywhere a surface id is
   accepted today.
3. Split placement from content so a pane can move.
4. Add the chord and the palette.
5. Persist the active window and pane per session, not just the active project —
   reopening should land where the person left off, which today it does not,
   because only `workspace save` writes the live document.

## Not in scope

- Multiple sessions visible on one screen at once. One screen shows one window.
- Detached sessions that keep running with no window. The process is the server.
- Remote sessions. `unibus` and `mycelium` own transport; a session is local.
