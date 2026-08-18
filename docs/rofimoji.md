# Rofimoji integration

Owlry has an optional `rofimoji` provider. The provider exposes the installed
rofimoji launch modes and imports its `kaomoji.csv` as searchable, copyable
Owlry results. Selecting the full picker entry starts the installed `rofimoji`
program, which keeps its own fuzzy search, frecency, skin-tone handling and
insertion actions.

Enable it in Lua with:

```lua
owlry.providers {
  "app", "cmd", "emoji", "rofimoji",
}
```

The provider exposes combined emoji/kaomoji, emoji-only, kaomoji-only, and
clipboard entries, plus the installed kaomoji database. Searching `shrug`
also finds rofimoji's `whatever` entries. It adds explicit entries for installed rofimoji
selectors (`rofi`, `wofi`, `fuzzel`, `dmenu`, `tofi`, `bemenu`, `wmenu`, and
`choose`) and typers (`xdotool`, `wtype`, `ydotool`, `cliclick`, and
`wl-ime-type`). The automatic entries defer backend selection to rofimoji.

If the data file is installed in a non-standard location, set
`ROFIMOJI_DATA=/path/to/kaomoji.csv`.

`rofimoji` is deliberately an Arch `optdepend`; Owlry simply hides this
provider when the executable is not installed. The provider can therefore be
compiled into the full build without making the external Python application a
hard dependency.

## Selector protocol

`owlry selector` (also available as `owlry dmenu`) implements the line-oriented
selector contract used by rofimoji: it reads one candidate per line from stdin
and writes the selected candidate to stdout. This is the stable integration point
for a future upstream `rofimoji --selector owlry` adapter. The current rofimoji release does
not list `owlry` among its selector choices, so this repository does not claim
that command line until the corresponding adapter is accepted upstream.

The adapter can invoke:

```bash
printf '%s\n' ... | owlry selector --prompt '😀 '
```
