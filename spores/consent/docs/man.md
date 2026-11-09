# What it asks

Whenever a capability set is about to be granted, the host asks. This Spore
draws that question: what is asking, what it wants, the reason it gave, and what
changes if you say yes. Approve and Deny are the only answers; there is no
"later" that quietly becomes yes.

# Who may ask

Unibus, the Spore Store and outbound actions all produce the same request shape.
The popup does not know or care which produced it. The host owns the decision
record; the popup is its visible half, and it must be able to outlive the window
that raised the request.

# What it cannot do

It cannot grant anything itself, remember a consent the host did not record, or
widen a request. No network, no process, no secrets, no clock.
