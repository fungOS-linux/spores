# Purpose

`surface-probe` is a fixture. It imports one host-reviewed surface, receives
normalized input events, and reports what arrived, so the surface ABI can be
tested without a real application.

It is not installed on a normal desktop and there is nothing to configure. Its
output is the ABI behaviour: which events arrive, in what coordinate space, and
whether the host refused an import it should have refused.

# What it cannot do

No raw filesystem, no network, no process, no secrets, no clock. The surface it
imports is a fixture the host reviews, not a path the Spore chooses.
