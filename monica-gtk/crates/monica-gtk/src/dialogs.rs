use std::path::PathBuf;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::state::AppState;

pub fn choose_save(
    state: &AppState,
    title: &str,
    filter_name: &str,
    pattern: &str,
    initial_name: &str,
    on_path: impl FnOnce(PathBuf) + 'static,
) {
    let dialog = gtk::FileDialog::builder()
        .title(title)
        .initial_name(initial_name)
        .build();
    let filter = gtk::FileFilter::new();
    filter.add_pattern(pattern);
    filter.set_name(Some(filter_name));
    dialog.set_default_filter(Some(&filter));
    if let Some(folder) = state.vault_path.borrow().parent() {
        dialog.set_initial_folder(Some(&gtk::gio::File::for_path(folder)));
    }
    dialog.save(
        Some(&state.window),
        None::<&gtk::gio::Cancellable>,
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
    let dialog = gtk::FileDialog::builder().title(title).build();
    let filter = gtk::FileFilter::new();
    filter.add_pattern(pattern);
    filter.set_name(Some(filter_name));
    dialog.set_default_filter(Some(&filter));
    if let Some(folder) = state.vault_path.borrow().parent() {
        dialog.set_initial_folder(Some(&gtk::gio::File::for_path(folder)));
    }
    dialog.open(
        Some(&state.window),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    on_path(path);
                }
            }
        },
    );
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
