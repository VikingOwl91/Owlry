# Provider Freshness

Static providers keep an item cache. `owlry refresh` rebuilds every static
provider cache, and `owlry refresh <id>` rebuilds one provider. Provider IDs
from `owlry providers` work, along with existing aliases such as `application`
for `app` and `systemd` for `uuctl`.

The application provider also watches the XDG application directories it
scans. Changes to `.desktop` files are debounced and refresh only application
entries, so installed and removed applications appear without restarting the
daemon. If watching cannot start, Owlry logs a warning and `owlry refresh app`
remains available.

`uuctl` discovers and caches systemd **user units** using `systemctl --user`.
It does not manage system-wide units. Unit discovery can be refreshed with
`owlry refresh uuctl`; when a unit submenu opens, Owlry resolves its active
state at that time so Start and Stop are not based on daemon-start state.

Providers whose items come from an external program declare it via
`Provider::backing_program`. The daemon re-checks whether that program is on
`PATH` when a query arrives, at most once every two seconds per provider, and
rebuilds the provider's cache only when the answer changed. Installing or
removing such a program therefore takes effect on the next query, with no
restart and no manual refresh — which matters because these programs need not
ship a `.desktop` file, so the application-directory watcher never sees them.

| Provider | Backing program |
|---|---|
| `rofimoji` | `rofimoji` |
| `clipboard` | `cliphist` |

The check is a stat per `PATH` entry, about a microsecond when the program is
installed. `refresh()` — the expensive part — runs only on a change, so the
steady state costs the check and nothing else. `filesearch` gates on `fd` /
`locate` but is a `DynamicProvider`, so it re-checks per query already and
needs no gate.

`owlry doctor` and `owlry providers` report each static provider's cached item
count, and say `MISSING: \`<program>\` not on PATH` for a gated provider whose
program is absent. A registered provider sitting at zero items is otherwise
indistinguishable from a working one.

`SIGHUP` and `systemctl --user reload owlry.service` remain coarse refresh
paths: they reload configuration, rebuild the provider manager, and re-evaluate
Lua user providers. The refresh command is the explicit, granular, scriptable
interface for refreshing cached provider data without rebuilding the full
configuration and provider set.
