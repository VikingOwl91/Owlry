# Theming

Owlry uses GTK4 CSS. A theme can set the core palette and, optionally,
per-provider badge colors.

## Built-in themes

| Theme | Description |
|---|---|
| `owl` | Dark theme with amber accents |
| `catppuccin-mocha` | Soothing pastel |
| `nord` | Arctic blue palette |
| `rose-pine` | Natural pine tones |
| `dracula` | Dark vampire theme |
| `gruvbox-dark` | Retro groove |
| `tokyo-night` | Tokyo city lights |
| `solarized-dark` | Precision colors |
| `one-dark` | Atom's One Dark |
| `apex-neon` | Neon cyberpunk |

Select one in Lua:

```lua
owlry.theme("catppuccin-mocha")
```

Or in TOML:

```toml
[appearance]
theme = "catppuccin-mocha"
```

## Custom theme

Create `~/.config/owlry/themes/mytheme.css`:

```css
:root {
    --owlry-bg: #1e1e2e;
    --owlry-bg-secondary: #313244;
    --owlry-border: #45475a;
    --owlry-text: #cdd6f4;
    --owlry-text-secondary: #a6adc8;
    --owlry-accent: #f38ba8;
    --owlry-accent-bright: #f5c2e7;
}
```

```lua
owlry.theme("mytheme")
```

`~/.config/owlry/style.css` is applied after the selected theme, so it can
override individual values without copying a full theme.

## CSS variables

| Variable | Purpose |
|---|---|
| `--owlry-bg` | Main background |
| `--owlry-bg-secondary` | Secondary surfaces |
| `--owlry-border` | Border color |
| `--owlry-text` | Primary text |
| `--owlry-text-secondary` | Muted text |
| `--owlry-accent` | Accent color |
| `--owlry-accent-bright` | Bright accent |
| `--owlry-shadow` | Window shadow (default: none) |

## Color overrides in configuration

Lua:

```lua
owlry.theme {
  background = "#1e1e2e",
  accent = "#cba6f7",
  badge_app = "#a6e3a1",
  badge_power = "#f38ba8",
}
```

TOML:

```toml
[appearance.colors]
background = "#1e1e2e"
accent = "#cba6f7"
badge_app = "#a6e3a1"
badge_power = "#f38ba8"
```

Badge keys follow the provider names: `badge_app`, `badge_cmd`, `badge_calc`,
`badge_clip`, `badge_dmenu`, `badge_emoji`, `badge_file`, `badge_power`
(alias `badge_sys`), `badge_ssh`, `badge_uuctl`, and `badge_web`.

See [Lua API](lua-api.md) section 4.5 for the full theme surface.
