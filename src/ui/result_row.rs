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

        // Icon - handle GResource paths, file paths, icon names, and fallbacks
        let icon_widget: Widget = if let Some(icon_path) = &item.icon {
            let img = if icon_path.starts_with("/org/owlry/launcher/icons/") {
                // GResource path - load from bundled resources
                Image::from_resource(icon_path)
            } else if icon_path.starts_with('/') {
                // Absolute file path
                Image::from_file(icon_path)
            } else {
                // Icon theme name
                Image::from_icon_name(icon_path)
            };
            img.set_pixel_size(32);
            img.add_css_class("owlry-result-icon");
            img.upcast()
        } else {
            // Default icon based on provider type
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
                // Widget providers now have icons set, but keep fallbacks
                crate::providers::ProviderType::Weather => "weather-clear-symbolic",
                crate::providers::ProviderType::MediaPlayer => "media-playback-start-symbolic",
                crate::providers::ProviderType::Pomodoro => "alarm-symbolic",
            };
            let img = Image::from_icon_name(default_icon);
            img.set_pixel_size(32);
            img.add_css_class("owlry-result-icon");
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
}
