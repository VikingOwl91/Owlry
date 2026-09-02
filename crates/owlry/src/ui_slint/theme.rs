//! Theme token adapter: real `AppearanceConfig`/`ThemeColors` from the
//! existing config path mapped onto Slint properties. No second config format.

use super::LauncherWindow;
use crate::config::{AppearanceConfig, ThemeColors};
use crate::providers::ProviderType;
use slint::Color;

fn hex_to_color(value: &str) -> Option<Color> {
    let hex = value.trim().trim_start_matches('#');
    match hex.len() {
        6 => u32::from_str_radix(hex, 16)
            .ok()
            .map(|rgb| Color::from_argb_encoded(0xff00_0000 | rgb)),
        8 => u32::from_str_radix(hex, 16)
            .ok()
            .map(Color::from_argb_encoded),
        _ => None,
    }
}

pub fn parse_color(value: &str) -> Option<Color> {
    hex_to_color(value)
}

/// Map a provider to its configured badge color key, mirroring the GTK
/// badge classes (`badge_app`, `badge_calc`, ...). `None` falls back to accent.
pub fn badge_color_for<'a>(provider: &ProviderType, colors: &'a ThemeColors) -> Option<&'a str> {
    match provider {
        ProviderType::Application => colors.badge_app.as_deref(),
        ProviderType::Command => colors.badge_cmd.as_deref(),
        ProviderType::Dmenu => colors.badge_dmenu.as_deref(),
        ProviderType::Plugin(id) => match id.as_str() {
            "calc" => colors.badge_calc.as_deref(),
            "uuctl" => colors.badge_uuctl.as_deref(),
            "power" | "sys" | "system" => colors.badge_power.as_deref(),
            "clipboard" => colors.badge_clip.as_deref(),
            "emoji" | "rofimoji" => colors.badge_emoji.as_deref(),
            "filesearch" => colors.badge_file.as_deref(),
            "ssh" => colors.badge_ssh.as_deref(),
            "websearch" => colors.badge_web.as_deref(),
            "bookmarks" => colors.badge_bookmark.as_deref(),
            "scripts" => colors.badge_script.as_deref(),
            _ => None,
        },
    }
}

/// Push the resolved theme tokens into the window. Unset keys keep the
/// markup defaults (a close match to the built-in owl palette).
pub fn apply(win: &LauncherWindow, appearance: &AppearanceConfig) {
    let colors = &appearance.colors;
    let set = |slot: &dyn Fn(Color), value: Option<&str>| {
        if let Some(v) = value.and_then(hex_to_color) {
            slot(v);
        }
    };
    set(&|c| win.set_window_bg(c), colors.background.as_deref());
    set(
        &|c| win.set_panel_bg(c),
        colors.background_secondary.as_deref(),
    );
    set(&|c| win.set_border_color(c), colors.border.as_deref());
    set(&|c| win.set_text_color(c), colors.text.as_deref());
    set(
        &|c| win.set_text_secondary(c),
        colors.text_secondary.as_deref(),
    );
    set(&|c| win.set_accent(c), colors.accent.as_deref());
    set(
        &|c| win.set_accent_bright(c),
        colors.accent_bright.as_deref(),
    );

    win.set_window_width(appearance.width as f32);
    win.set_window_height(appearance.height as f32);
    win.set_font_size(appearance.font_size as f32);
    win.set_corner_radius(appearance.border_radius as f32);
}
