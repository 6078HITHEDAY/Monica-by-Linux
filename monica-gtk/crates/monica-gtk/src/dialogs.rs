//! Portal-backed file dialogs (`GtkFileDialog`).
//!
//! GTK4 `FileDialog` uses xdg-desktop-portal on Wayland (and a native backend
//! on X11). Do not use `GtkFileChooserDialog` / `GtkFileChooserNative`.

use std::path::PathBuf;

use gtk4 as gtk;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use secrecy::SecretString;

use crate::i18n::t;
use crate::state::AppState;

pub fn choose_save(
    state: &AppState,
    title: &str,
    filter_name: &str,
    pattern: &str,
    initial_name: &str,
    on_path: impl FnOnce(PathBuf) + 'static,
) {
    let dialog = build_dialog(state, title, filter_name, pattern);
    dialog.set_initial_name(Some(initial_name));
    dialog.save(
        Some(&state.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    on_path(path);
                }
            }
        },
    );
}

pub fn choose_open(
    state: &AppState,
    title: &str,
    filter_name: &str,
    pattern: &str,
    on_path: impl FnOnce(PathBuf) + 'static,
) {
    let dialog = build_dialog(state, title, filter_name, pattern);
    dialog.open(
        Some(&state.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    on_path(path);
                }
            }
        },
    );
}

fn build_dialog(
    state: &AppState,
    title: &str,
    filter_name: &str,
    pattern: &str,
) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    let named = gtk::FileFilter::new();
    named.add_pattern(pattern);
    named.set_name(Some(filter_name));
    let all = gtk::FileFilter::new();
    all.add_pattern("*");
    all.set_name(Some(t("common.all_files").as_str()));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&named);
    filters.append(&all);
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&named));
    if let Some(folder) = state.vault_path.borrow().parent() {
        dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
    }
    dialog
}

/// Ask for a short-lived secret (KDBX file password). Empty input is rejected.
pub fn ask_secret(
    state: &AppState,
    title: &str,
    body: &str,
    on_ok: impl FnOnce(SecretString) + 'static,
) {
    let dialog = libadwaita::AlertDialog::new(Some(title), Some(body));
    dialog.add_response("cancel", &t("common.cancel"));
    dialog.add_response("ok", &t("common.ok"));
    dialog.set_response_appearance("ok", libadwaita::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("cancel");
    let entry = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .activates_default(true)
        .hexpand(true)
        .placeholder_text(t("dialog.file_password"))
        .build();
    dialog.set_extra_child(Some(&entry));
    let on_ok = std::cell::RefCell::new(Some(on_ok));
    let window = state.window.clone();
    let toast_state = state.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response != "ok" {
            return;
        }
        let typed = entry.text().to_string();
        entry.set_text("");
        if typed.is_empty() {
            toast_state.show_error(None, &t("dialog.file_password_empty"));
            return;
        }
        if let Some(callback) = on_ok.borrow_mut().take() {
            callback(SecretString::from(typed));
        }
    });
    dialog.present(Some(&window));
}

pub fn status_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .css_classes(["dim-label"])
        .build()
}

pub fn pill(label: &str, suggested: bool) -> gtk::Button {
    let mut classes = vec!["pill"];
    if suggested {
        classes.push("suggested-action");
    }
    gtk::Button::builder()
        .label(label)
        .css_classes(classes)
        .build()
}

pub fn button_row(buttons: &[&gtk::Button]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for button in buttons {
        row.append(*button);
    }
    row
}

pub fn note_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .wrap(true)
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .build()
}
