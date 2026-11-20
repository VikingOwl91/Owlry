//! Move spawned apps into their own transient systemd user scope.
//!
//! With `use_uwsm = false` owlry spawns apps as direct child processes, so
//! they land in owlry's own scope (e.g. `app-niri-owlry-<rand>.scope`) and
//! share its fate under monitoring, systemd-cgtop, OOM handling and "stop
//! the scope". This module does what uwsm does: after spawning, move the
//! child's PID into a transient `app-owlry-<appid>-<rand>.scope` under
//! `app-graphical.slice` via the user systemd manager's
//! `org.freedesktop.systemd1.Manager.StartTransientUnit`. The PID keeps its
//! identity, so the returned `std::process::Child` handle (kill/wait and
//! stdin/stdout pipes) keeps working. When no session bus or systemd user
//! manager is reachable, the child simply stays a direct child.

use std::process::Child;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use zbus::blocking::connection::Builder as ConnectionBuilder;
use zbus::blocking::{Connection, Proxy};
use zbus::names::BusName;
use zbus::zvariant::{OwnedObjectPath, Value};

const SYSTEMD_NAME: &str = "org.freedesktop.systemd1";
const MANAGER_PATH: &str = "/org/freedesktop/systemd1";
const MANAGER_IFACE: &str = "org.freedesktop.systemd1.Manager";
const SLICE: &str = "app-graphical.slice";
const COLLECT_MODE: &str = "inactive-or-failed";

/// `a{sv}` property vector for `StartTransientUnit`.
type Props = Vec<(String, Value<'static>)>;

/// Cached session-bus connection. `Some` only when a session bus exists and
/// the systemd user manager owns `org.freedesktop.systemd1` on it; `None`
/// means direct-spawn fallback. Method calls are bounded to 2 s so a hung
/// user manager cannot block the caller for the ~25 s bus default.
static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();

fn connection() -> Option<&'static Connection> {
    CONNECTION
        .get_or_init(|| {
            let conn = ConnectionBuilder::session()
                .ok()?
                .method_timeout(std::time::Duration::from_secs(2))
                .build()
                .ok()?;
            let dbus = zbus::blocking::fdo::DBusProxy::new(&conn).ok()?;
            let name = BusName::try_from(SYSTEMD_NAME).ok()?;
            dbus.get_name_owner(name).ok()?;
            Some(conn)
        })
        .as_ref()
}

/// Move `child` into its own transient scope `app-owlry-<appid>-<rand>.scope`
/// under `app-graphical.slice`. Best-effort and never panics: without a user
/// manager the child stays in owlry's scope (previous behavior), and a failed
/// move costs at most one D-Bus roundtrip plus one retry.
pub fn scope_child(child: &Child, appid: &str) {
    let Some(conn) = connection() else {
        return;
    };
    let unit = unit_name(appid);
    let pid = child.id();
    if start_transient_unit(conn, &unit, scope_props(pid, appid)).is_err()
        && start_transient_unit(conn, &unit, minimal_props(pid)).is_err()
    {
        log::debug!("scope: could not move pid {pid} into {unit}; leaving it in owlry's scope");
    }
}

