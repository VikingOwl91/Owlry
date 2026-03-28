# Config Editor — Design Spec

## Goal

A built-in provider in owlry-core that lets users browse and modify their configuration directly from the launcher UI, without opening a text editor.

## Scope

### Editable settings (curated)

**Provider toggles** (boolean):
- applications, commands, calculator, converter, system
- websearch, ssh, clipboard, bookmarks, emoji, scripts, files
- media, weather, pomodoro
- uuctl (systemd user units)

**Appearance** (text input + selection):
- theme (selection from available themes)
- font_size (numeric input)
- width, height (numeric input)
- border_radius (numeric input)

**Search** (text input + selection):
- search_engine (selection: google, duckduckgo, bing, startpage, brave, ecosia)
- frecency (boolean toggle)
- frecency_weight (numeric input, 0.0–1.0)

**Profiles** (CRUD):
- List existing profiles
- Create new profile (name input + mode checklist)
- Edit profile (rename, edit modes, delete)

### Not in scope

- Weather API key / location (sensitive, better in config file)
- Pomodoro durations (niche, config file)
- Plugin disabled list (covered by provider toggles)
- use_uwsm / terminal_command (advanced, config file)

## UX Flow

### Entry point

Type `:config` or select the "Settings" item that appears for queries like "settings", "config", "preferences".

### Top-level categories

```
:config →
  ┌─ Providers          Toggle providers on/off
  ├─ Appearance          Theme, font size, dimensions
  ├─ Search              Search engine, frecency
  └─ Profiles            Manage named mode sets
```

Each category is a submenu item. Selecting one opens its submenu.

### Provider toggles

```
Providers →
  ┌─ ✓ Applications     [toggle]
  ├─ ✓ Commands          [toggle]
  ├─ ✓ Calculator        [toggle]
  ├─ ✓ Converter         [toggle]
  ├─ ✓ System            [toggle]
  ├─ ✗ Weather           [toggle]
  ├─ ...
```

Selecting a row toggles it. The ✓/✗ prefix updates immediately. Change is written to `config.toml` and hot-applied where possible.

### Appearance settings

```
Appearance →
  ┌─ Theme: owl                    [select]
  ├─ Font Size: 14                 [edit]
  ├─ Width: 850                    [edit]
  ├─ Height: 650                   [edit]
  └─ Border Radius: 12             [edit]
```

**Selection fields** (theme): Selecting opens a submenu with available options. Current value is marked with ✓.

**Text/numeric fields** (font size, width, etc.): Selecting a row enters edit mode — the search bar clears and shows a placeholder like "Font Size (current: 14)". User types a new value and presses Enter. The value is validated (numeric, within reasonable range), written to config, and the submenu re-displays with the updated value.

### Search settings

```
Search →
  ┌─ Search Engine: duckduckgo     [select]
  ├─ Frecency: enabled             [toggle]
  └─ Frecency Weight: 0.3          [edit]
```

Same patterns — selection for engine, toggle for frecency, text input for weight.

### Profile management

```
Profiles →
  ┌─ dev (app, cmd, ssh)           [submenu]
  ├─ media (media, emoji)          [submenu]
  └─ ➕ Create New Profile         [action]
```

**Select existing profile** → submenu:
```
Profile: dev →
  ┌─ Edit Modes                    [submenu → checklist]
  ├─ Rename                        [text input]
  └─ Delete                        [confirm action]
```

**Edit Modes** → checklist (same as provider toggles but for the profile's mode list):
```
Edit Modes: dev →
  ┌─ ✓ app
  ├─ ✓ cmd
  ├─ ✗ calc
  ├─ ✗ conv
  ├─ ✓ ssh
  ├─ ...
```

Toggle to include/exclude. Changes saved on submenu exit (Escape).

**Create New Profile**:
1. Search bar becomes name input (placeholder: "Profile name...")
2. User types name, presses Enter
3. Opens mode checklist (all unchecked)
4. Toggle desired modes, press Escape to save

**Delete**: Selecting "Delete" removes the profile from config and returns to the profiles list.

## Architecture

### Provider type

Built-in static provider in owlry-core. Uses `ProviderType::Plugin("config")` with prefix `:config`.

### Provider classification

**Static** — the top-level items (Providers, Appearance, Search, Profiles) are populated at refresh time. But it also needs **submenu support** — each category opens a submenu with actions.

This means the config provider needs to handle `?SUBMENU:` queries to generate submenu items dynamically, and `!ACTION:` commands to execute changes.

### Command protocol

Actions use the existing plugin action system (`PluginAction` IPC request):

- `CONFIG:toggle:providers.calculator` — toggle a boolean
- `CONFIG:set:appearance.font_size:16` — set a value
- `CONFIG:set:providers.search_engine:google` — set a string
- `CONFIG:profile:create:dev` — create a profile
- `CONFIG:profile:delete:dev` — delete a profile
- `CONFIG:profile:rename:dev:development` — rename
- `CONFIG:profile:mode:dev:toggle:ssh` — toggle a mode in a profile

### Config persistence

All changes write to `~/.config/owlry/config.toml` via the existing `Config::save()` method.

### Hot-apply behavior

| Setting | Hot-apply | Notes |
|---------|-----------|-------|
| Provider toggles | Yes | Daemon re-reads config, enables/disables providers |
| Theme | Yes | UI reloads CSS |
| Frecency toggle/weight | Yes | Next search uses new value |
| Search engine | Yes | Next web search uses new engine |
| Font size | Restart | CSS variable, needs reload |
| Width/Height | Restart | GTK window geometry set at construction |
| Border radius | Restart | CSS variable, needs reload |
| Profiles | Yes | Config file update, available on next `--profile` launch |

Settings that require restart show a "(restart to apply)" hint in the description.

### Submenu integration

The config provider uses the existing submenu system:
- Top-level items have `SUBMENU:config:{category}` commands
- Categories return action items via `?SUBMENU:{category}`
- Actions execute via `CONFIG:*` commands through `execute_plugin_action`

This keeps the implementation within the existing provider/submenu architecture without new IPC message types.
