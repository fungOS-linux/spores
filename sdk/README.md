# SDK: `fungos.ui`

The UI library Spores draw with: declarative trees, one-pass layout, and every
value taken from the host's theme tokens.

This directory is the package source, not a copy. A Spore consumes it the same way
it consumes any other artifact — composition writes the resolved `ui.lua` into the
module it ships, and `spore.lock` covers the result.

## Why it is a package

Every Spore used to carry its own `ui/lib/ui.lua`. Two agents editing two copies is
how one shell ends up with three definitions of "small gap". One versioned package
means a fix lands everywhere, and a Spore that wants a different answer has to say
so with a version, not with a divergent copy.

## What lives here

- `ui.lua` — the library: `column`, `row`, `card`, `stack`, `radial`, `text`,
  `rule`, `code`, `diagram`, `bullets`, `quote`, `bar`, `fill`, `button`, `kbd`,
  `icon`, `icon_button`, plus the layout pass (`lay`) and the token helpers.
- `package.toml` — name, version, entry.

## Rules the library enforces

1. A view is a pure function of its snapshot; it holds no state.
2. Interaction is an action (`ui.action("save")`), never a callback into the host.
3. Layout is single-pass; nothing measures twice, so nothing can disagree with
   itself.
4. No view names a colour or a pixel: `ink`, `style`, `space`, `radius` come from
   the theme, so retheming moves the whole interface.

## Still to land here

- `tokens.lua`: `elevation`, `stroke`, `glass`, `density`, `motion`, so chrome and
  apps share one visual system.
- `components.lua`: `pill`, `tile`, `list-row`, `gauge`, `section`, `glass-panel`.
- Chrome regions migrated onto this library (`control-center` first) so the shell
  and the apps stop looking like two products.
