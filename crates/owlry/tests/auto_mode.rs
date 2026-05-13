//! Auto-mode integration test (Phase 1 task #10, D7).
//!
//! Pins down the invariant that `owlry` invoked with no `-m`/`--profile`/CLI
//! restriction surfaces results from every enabled provider, with tabs
//! reflecting `general.tabs`. This is the existing default behavior that
//! refactors in subsequent phases must not silently regress.

use owlry::config::ProvidersConfig;
use owlry::filter::ProviderFilter;
use owlry::providers::ProviderType;

/// Build a filter the same way `Server::bind` does when the user runs `owlry`
/// with no CLI mode and no profile: `cli_mode=None`, `cli_providers=None`.
fn auto_mode_filter(tabs: &[&str]) -> ProviderFilter {
    let cfg = ProvidersConfig::default();
    let tabs: Vec<String> = tabs.iter().map(|s| s.to_string()).collect();
    ProviderFilter::new(None, None, &cfg, &tabs)
}

#[test]
fn auto_mode_filter_accepts_every_provider_type() {
    // Even providers the user didn't list in `general.tabs` must remain
    // searchable — tabs only drive UI display, not query routing.
    let filter = auto_mode_filter(&["app", "cmd", "power"]);

    assert!(filter.is_accept_all(), "auto mode must set accept_all=true");

    assert!(filter.is_active(ProviderType::Application));
    assert!(filter.is_active(ProviderType::Command));
    assert!(filter.is_active(ProviderType::Plugin("power".into())));
    // Providers NOT in tabs are still active in auto mode.
    assert!(filter.is_active(ProviderType::Plugin("uuctl".into())));
    assert!(filter.is_active(ProviderType::Plugin("bookmarks".into())));
    assert!(filter.is_active(ProviderType::Plugin("anything-else".into())));
}

#[test]
fn auto_mode_filter_with_empty_tabs_still_accepts_everything() {
    // Even an empty `general.tabs` list (degenerate config) must not silently
    // drop providers from query routing — accept_all stays true.
    let filter = auto_mode_filter(&[]);
    assert!(filter.is_accept_all());
    assert!(filter.is_active(ProviderType::Application));
    assert!(filter.is_active(ProviderType::Plugin("uuctl".into())));
}

#[test]
fn auto_mode_changes_to_filtered_when_cli_mode_is_set() {
    // Counterpoint: passing `-m app` flips accept_all off so the user sees only
    // applications. Encodes the contract from the other direction.
    let cfg = ProvidersConfig::default();
    let tabs: Vec<String> = vec!["app".into(), "cmd".into()];
    let filter = ProviderFilter::new(Some(ProviderType::Application), None, &cfg, &tabs);

    assert!(!filter.is_accept_all(), "single-mode must clear accept_all");
    assert!(filter.is_active(ProviderType::Application));
    assert!(!filter.is_active(ProviderType::Command));
    assert!(!filter.is_active(ProviderType::Plugin("uuctl".into())));
}

#[test]
fn auto_mode_prefix_overrides_accept_all_for_routing() {
    // Inside the UI, typing `:uuctl foo` should narrow results to that
    // provider even when the filter is otherwise accept_all. The prefix is
    // a per-keystroke override; the filter's enabled set is unchanged.
    let mut filter = auto_mode_filter(&["app", "cmd"]);
    assert!(filter.is_accept_all());

    filter.set_prefix(Some(ProviderType::Plugin("uuctl".into())));
    assert!(filter.is_active(ProviderType::Plugin("uuctl".into())));
    assert!(!filter.is_active(ProviderType::Application));
    assert!(!filter.is_active(ProviderType::Command));

    // Clearing the prefix restores the accept_all reach.
    filter.set_prefix(None);
    assert!(filter.is_active(ProviderType::Application));
    assert!(filter.is_active(ProviderType::Plugin("uuctl".into())));
}

#[test]
fn dash_m_auto_explicit_alias_is_equivalent_to_no_flag() {
    // `owlry -m auto` is the documented explicit form of the no-flag default.
    // The parser turns `auto` into Plugin("auto"); the main launcher treats
    // any unknown-to-us mode like the default. Here we just confirm the
    // FromStr resolution stays under the user's control — implementation of
    // the actual treatment lives in the UI, but the parsing must not silently
    // map `auto` to something else.
    use std::str::FromStr;
    let parsed = ProviderType::from_str("auto").unwrap();
    assert_eq!(parsed, ProviderType::Plugin("auto".into()));
}
