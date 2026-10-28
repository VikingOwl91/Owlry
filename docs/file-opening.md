# Opening files and folders

Filesystem results from `:file` (or `/`) open with the system's default
application for the path's MIME type, the same association `xdg-open` uses.
How Owlry launches the handler depends on what the `.desktop` entry declares.

- **GUI handler** (a browser, editor, image viewer, or file manager):
  launched through `xdg-open`.
- **Terminal handler** (`Terminal=true`, for example `nvim`, `vim`, or
  `less`): Owlry reads the entry's `Exec` line and runs it inside the
  configured terminal, for example `kitty -e nvim <file>`. Plain `xdg-open`
  would launch these windowless, so without this they appear to do nothing.
- **No associated application**: Owlry opens the "Open with..." picker instead
  of failing silently.

The terminal command comes from `general.terminal_command`, or is detected
automatically. Terminal handlers use the `-e` convention; terminals that need
a different flag may not receive the file argument correctly. Set
`terminal_command` in `config.toml` or `terminal` in `owlry.set` if your
terminal needs an explicit command.

## Open with picker

Press `Shift+Enter` on a file result to choose an application. Files whose
MIME type has no default application open the picker on plain `Enter` as well.

- `Enter` opens the file with the chosen application once. Nothing is saved.
- `Shift+Enter` sets that application as the default for the file's MIME type
  and then opens the file. The change is written to `~/.config/mimeapps.list`
  through `xdg-mime`.

The chosen application is launched the same way as a default handler, so a
`Terminal=true` application still opens inside the terminal.

To change a default without the picker:

```bash
xdg-mime default dev.zed.Zed.desktop text/x-c
```

## Handler safety

Owlry parses the desktop entry's `Exec` value into an argument vector and
expands the standard field codes. It refuses a handler that explicitly
invokes a shell on the terminal-handler and open-with paths, and falls back
to `xdg-open` or `gio launch`.

The shell-wrapper check is deliberately conservative and heuristic. It covers
shells and common wrappers, but an unusual wrapper spelling can still get
past it. Owlry does not add a shell of its own; this only affects handlers
that already contain one.

A refused terminal handler falls back to `xdg-open`, and a refused open-with
handler falls back to `gio launch`. Those tools resolve the same desktop
entry through the desktop's own configuration.
