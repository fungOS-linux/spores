# The board

`crm` presents a pipeline of leads and deals for the granted CRM namespaces.
Every brand gets its own namespace, so a record belongs to exactly one pipeline
and a query for one brand cannot return another's rows.

# Records

Leads, calls and messages are records. The pipeline board is a projection of
their stage field, not a second copy: moving a card writes the record it came
from, with the version it was read at.

# Contacts

A lead may point at a contact held by the contacts Spore. The CRM reads that
projection and never edits the person; when the projection is removed the lead
stays, with no contact attached.

# What it cannot do

No network: the CRM has no network authority, so mail sync and enrichment are
host-side adapters. No raw filesystem; the host maps namespaces to books.
