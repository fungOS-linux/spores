# Contacts

`contacts` keeps personal contacts in the granted records namespace. A contact
has a name, the ways it can be reached, and whatever notes the person chose to
keep. Editing is a records write with an expected version, so two editors cannot
silently overwrite each other.

# CRM projections

A contact becomes a CRM lead only through an explicit projection into a named
brand namespace. The projection is a separate record that points at the contact,
so removing it from a CRM never deletes the person, and a CRM cannot read a
contact that was never projected.

# What it cannot do

No network: contact data has no network authority, so enrichment and verification
are host-side adapters, not something this Spore can call. No raw filesystem; the
host maps namespaces to books.
