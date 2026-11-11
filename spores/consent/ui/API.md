# Consent — Lua API

Consent is a **native** surface. The trusted popup is
`soil-consent-popup`; the Lua file is a descriptor so `tend` and the host can
see that the package has no Canvas view, not a second implementation of the
prompt.

## Entrypoint

`ui/consent.lua` returns a descriptor table:

```lua
{ title = "Consent", status = "native host popup",
  note = "The trusted consent surface is the native soil-consent-popup binary." }
```

There is no `M.render`. A host that composes the package gets the descriptor and
routes the actual prompt to the native binary.

## Snapshot

None. The prompt's content is host-owned and never crosses into Lua; a consent
dialog must not be renderable by a guest that could be tricked into asking for
the wrong grant.

## Actions

None from Lua. Approve/deny are host UI events, delivered by the native popup
to the host, not intents a Spore can emit.

## Slots

None.

## Components

None.

## Theme/config

None. The native popup follows the host theme; there is no Lua scene to retheme.

## Stable ids

The package id is `dev.fungos.consent`; the descriptor is the package's only
surface contract.

## Authority

None. Keeping the prompt out of the sandbox is the point: a Spore cannot draw
its own consent UI, so it cannot forge one.
