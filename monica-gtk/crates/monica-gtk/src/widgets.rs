use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::state::AppState;

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
        .buttons(["取消", ok_label])
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
        .label("保存")
        .css_classes(["suggested-action", "pill"])
        .build();
    let cancel = gtk::Button::builder()
        .label("取消")
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

    let empty = libadwaita::StatusPage::builder()
        .icon_name(empty_icon)
        .title(empty_title)
        .build();
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
    new_button.set_tooltip_text(Some("新建"));
    new_button.add_css_class("flat");
    let list_header = libadwaita::HeaderBar::new();
    list_header.pack_end(&new_button);
    let list_toolbar = libadwaita::ToolbarView::new();
    list_toolbar.add_top_bar(&list_header);
    list_toolbar.set_content(Some(&list_stack));
    let list_page = libadwaita::NavigationPage::builder()
        .title(list_title)
        .child(&list_toolbar)
        .build();

    let detail_empty = libadwaita::StatusPage::builder()
        .icon_name("view-reveal-symbolic")
        .title("选择一条目")
        .build();
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
    detail_toolbar.add_top_bar(&libadwaita::HeaderBar::new());
    detail_toolbar.set_content(Some(&detail_stack));
    let detail_page = libadwaita::NavigationPage::builder()
        .title("详情")
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

pub fn locked_empty(empty: &libadwaita::StatusPage, list_stack: &gtk::Stack, detail_stack: &gtk::Stack) {
    empty.set_title("请先解锁保险库");
    empty.set_description(Some("在「解锁」页打开保险库后可用。"));
    list_stack.set_visible_child_name("empty");
    detail_stack.set_visible_child_name("empty");
}
