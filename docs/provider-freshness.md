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

`SIGHUP` and `systemctl --user reload owlry.service` remain coarse refresh
paths: they reload configuration, rebuild the provider manager, and re-evaluate
Lua user providers. The refresh command is the explicit, granular, scriptable
interface for refreshing cached provider data without rebuilding the full
configuration and provider set.
