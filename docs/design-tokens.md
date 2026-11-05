# Design tokens, components, and the rules worth enforcing

Status: proposal. This is the concrete schema under `docs/ui-model.md`.

## Tokens

One table, resolved by the host from the theme, read-only in a view.

```lua
tokens = {
  color = {
    background, surface, surface_raised, surface_sunken,
    border, border_strong, divider, scrim,
    text, text_secondary, text_disabled,
    accent, accent_text, success, warning, danger,
  },
  type  = { micro, small, body, large, title, display, hero },   -- px
  space = { xs=4, sm=8, md=16, lg=28, xl=44 },                    -- logical px
  radius= { sm=4, md=8, lg=12, pill=999 },
  border= { hair=1, thin=2 },
  shadow= { none, low, high },
  motion= { fast=120, base=180, slow=280 },                       -- ms
  chrome= { titlebar=28, caption=18, grip=8, inset=4 },           -- chrome metrics
}
```

Every one of those is a *role*. A theme supplies values; a view names roles.

## Components

| Component | Props | Variants |
|---|---|---|
| `text` | `value`, `style`, `ink`, `chars` | named styles only |
| `button` | `label`, `action`, `disabled` | `primary`, `secondary`, `ghost`, `danger` |
| `icon_button` | `glyph`, `action`, `label` | quiet / selected / disabled |
| `column`/`row`/`stack` | `gap`, `widths`, `align`, `grow` | token gaps only |
| `card` | `gap`, padding | radius `md` |
| `rule`, `fill` | `ink`, `opacity` | — |
| `code`, `diagram`, `bullets`, `quote` | content | — |

Components take **variants**, not styles. A view that reaches for `color = "#fff"`
is doing something the token set does not support; that is a signal the token set
is incomplete, not that the view is clever.

## Enforceable rules

Checked in the library (cheap, immediate, visible in the loop):

1. `gap`, padding, radius, and border **must** be token names or token multiples.
   A raw number is rejected with the token list in the message.
2. `style` must be one of the named text styles.
3. `ink` must be a semantic color role.
4. An interactive node must declare a hit target ≥ 32 logical px; visual size may
   be smaller.
5. At most **one** `primary` button per view; more is a warning naming the extras.
6. Chrome radius follows the rule: `lg` for windows, `md` for controls, `0` when
   maximized or snapped.
7. Nodes that participate in diffing carry explicit `id`s.

Rules 1–3 are hard errors; 4–5 are warnings the view can render as a badge in dev
mode; 6–7 are enforced by the host.

## Chrome, in this vocabulary

Chrome is not special: it is a component made of the same tokens.

```lua
chrome.window {
  title = "Field notes",
  subtitle = "spore / markdown / draft",
  active = true,
  maximized = false,
  buttons = { "minimize", "maximize", "close" },
  body = content,          -- the pane's tree
}
```

- `active = false` maps to a token variant (dimmer surface, muted text, weaker
  shadow), not to a different component.
- `maximized = true` drops the radius to `0` and hides the shadow, so edges meet
  the display cleanly.
- Caption order and glyphs are themeable; the *actions* and hit regions are not.
- The host hit-tests the regions it computed, so a themed chrome cannot fake a
  close button.

## Ownership decision

Shell-drawn decorations by default (consistent, themeable, one hit-test model).
A pane may request `chrome = "custom"` and draw its own frame, but it must still
declare the regions and actions, and Canvas still performs the hit-testing. That
is the hybrid model: **visual expression is negotiable; the contract is not.**

## Order of work

1. Token table in `module.theme` (roles above, including `chrome` metrics).
2. Library rules 1–3 as hard errors.
3. `chrome.window` component over the built-in frame, so a themed chrome is a
   view, not a fork.
4. Rules 4–5 as dev warnings.
5. Custom-chrome negotiation last, once the default is good enough that nobody
   needs it.
