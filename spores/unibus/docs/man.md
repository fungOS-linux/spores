# What it shows

`inbox` presents mail threads and chat conversations from the Unibus metadata
frames this machine observes. Bodies are not cached by default; what is stored is
the envelope the actions need: sender, subject, thread, labels and read state.

# Actions

`mail.read`, `mail.unread`, `mail.archive` and `mail.tag` are declared actions,
not UI buttons. Each names its records input, the grant it requires and who may
call it (`human`, `agent`). An agent calling `mail.archive` is the same operation
as a person clicking it, which is what makes the history answerable.

# Cache

Observed metadata is cached as records so the inbox opens without the bus. A
message that moved on the server shows as stale until the next observation; the
cache never invents a state.

# What it cannot do

It cannot send, reply, or delete. Sending is a separate Spore with a separate
grant set (`dev.fungos.mail`), so "is this allowed to send?" is answered by a
manifest, never by auditing the view.
