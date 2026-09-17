//! Adwaita **system symbolic** icons (theme names, not a custom pack).
//!
//! Names are from the Adwaita icon theme shipped on Ubuntu / GNOME so they
//! follow light/dark automatically.

use gtk4 as gtk;
use gtk4::prelude::*;

/// Icon-only header/toolbar button (`list-add-symbolic`, lock, browse, …).
pub fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    button
}

/// Labelled action button with a symbolic icon via [`libadwaita::ButtonContent`].
pub fn labeled_icon_button(
    icon_name: &str,
    label: &str,
    classes: &[&str],
) -> (gtk::Button, libadwaita::ButtonContent) {
    let content = libadwaita::ButtonContent::builder()
        .icon_name(icon_name)
        .label(label)
        .can_shrink(false)
        .build();
    let button = gtk::Button::builder().child(&content).build();
    for class in classes {
        button.add_css_class(class);
    }
    button.set_tooltip_text(Some(label));
    (button, content)
}

pub fn set_reveal_visual(content: &libadwaita::ButtonContent, revealed: bool) {
    if revealed {
        content.set_icon_name("view-conceal-symbolic");
        content.set_label("隐藏");
    } else {
        content.set_icon_name("view-reveal-symbolic");
        content.set_label("显示");
    }
}

/// Prefix image for [`libadwaita::ActionRow`] (replaces deprecated `icon-name`).
pub fn row_icon(icon_name: &str) -> gtk::Image {
    gtk::Image::from_icon_name(icon_name)
}
