# Contact connectors

Contacts is a records-only Spore. It must never import a mail, phone, chat, or
calendar capability directly.

Connectors are optional, removable producers that write bounded observation
records into the same records store. Contacts reads them if present. If no
connector is installed, Contacts still works with names, orgs, tags, favorites,
and vCard dates.

## Collections

```text
contact                 vCard-shaped contact records
contact_preference      favorites and tags
contact_interaction     bounded interaction summaries
```

No raw message bodies, call recordings, credentials, or provider tokens enter
these collections.

## `contact_preference`

Key: contact id.

```json
{
  "favorite": true,
  "tags": ["assistant", "scheduling"]
}
```

Written by Contacts actions, a Settings/Favorites Spore, or a user dotfile
importer. Not owned by mail or telephony connectors.

## `contact_interaction`

Key: stable source-scoped key, e.g. `agentmail:email:amy@x.ai`.

```json
{
  "contact_id": "contact-...",
  "identity_kind": "email",
  "identity": "amy@x.ai",
  "source": "agentmail",
  "kind": "email",
  "last_at_ms": 1762390000000,
  "count": 42,
  "direction": "mixed"
}
```

Allowed identities:

```text
contact_id      direct association
identity_kind   email | phone | handle
identity        normalized email, E.164-ish phone, or provider handle
source          agentmail | imessage | phone | calendar | ...
kind            email | call | message | meeting | ...
```

The record is an aggregate, not an event log. A connector should upsert one row
per `(contact/identity, source, kind)` and update `last_at_ms` and `count`.
This keeps the records book bounded regardless of mailbox size.

## Connector examples

```text
AgentMail connector
  grant: mail metadata read
  writes: contact_interaction { source = "agentmail", kind = "email" }

Call-history connector
  grant: telephony history read
  writes: contact_interaction { source = "phone", kind = "call" }

iMessage connector
  grant: chat metadata read
  writes: contact_interaction { source = "imessage", kind = "message" }

Calendar connector
  grant: calendar read
  writes: contact_interaction { source = "calendar", kind = "meeting" }
```

Connectors may run as Spores, host adapters, or one-shot host jobs. The contract
is the records collection, not a direct dependency on Contacts.

## Matching

Contacts matches, in order:

1. `contact_id`
2. normalized email against every contact email
3. normalized phone digits against every contact phone, allowing country-code
   prefix differences

Unmatched observations are ignored. A connector may choose to resolve and write
`contact_id` when it knows the association.

## Derived fields

Contacts computes from records only:

```text
favorite
tags
last_contacted     max(contact_interaction.last_at_ms)
interactions       matched summaries
updated_at_ms      record revision / HLC
created            vCard CREATED, when present
last_modified      vCard LAST-MODIFIED or REV, when present
```

MRU is not a connector. It is a projection of the observations.

## Failure rules

- Missing connector = no interactions, never an error.
- Malformed observation = skipped, never a failed contacts load.
- Unmatched identity = ignored.
- Connector may not widen Contacts grants.
- Contacts may not call a connector directly.
- Raw content stays in the source system.
