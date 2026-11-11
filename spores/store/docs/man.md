# Browsing

`store` opens the storefront. The left rail is the category list — the twelve
names `soil-spore` accepts, plus All, Updates and Installed — and the main pane
is a listing of what the active source offers. A listing row opens the app;
featured cards open the same page.

Nothing is downloaded until a listing is opened, and nothing is installed until
an install is asked for.

# One listing

A listing shows a name, a publisher, a rating, the screenshots the publisher
pinned, the description, the licence, and **what the Spore can reach**: every
grant with the reason the manifest gives for it. Screenshots are content-
addressed (`ipfs://…`), so the image a listing showed is the image it still
shows.

# Reviews

A rating is bound to a package digest, not to an app name: a review of `0.1.0`
does not silently apply to `0.2.0`. Only an installed package can be reviewed,
and the host attests that at the moment the review is written. A store that
shows "verified install" means it checked that attestation.

# Updates

`store --updates` (or the Updates button) lists every installed Spore and what
applying its update would cost:

- **live** — a view-only change; Canvas hot-reloads it in the open window
- **restart** — the module changed; it opens on the new version next launch
- **consent** — the grant set grew; it waits for you
- **system** — a system surface changes; it waits for a reboot
- **held** — pinned by a source or fleet policy; it will not apply

The first two apply automatically within the rollout window. The rest wait, and
waiting is a state the window shows rather than a silent failure. "Update all
that can" never crosses a consent, system or held row.

# Sources

A source is a signed, content-addressed index, not a service to trust at request
time. The fungOS store is the default source; anyone can host another, and a
fleet usually hosts its own. The Sources button names the active source and its
index; adding one is adding a key and an origin.

# What it cannot do

Install, fetch an index, or reach a network. This package holds `soil:canvas`
and nothing else, and denies `soil:spore.catalog`, `soil:spore.install` and
`soil:http`. Installing is a host capability with its own verification and
consent; the store asks, the host decides.

# In a browser

The package also ships the same store as a page, `web/index.html`. A host web
view (or `store.myc` in a browser) presents it over the catalog API: hero art,
a rotating showcase of the listings that carry screenshots, the category chips,
the same search, and a page per package with its screenshots, grants and
reviews.

Links are shareable: `/s/<spore-id>` opens one listing, and the page keeps its
route in the URL, so a link copied out of the store reopens the same shelf.
Search is answered by the store service, not the page, so the shell, the page and
`soil-store search` cannot disagree about what matches a word.

The page adds no grant to this package. Its requests belong to the host's web
view; a `soil:http` declared here would be a capability the Lua view cannot use.
Choosing **Install** on the page is a host intent like any other, and stays
disabled until the host exposes it to the web view.
