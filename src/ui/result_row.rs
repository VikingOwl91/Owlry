use crate::providers::LaunchItem;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Image, Label, ListBoxRow, Orientation, Widget};

#[allow(dead_code)]
pub struct ResultRow {
    row: ListBoxRow,
}

impl ResultRow {
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

        // Icon - handle file paths, icon names, emoji widgets, and fallbacks
        let icon_widget: Widget = if let Some(icon_name) = &item.icon {
            let img = if icon_name.starts_with('/') {
                // Absolute file path
                Image::from_file(icon_name)
            } else {
                Image::from_icon_name(icon_name)
            };
            img.set_pixel_size(32);
            img.add_css_class("owlry-result-icon");
            img.upcast()
        } else {
            // Default icon based on provider type
            // For widgets, use emoji labels (more reliable than icon themes)
            match item.provider {
                crate::providers::ProviderType::Weather => {
                    Self::create_emoji_icon("🌤️")
                }
                crate::providers::ProviderType::MediaPlayer => {
                    Self::create_emoji_icon("🎵")
                }
                crate::providers::ProviderType::Pomodoro => {
                    Self::create_emoji_icon("🍅")
                }
                _ => {
                    let default_icon = match item.provider {
                        crate::providers::ProviderType::Application => "application-x-executable",
                        crate::providers::ProviderType::Bookmarks => "user-bookmarks",
                        crate::providers::ProviderType::Calculator => "accessories-calculator",
                        crate::providers::ProviderType::Clipboard => "edit-paste",
                        crate::providers::ProviderType::Command => "utilities-terminal",
                        crate::providers::ProviderType::Dmenu => "view-list-symbolic",
                        crate::providers::ProviderType::Emoji => "face-smile",
                        crate::providers::ProviderType::Files => "folder",
                        crate::providers::ProviderType::Scripts => "application-x-executable",
                        crate::providers::ProviderType::Ssh => "network-server",
                        crate::providers::ProviderType::System => "system-shutdown",
                        crate::providers::ProviderType::Uuctl => "system-run",
                        crate::providers::ProviderType::WebSearch => "web-browser",
                        _ => "application-x-executable",
                    };
                    let img = Image::from_icon_name(default_icon);
                    img.set_pixel_size(32);
                    img.add_css_class("owlry-result-icon");
                    img.upcast()
                }
            }
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
            .label(&item.provider.to_string())
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

    /// Create an emoji-based icon widget for widgets (weather, media, pomodoro)
    /// More reliable than icon themes which may not have all icons
    fn create_emoji_icon(emoji: &str) -> Widget {
        let label = Label::builder()
            .label(emoji)
            .width_request(32)
            .height_request(32)
            .build();
        label.add_css_class("owlry-emoji-icon");
        label.upcast()
    }
}
