# The report

`dfm-report` shows design-for-manufacture findings for one model. Each finding
names the feature it is about, the rule it breaks, and the severity. Selecting a
finding highlights the geometry it refers to.

# Rendering

Geometry is rendered by this Spore into one Canvas-owned frame. Canvas owns the
window, the input and the lifetime; the Spore owns the camera, the materials and
the highlight. That is why a finding can point at a feature instead of
describing it in prose.

# Data

Findings are host-supplied. The model and its scene come from the bundle, so the
report renders without a backend and without a network path.

# What it cannot do

No network: findings are supplied, not fetched. No raw filesystem: the host owns
the workspace root.
