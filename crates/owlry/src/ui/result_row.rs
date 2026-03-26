use owlry_core::providers::LaunchItem;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Image, Label, ListBoxRow, Orientation, Widget};

#[allow(dead_code)]
pub struct ResultRow {
    row: ListBoxRow,
}

/// Check if a string looks like an emoji (starts with a non-ASCII character
/// and is very short - typically 1-4 chars for complex emojis with ZWJ)
fn is_emoji_icon(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Emojis are non-ASCII and typically very short (1-8 chars for complex ZWJ sequences)
    let first_char = s.chars().next().unwrap();
    !first_char.is_ascii() && s.chars().count() <= 8
}

impl ResultRow {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(item: &LaunchItem) -> ListBoxRow {
        let row = ListBoxRow::builder()
            .selectable(true)
            .activatable(true)
            .build();

        row.add_css_class("owlry-result-row");

        let hbox = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Icon - handle GResource paths, file paths, icon names, emojis, and fallbacks
        let icon_widget: Widget = if let Some(icon_path) = &item.icon {
            if is_emoji_icon(icon_path) {
                // Emoji character - display as text label
                let emoji_label = Label::builder()
                    .label(icon_path)
                    .width_request(32)
                    .height_request(32)
                    .valign(gtk4::Align::Center)
                    .halign(gtk4::Align::Center)
                    .build();
                emoji_label.add_css_class("owlry-result-icon");
                emoji_label.add_css_class("owlry-emoji-icon");
                emoji_label.upcast()
            } else if icon_path.starts_with("/org/owlry/launcher/icons/") {
                // GResource path - load from bundled resources
                let img = Image::from_resource(icon_path);
                img.set_pixel_size(32);
                img.add_css_class("owlry-result-icon");
                // SVG icons from resources should be treated as symbolic for color inheritance
                if icon_path.ends_with(".svg") {
                    img.add_css_class("owlry-symbolic-icon");
                }
                img.upcast()
            } else if icon_path.starts_with('/') {
                // Absolute file path
                let img = Image::from_file(icon_path);
                img.set_pixel_size(32);
                img.add_css_class("owlry-result-icon");
                img.upcast()
            } else {
                // Icon theme name
                let img = Image::from_icon_name(icon_path);
                img.set_pixel_size(32);
                img.add_css_class("owlry-result-icon");
                // Add symbolic class for icons ending with "-symbolic"
                if icon_path.ends_with("-symbolic") {
                    img.add_css_class("owlry-symbolic-icon");
                }
                img.upcast()
            }
        } else {
            // Default icon based on provider type (only core types, plugins should provide icons)
            let default_icon = match &item.provider {
                owlry_core::providers::ProviderType::Application => "application-x-executable-symbolic",
                owlry_core::providers::ProviderType::Command => "utilities-terminal-symbolic",
                owlry_core::providers::ProviderType::Dmenu => "view-list-symbolic",
                // Plugins should provide their own icon; fallback to generic addon icon
                owlry_core::providers::ProviderType::Plugin(_) => "application-x-addon-symbolic",
            };
            let img = Image::from_icon_name(default_icon);
            img.set_pixel_size(32);
            img.add_css_class("owlry-result-icon");
            img.add_css_class("owlry-symbolic-icon");
            img.upcast()
        };

        // Text container
        let text_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();

        // Name label
        let name_label = Label::builder()
            .label(&item.name)
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();

        name_label.add_css_class("owlry-result-name");

        // Description label
        if let Some(desc) = &item.description {
            let desc_label = Label::builder()
                .label(desc)
                .halign(gtk4::Align::Start)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .build();

            desc_label.add_css_class("owlry-result-description");
            text_box.append(&name_label);
            text_box.append(&desc_label);
        } else {
            text_box.append(&name_label);
        }

        // Tag badges (show first 3 tags)
        if !item.tags.is_empty() {
            let tags_box = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(4)
                .halign(gtk4::Align::Start)
                .build();

            for tag in item.tags.iter().take(3) {
                let tag_label = Label::builder()
                    .label(tag)
                    .build();
                tag_label.add_css_class("owlry-tag-badge");
                tags_box.append(&tag_label);
            }

            text_box.append(&tags_box);
        }

        // Provider badge
        let badge = Label::builder()
            .label(item.provider.to_string())
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Center)
            .build();

        badge.add_css_class("owlry-result-badge");
        badge.add_css_class(&format!("owlry-badge-{}", item.provider));

        hbox.append(&icon_widget);
        hbox.append(&text_box);
        hbox.append(&badge);

        row.set_child(Some(&hbox));

        row
    }
}
