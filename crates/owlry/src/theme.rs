use crate::config::AppearanceConfig;

/// Generate CSS with :root variables from config settings
pub fn generate_variables_css(config: &AppearanceConfig) -> String {
    let mut css = String::from(":root {\n");

    // Always inject layout config values
    css.push_str(&format!("    --owlry-font-size: {}px;\n", config.font_size));
    css.push_str(&format!(
        "    --owlry-border-radius: {}px;\n",
        config.border_radius
    ));

    // Only inject colors if user specified them
    if let Some(ref bg) = config.colors.background {
        css.push_str(&format!("    --owlry-bg: {};\n", bg));
    }
    if let Some(ref bg_secondary) = config.colors.background_secondary {
        css.push_str(&format!("    --owlry-bg-secondary: {};\n", bg_secondary));
    }
    if let Some(ref border) = config.colors.border {
        css.push_str(&format!("    --owlry-border: {};\n", border));
    }
    if let Some(ref text) = config.colors.text {
        css.push_str(&format!("    --owlry-text: {};\n", text));
    }
    if let Some(ref text_secondary) = config.colors.text_secondary {
        css.push_str(&format!(
            "    --owlry-text-secondary: {};\n",
            text_secondary
        ));
    }
    if let Some(ref accent) = config.colors.accent {
        css.push_str(&format!("    --owlry-accent: {};\n", accent));
    }
    if let Some(ref accent_bright) = config.colors.accent_bright {
        css.push_str(&format!("    --owlry-accent-bright: {};\n", accent_bright));
    }

    // Provider badge colors
    if let Some(ref badge_app) = config.colors.badge_app {
        css.push_str(&format!("    --owlry-badge-app: {};\n", badge_app));
    }
    if let Some(ref badge_bookmark) = config.colors.badge_bookmark {
        css.push_str(&format!(
            "    --owlry-badge-bookmark: {};\n",
            badge_bookmark
        ));
    }
    if let Some(ref badge_calc) = config.colors.badge_calc {
        css.push_str(&format!("    --owlry-badge-calc: {};\n", badge_calc));
    }
    if let Some(ref badge_clip) = config.colors.badge_clip {
        css.push_str(&format!("    --owlry-badge-clip: {};\n", badge_clip));
    }
    if let Some(ref badge_cmd) = config.colors.badge_cmd {
        css.push_str(&format!("    --owlry-badge-cmd: {};\n", badge_cmd));
    }
    if let Some(ref badge_dmenu) = config.colors.badge_dmenu {
        css.push_str(&format!("    --owlry-badge-dmenu: {};\n", badge_dmenu));
    }
    if let Some(ref badge_emoji) = config.colors.badge_emoji {
        css.push_str(&format!("    --owlry-badge-emoji: {};\n", badge_emoji));
    }
    if let Some(ref badge_file) = config.colors.badge_file {
        css.push_str(&format!("    --owlry-badge-file: {};\n", badge_file));
    }
    if let Some(ref badge_script) = config.colors.badge_script {
        css.push_str(&format!("    --owlry-badge-script: {};\n", badge_script));
    }
    if let Some(ref badge_ssh) = config.colors.badge_ssh {
        css.push_str(&format!("    --owlry-badge-ssh: {};\n", badge_ssh));
    }
    if let Some(ref badge_power) = config.colors.badge_power {
        // Emit both for transition: pre-v2 stylesheets reference --owlry-badge-sys.
        css.push_str(&format!("    --owlry-badge-power: {};\n", badge_power));
        css.push_str(&format!("    --owlry-badge-sys: {};\n", badge_power));
    }
    if let Some(ref badge_uuctl) = config.colors.badge_uuctl {
        css.push_str(&format!("    --owlry-badge-uuctl: {};\n", badge_uuctl));
    }
    if let Some(ref badge_web) = config.colors.badge_web {
        css.push_str(&format!("    --owlry-badge-web: {};\n", badge_web));
    }

    // Widget badge colors
    if let Some(ref badge_media) = config.colors.badge_media {
        css.push_str(&format!("    --owlry-badge-media: {};\n", badge_media));
    }
    if let Some(ref badge_weather) = config.colors.badge_weather {
        css.push_str(&format!("    --owlry-badge-weather: {};\n", badge_weather));
    }
    if let Some(ref badge_pomo) = config.colors.badge_pomo {
        css.push_str(&format!("    --owlry-badge-pomo: {};\n", badge_pomo));
    }

    css.push_str("}\n");
    css
}
