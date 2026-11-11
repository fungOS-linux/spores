# Sessions

`terminal` lists the Mycelium targets visible to this machine and opens one.
The session is a host-mediated SSH PTY: the Spore presents a bounded cell grid
and receives normalized input, while routes, host keys and credentials stay in
the Mycelium capability.

A target the directory does not list cannot be addressed. The directory is the
whole of what this Spore knows about the network.

# Keys

Keys reported by the host are forwarded as input events, never as text the Spore
interprets. The VT surface is libghostty-compatible, so an application that
behaves in a native terminal behaves here.

# What it cannot do

No socket, no QUIC, no iroh, no SSH, no process, no secrets, no environment, no
clock. It draws a grid and forwards keys; the host owns the wire.
