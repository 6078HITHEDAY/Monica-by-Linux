use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::i18n::t;
use crate::state::AppState;

/// Sidebar HeaderBar of a NavigationSplitView: hide end window controls so
/// only the content HeaderBar shows min/max/close.
pub fn sidebar_header() -> libadwaita::HeaderBar {
    let bar = libadwaita::HeaderBar::new();
    bar.set_show_end_title_buttons(false);
    bar
}

/// Nested page HeaderBar: no window controls (the shell chrome owns them).
pub fn nested_header() -> libadwaita::HeaderBar {
    let bar = libadwaita::HeaderBar::new();
    bar.set_show_start_title_buttons(false);
    bar.set_show_end_title_buttons(false);
    bar
}

/// Sidebar (16px): prefer a card glyph when the theme has one.
pub fn wallet_icon() -> String {
    available_icon(
        &[
            "payment-card-symbolic",
            "credit-card-symbolic",
            "auth-smartcard-symbolic",
        ],
        "auth-smartcard-symbolic",
    )
}

pub fn available_icon(names: &[&str], fallback: &'static str) -> String {
    let Some(display) = gtk::gdk::Display::default() else {
        return fallback.to_string();
    };
    let theme = gtk::IconTheme::for_display(&display);
    names
        .iter()
        .copied()
        .find(|name| theme.has_icon(name))
        .unwrap_or(fallback)
        .to_string()
}

/// StatusPage paints ~128px. WhiteSur card/mime 16px symbols become the missing-image glyph.
pub fn apply_status_page_icon(page: &libadwaita::StatusPage, name: &str) {
    page.set_icon_name(Some(status_page_icon_name(name)));
}

fn status_page_icon_name(name: &str) -> &'static str {
    match name {
        "dialog-password-symbolic" => "dialog-password-symbolic",
        "folder-symbolic" => "folder-symbolic",
        "drive-harddisk-symbolic" => "drive-harddisk-symbolic",
        "view-reveal-symbolic" => "view-reveal-symbolic",
        "text-x-generic-symbolic" => "text-x-generic-symbolic",
        "user-trash-symbolic" => "user-trash-symbolic",
        "document-open-recent-symbolic" => "document-open-recent-symbolic",
        "auth-smartcard-symbolic" | "payment-card-symbolic" | "credit-card-symbolic" => {
            "text-x-generic-symbolic"
        }
        "channel-secure-symbolic" | "security-high-symbolic" => "dialog-password-symbolic",
        _ => "folder-symbolic",
    }
}

pub fn field(title: &str, child: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.append(
        &gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build(),
    );
    box_.append(child);
    box_.upcast()
}

pub fn value_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["body"])
        .build()
}

pub fn hidden_secret() -> String {
    "••••••••".to_string()
}

pub fn short_time(iso: &str) -> String {
    iso.replace('T', " ").chars().take(16).collect()
}

pub fn dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "—"
    } else {
        value
    }
}

pub fn confirm_action(
    state: &AppState,
    message: &str,
    detail: &str,
    ok_label: &str,
    on_yes: impl FnOnce() + 'static,
) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .buttons([t("common.cancel"), ok_label.to_string()])
        .cancel_button(0)
        .default_button(0)
        .build();
    let window = state.window.clone();
    dialog.choose(
        Some(&window),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            if result == Ok(1) {
                on_yes();
            }
        },
    );
}

pub fn present_editor(
    state: &AppState,
    title: &str,
    form: &impl IsA<gtk::Widget>,
) -> libadwaita::Window {
    let editor = libadwaita::Window::builder()
        .transient_for(&state.window)
        .modal(true)
        .title(title)
        .default_width(440)
        .default_height(560)
        .build();
    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());
    toolbar.set_content(Some(form));
    editor.set_content(Some(&toolbar));
    editor
}

pub fn editor_buttons() -> (gtk::Box, gtk::Button, gtk::Button) {
    let save = gtk::Button::builder()
        .label(t("common.save"))
        .css_classes(["suggested-action", "pill"])
        .build();
    let cancel = gtk::Button::builder()
        .label(t("common.cancel"))
        .css_classes(["pill"])
        .build();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.append(&cancel);
    buttons.append(&save);
    (buttons, save, cancel)
}

