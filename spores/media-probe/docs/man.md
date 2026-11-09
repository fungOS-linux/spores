# Purpose

`media-probe` is a fixture. It opens host-reviewed media sources, reads encoded
chunks, and reads an image back, so the media ABI can be exercised end to end
without shipping a media player.

It is not installed on a normal desktop and there is nothing to configure. Its
output is the ABI behaviour: whether a chunk arrives, whether a frame reads
back, whether the host refused something it should have refused.

# What it cannot do

No raw filesystem, no network, no process, no secrets, no clock. Sources are
host-reviewed fixtures; the Spore cannot name an arbitrary path.
