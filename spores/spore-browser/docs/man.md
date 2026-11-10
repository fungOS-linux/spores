# What it lists

`spore-browser` lists the Spore packages installed on this machine: id, version,
the summary the package declares, the surfaces it can open, and the grants it
holds. Selecting one launches a declared surface.

# Launching

Launch is a host action. The view asks `soil:spore.launch` for a surface and the
host performs it; a Spore cannot start another Spore, choose its own window, or
widen a grant on launch.

# What it cannot do

No network, no process, no secrets, no environment, no raw filesystem, no clock.
It reads the installed catalog and names an intent.