#[derive(Clone)]
pub struct SplitWorkspace {
    pub root: gtk::Widget,
    pub list: gtk::ListBox,
    pub empty: libadwaita::StatusPage,
    pub list_stack: gtk::Stack,
    pub detail_stack: gtk::Stack,
    pub new_button: gtk::Button,
    pub split: libadwaita::NavigationSplitView,
    pub detail_box: gtk::Box,
}

pub fn build_split(list_title: &str, empty_icon: &str, empty_title: &str) -> SplitWorkspace {
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_vexpand(true);

    let empty = libadwaita::StatusPage::builder().title(empty_title).build();
    apply_status_page_icon(&empty, empty_icon);
    let list_stack = gtk::Stack::new();
    list_stack.add_named(&empty, Some("empty"));
    list_stack.add_named(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build(),
        Some("list"),
    );
    list_stack.set_visible_child_name("empty");

    let new_button = gtk::Button::from_icon_name("list-add-symbolic");
    new_button.set_tooltip_text(Some(t("common.new").as_str()));
    new_button.add_css_class("flat");
    let list_header = nested_header();
    list_header.pack_end(&new_button);
    let list_toolbar = libadwaita::ToolbarView::new();
    list_toolbar.add_top_bar(&list_header);
    list_toolbar.set_content(Some(&list_stack));
    let list_page = libadwaita::NavigationPage::builder()
        .title(list_title)
        .child(&list_toolbar)
        .build();

    let detail_empty = libadwaita::StatusPage::builder()
        .title(t("common.select_item_short"))
        .description(t("common.select_item_hint"))
        .build();
    apply_status_page_icon(&detail_empty, "view-reveal-symbolic");
    let detail_box = gtk::Box::new(gtk::Orientation::Vertical, 14);
    detail_box.set_margin_start(18);
    detail_box.set_margin_end(18);
    detail_box.set_margin_top(18);
    detail_box.set_margin_bottom(18);

    let detail_stack = gtk::Stack::new();
    detail_stack.add_named(&detail_empty, Some("empty"));
    detail_stack.add_named(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&detail_box)
            .build(),
        Some("detail"),
    );
    detail_stack.set_visible_child_name("empty");
    let detail_toolbar = libadwaita::ToolbarView::new();
    detail_toolbar.add_top_bar(&nested_header());
    detail_toolbar.set_content(Some(&detail_stack));
    let detail_page = libadwaita::NavigationPage::builder()
        .title(t("common.detail"))
        .child(&detail_toolbar)
        .build();

    let split = libadwaita::NavigationSplitView::new();
    split.set_min_sidebar_width(240.0);
    split.set_sidebar_width_fraction(0.36);
    split.set_sidebar(Some(&list_page));
    split.set_content(Some(&detail_page));

    SplitWorkspace {
        root: split.clone().upcast(),
        list,
        empty,
        list_stack,
        detail_stack,
        new_button,
        split,
        detail_box,
    }
}

pub fn fill_list(
    list: &gtk::ListBox,
    ids: &std::cell::RefCell<Vec<String>>,
    rows: impl IntoIterator<Item = (String, String, String)>,
) -> usize {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    ids.borrow_mut().clear();
    let mut count = 0;
    for (id, title, subtitle) in rows {
        let row = libadwaita::ActionRow::builder()
            .title(&title)
            .subtitle(&subtitle)
            .activatable(true)
            .build();
        list.append(&row);
        ids.borrow_mut().push(id);
        count += 1;
    }
    count
}

pub fn locked_empty(
    empty: &libadwaita::StatusPage,
    list_stack: &gtk::Stack,
    detail_stack: &gtk::Stack,
) {
    empty.set_title(&t("common.unlock_first"));
    empty.set_description(Some(t("common.unlock_page_hint").as_str()));
    list_stack.set_visible_child_name("empty");
    detail_stack.set_visible_child_name("empty");
}

#[cfg(test)]
mod tests {
    use super::status_page_icon_name;

    #[test]
    fn card_icons_use_status_page_safe_standins() {
        assert_eq!(
            status_page_icon_name("payment-card-symbolic"),
            "text-x-generic-symbolic"
        );
        assert_eq!(
            status_page_icon_name("auth-smartcard-symbolic"),
            "text-x-generic-symbolic"
        );
        assert_eq!(
            status_page_icon_name("dialog-password-symbolic"),
            "dialog-password-symbolic"
        );
        assert_eq!(
            status_page_icon_name("view-reveal-symbolic"),
            "view-reveal-symbolic"
        );
    }
}
