# Dropping a file

Drag a file onto the Spore. It reads the source by workspace path, decides which
operations the file's media type supports, and shows them as a ring: the format
to convert to, the trim or resize to apply, and the destination name. Nothing
runs until a segment is chosen.

# Operations

An operation declares its input media type, its output media type and the
program it maps to. Conversion, trimming, resizing and extraction are operations;
the ring is a view of the set, not a hard-coded menu.

# Undo

Each completed operation writes a receipt and offers Undo. Undo restores the
input path and removes the output this operation wrote, using the receipt rather
than guessing from the file name.

# What it cannot do

No process: the conversions run in the host, which owns the encoder. No network,
no secrets, no raw filesystem, no clock.

# Status

The operation model and its tests are the current package. The radial view waits
on a Canvas primitive with a ring; a list pretending to be a menu is worse than
no menu.