/// `StartTransientUnit(unit, "fail", props, [])` on the user manager.
fn start_transient_unit(conn: &Connection, unit: &str, props: Props) -> zbus::Result<()> {
    let manager = Proxy::new(conn, SYSTEMD_NAME, MANAGER_PATH, MANAGER_IFACE)?;
    let aux: Vec<(String, Vec<(String, Value<'static>)>)> = Vec::new();
    let _: OwnedObjectPath = manager.call(
        "StartTransientUnit",
        &(unit.to_string(), "fail".to_string(), props, aux),
    )?;
    Ok(())
}

/// Full property set: PIDs, Slice, CollectMode, Description.
fn scope_props(pid: u32, appid: &str) -> Props {
    vec![
        ("PIDs".to_string(), Value::new(vec![pid])),
        ("Slice".to_string(), Value::new(SLICE)),
        ("CollectMode".to_string(), Value::new(COLLECT_MODE)),
        (
            "Description".to_string(),
            Value::new(format!("owlry: {appid}")),
        ),
    ]
}

/// Retry property set without the optional Slice/Description, in case the
/// manager rejects them (e.g. no app-graphical.slice on ancient systemd).
fn minimal_props(pid: u32) -> Props {
    vec![
        ("PIDs".to_string(), Value::new(vec![pid])),
        ("CollectMode".to_string(), Value::new(COLLECT_MODE)),
    ]
}

/// Unit name per the systemd convention `app-<launcher>-<appid>-<random>.scope`.
fn unit_name(appid: &str) -> String {
    format!(
        "app-owlry-{}-{}.scope",
        sanitize_appid(appid),
        random_suffix()
    )
}

/// Map an appid to a unit-name-safe component: keep ASCII alnum and
/// `: - _ .`, map everything else to `-`. Empty becomes `app`; the result is
/// capped at 128 bytes (all ASCII after mapping, so byte-truncation is safe).
fn sanitize_appid(appid: &str) -> String {
    let mut out = String::with_capacity(appid.len().min(128));
    for ch in appid.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, ':' | '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.truncate(128);
    if out.is_empty() {
        out.push_str("app");
    }
    out
}

/// 8 hex chars of entropy: low 32 bits of (time nanos ^ pid ^ counter * golden ratio).
fn random_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = u128::from(std::process::id());
    let counter = u128::from(COUNTER.fetch_add(1, Ordering::Relaxed));
    let mix = nanos ^ pid ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{:08x}", (mix & 0xFFFF_FFFF) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_name_follows_app_owlry_convention() {
        let name = unit_name("org.gnome.gedit.desktop");
        assert!(
            name.starts_with("app-owlry-org.gnome.gedit.desktop-"),
            "{name}"
        );
        assert!(name.ends_with(".scope"), "{name}");
        assert!(name.len() <= 255, "{name} is {} bytes", name.len());
        // fixed parts: "app-owlry-" + "-" + 8 hex + ".scope"
        let suffix = name
            .strip_prefix("app-owlry-org.gnome.gedit.desktop-")
            .and_then(|s| s.strip_suffix(".scope"))
            .expect("shape");
        assert_eq!(suffix.len(), 8, "suffix {suffix:?}");
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()), "{suffix}");
    }

    #[test]
    fn unit_name_sanitizes_hostile_appids() {
        let name = unit_name("org.weird/App;rm -rf");
        assert!(
            name.starts_with("app-owlry-org.weird-App-rm--rf-"),
            "{name}"
        );
        assert!(name.ends_with(".scope"), "{name}");
        assert!(name.len() <= 255);
        for ch in name.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || matches!(ch, '-' | ':' | '_' | '.'),
                "unexpected char {ch:?} in {name}"
            );
        }
    }

    #[test]
    fn unit_name_empty_appid_becomes_app() {
        let name = unit_name("");
        assert!(name.starts_with("app-owlry-app-"), "{name}");
        assert!(name.ends_with(".scope"), "{name}");
    }

    #[test]
    fn unit_name_caps_long_appids() {
        let long = "a".repeat(500);
        let name = unit_name(&long);
        assert!(name.len() <= 255, "{} bytes", name.len());
        // 10 + 128 + 1 + 8 + 6 = 153
        assert_eq!(name.len(), 153, "{name}");
    }

    #[test]
    fn unit_name_is_unique_across_calls() {
        let names: Vec<String> = (0..100).map(|_| unit_name("x.desktop")).collect();
        let unique: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(unique.len(), names.len(), "duplicate unit names");
    }

    #[test]
    fn scope_props_carry_pids_slice_collectmode_description() {
        let props = scope_props(4242, "foo.desktop");
        assert_eq!(props.len(), 4);

        let pids = props.iter().find(|(k, _)| k == "PIDs").expect("PIDs");
        match &pids.1 {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 1);
                assert!(matches!(arr[0], Value::U32(p) if p == 4242));
            }
            other => panic!("PIDs should be au, got {other:?}"),
        }

        let slice = props.iter().find(|(k, _)| k == "Slice").expect("Slice");
        assert!(matches!(&slice.1, Value::Str(s) if s == SLICE));

        let collect = props
            .iter()
            .find(|(k, _)| k == "CollectMode")
            .expect("CollectMode");
        assert!(matches!(&collect.1, Value::Str(s) if s == COLLECT_MODE));

        let desc = props
            .iter()
            .find(|(k, _)| k == "Description")
            .expect("Description");
        assert!(matches!(&desc.1, Value::Str(s) if s == "owlry: foo.desktop"));
    }

    #[test]
    fn minimal_props_carry_only_pids_and_collectmode() {
        let props = minimal_props(7);
        assert_eq!(props.len(), 2);
        assert!(props.iter().any(|(k, _)| k == "PIDs"));
        assert!(props.iter().any(|(k, _)| k == "CollectMode"));
        match &props[0].1 {
            Value::Array(arr) => assert!(matches!(arr[0], Value::U32(p) if p == 7)),
            other => panic!("PIDs should be au, got {other:?}"),
        }
    }

    #[test]
    fn scope_child_returns_promptly_without_a_user_bus() {
        // CI has no session bus / systemd user manager: scope_child must not
        // panic and must return promptly (fallback = plain direct spawn).
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("sleep should spawn");
        let started = std::time::Instant::now();
        scope_child(&child, "ci-test.desktop");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "scope_child blocked for {:?}",
            started.elapsed()
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}
