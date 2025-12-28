use crate::providers::LaunchItem;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Image, Label, ListBoxRow, Orientation};

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

        // Icon
        let icon = if let Some(icon_name) = &item.icon {
            Image::from_icon_name(icon_name)
        } else {
            // Default icon based on provider type
            let default_icon = match item.provider {
                crate::providers::ProviderType::Application => "application-x-executable",
                crate::providers::ProviderType::Calculator => "accessories-calculator",
                crate::providers::ProviderType::Command => "utilities-terminal",
                crate::providers::ProviderType::Dmenu => "view-list-symbolic",
                crate::providers::ProviderType::Uuctl => "system-run",
            };
            Image::from_icon_name(default_icon)
        };

        icon.set_pixel_size(32);
        icon.add_css_class("owlry-result-icon");

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

        // Provider badge
        let badge = Label::builder()
            .label(&item.provider.to_string())
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Center)
            .build();

        badge.add_css_class("owlry-result-badge");
        badge.add_css_class(&format!("owlry-badge-{}", item.provider));

        hbox.append(&icon);
        hbox.append(&text_box);
        hbox.append(&badge);

        row.set_child(Some(&hbox));

        row
    }
}
