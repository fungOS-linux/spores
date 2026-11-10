# Regions

Chrome is a region, not a window. The host keeps the extent, the input routing
and the fallback; this Spore owns the composition. It claims the top bar, dock,
launcher, switcher, control centre, window chrome, notifications and attention.

Every region has a built-in implementation in the host, so removing this Spore
restores the default shell instead of leaving a hole in the desktop. That is the
contract: a chrome Spore that fails to draw is replaced, never waited for.

# The package

There is no module. A dock is a composition, not a program; the behaviour of
each region belongs to the host that owns the input. What this package carries is
the Lua for each region and the declaration that says which regions it replaces.

# What it cannot do

No transport, no process, no secrets, no environment, no raw filesystem, no
clock. It draws; it does not decide.
