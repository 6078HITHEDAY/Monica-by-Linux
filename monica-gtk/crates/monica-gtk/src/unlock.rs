use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{create_session, inspect_vault, secret_password, unlock_session, VaultInfo};

use crate::i18n::{t, tf};
use crate::dialogs::{choose_open, choose_save};
use crate::pages::Pages;
use crate::state::AppState;

pub fn build_unlock_page(state: AppState, pages: Pages) -> gtk::Widget {
    let style = libadwaita::StyleManager::default();
    let theme_label = theme_summary(&style);
    style.connect_dark_notify(glib::clone!(
        #[weak]
        theme_label,
        move |style_manager| {
            theme_label.set_label(&theme_summary_text(style_manager));
        }
    ));

    let path_row = libadwaita::EntryRow::builder().title(t("unlock.path")).build();
    path_row.set_text(&default_vault_path().to_string_lossy());

    let browse = gtk::Button::from_icon_name("document-open-symbolic");
    browse.set_valign(gtk::Align::Center);
    browse.set_tooltip_text(Some(t("unlock.browse").as_str()));
    path_row.add_suffix(&browse);
    let save_as = gtk::Button::from_icon_name("document-save-as-symbolic");
    save_as.set_valign(gtk::Align::Center);
    save_as.set_tooltip_text(Some(t("unlock.save_as").as_str()));
    path_row.add_suffix(&save_as);

    let password_row = libadwaita::PasswordEntryRow::builder()
        .title(t("unlock.password"))
        .build();

    let group = libadwaita::PreferencesGroup::builder()
        .title(t("unlock.group"))
        .description(t("unlock.group_desc"))
        .build();
    group.add(&path_row);
    group.add(&password_row);

    let unlock_button = gtk::Button::builder()
        .label(t("unlock.button"))
        .css_classes(["suggested-action", "pill"])
        .build();
    let create_button = gtk::Button::builder()
        .label(t("unlock.create"))
        .css_classes(["pill"])
        .build();
    let inspect_button = gtk::Button::builder()
        .label(t("unlock.inspect"))
        .css_classes(["pill", "flat"])
        .build();

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&unlock_button);
    buttons.append(&create_button);
    buttons.append(&inspect_button);

    let status = gtk::Label::builder()
        .label(t("unlock.idle"))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    status.set_selectable(true);
    *state.unlock_status.borrow_mut() = Some(status.clone());

    let form = gtk::Box::new(gtk::Orientation::Vertical, 18);
    form.set_valign(gtk::Align::Center);
    form.append(
        &gtk::Label::builder()
            .label(t("unlock.heading"))
            .css_classes(["title-1"])
            .build(),
    );
    form.append(
        &gtk::Label::builder()
            .label(t("unlock.intro"))
            .wrap(true)
            .xalign(0.0)
            .css_classes(["body"])
            .build(),
    );
    form.append(&theme_label);
    form.append(&group);
    form.append(&buttons);
    form.append(&status);

    let clamp = libadwaita::Clamp::builder()
        .maximum_size(460)
        .tightening_threshold(320)
        .child(&form)
        .build();

    let last_path: Rc<RefCell<PathBuf>> = Rc::new(RefCell::new(default_vault_path()));
    let actions = UnlockActions {
        unlock: unlock_button.clone(),
        create: create_button.clone(),
        inspect: inspect_button.clone(),
        browse: browse.clone(),
        save_as: save_as.clone(),
        password: password_row.clone(),
    };

    browse.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[weak]
        path_row,
        move |_| {
            state.touch();
            choose_open(
                &state,
                &t("unlock.choose_open"),
                &t("unlock.filter"),
                "*.mdbx",
                {
                    let state = state.clone();
                    move |path| {
                        path_row.set_text(&path.to_string_lossy());
                        *state.vault_path.borrow_mut() = path;
                    }
                },
            );
        }
    ));
    save_as.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[weak]
        path_row,
        move |_| {
            state.touch();
            choose_save(
                &state,
                &t("unlock.choose_create"),
                &t("unlock.filter"),
                "*.mdbx",
                "local.mdbx",
                {
                    let state = state.clone();
                    move |path| {
                        path_row.set_text(&path.to_string_lossy());
                        *state.vault_path.borrow_mut() = path;
                    }
                },
            );
        }
    ));

    unlock_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        #[strong]
        actions,
        #[weak]
        path_row,
        #[weak]
        status,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            *state.vault_path.borrow_mut() = path.clone();
            let typed = actions.password.text().to_string();
            if typed.is_empty() {
                state.show_error(Some(&status), &t("unlock.need_password"));
                return;
            }
            let password = secret_password(typed);
            actions.password.set_text("");
            let unlock_state = state.clone();
            let unlock_pages = pages.clone();
            let unlock_status = status.clone();
            state.spawn_job(
                Some(status.clone()),
                {
                    let actions = actions.clone();
                    move |busy| actions.set_busy(busy)
                },
                &t("unlock.unlocking"),
                move || unlock_session(&path, &password),
                move |session| {
                    show_session_opened(
                        &unlock_state,
                        &unlock_pages,
                        &unlock_status,
                        session,
                        &t("unlock.unlocked"),
                    );
                },
            );
        }
    ));

    inspect_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        actions,
        #[weak]
        path_row,
        #[weak]
        status,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            *state.vault_path.borrow_mut() = path.clone();
            state.spawn_job(
                Some(status.clone()),
                {
                    let actions = actions.clone();
                    move |busy| actions.set_busy(busy)
                },
                &t("unlock.inspecting"),
                move || inspect_vault(&path),
                {
                    let toast = state.toast.clone();
                    move |info: VaultInfo| {
                    let session = if info.unlocked {
                        t("unlock.unlocked")
                    } else {
                        t("unlock.locked_readonly")
                    };
                    let upgrade = if info.requires_upgrade {
                        t("unlock.inspect_upgrade")
                    } else {
                        String::new()
                    };
                    status.set_label(&tf(
                        "unlock.inspect_status",
                        &[
                            &info.path.display().to_string(),
                            &info.vault_id,
                            &info.format_version,
                            &info.schema_version.to_string(),
                            &info.tiga_mode,
                            &session,
                            &upgrade,
                        ],
                    ));
                    toast.add_toast(libadwaita::Toast::new(&t("unlock.inspect_ok")));
                    }
                },
            );
        }
    ));

    create_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        #[strong]
        last_path,
        #[strong]
        actions,
        #[weak]
        path_row,
        #[weak]
        status,
        move |_| {
            let mut path = PathBuf::from(path_row.text().as_str());
            if path.as_os_str().is_empty() {
                path = last_path.borrow().clone();
                path_row.set_text(&path.to_string_lossy());
            }
            let typed = actions.password.text().to_string();
            if typed.is_empty() {
                state.show_error(Some(&status), &t("unlock.need_password_create"));
                return;
            }
            *state.vault_path.borrow_mut() = path.clone();
            let password = secret_password(typed);
            actions.password.set_text("");
            let remembered = last_path.clone();
            let created_path = path.clone();
            let create_state = state.clone();
            let create_pages = pages.clone();
            let create_status = status.clone();
            state.spawn_job(
                Some(status.clone()),
                {
                    let actions = actions.clone();
                    move |busy| actions.set_busy(busy)
                },
                &t("unlock.creating"),
                move || create_session(&path, &password),
                move |session| {
                    *remembered.borrow_mut() = created_path;
                    show_session_opened(
                        &create_state,
                        &create_pages,
                        &create_status,
                        session,
                        &t("unlock.created"),
                    );
                },
            );
        }
    ));

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build();
    scrolled.upcast()
}

fn show_session_opened(
    state: &AppState,
    pages: &crate::pages::Pages,
    status: &gtk::Label,
    session: monica_vault::VaultSession,
    heading: &str,
) {
    let info = session.info().clone();
    status.set_label(&tf(
        "unlock.session_status",
        &[
            heading,
            &info.path.display().to_string(),
            &info.vault_id,
            &info.format_version,
            &info.schema_version.to_string(),
            &info.tiga_mode,
        ],
    ));
    state.replace_session(Some(session));
    state.toast.add_toast(libadwaita::Toast::new(heading));
    pages.on_session_changed(state);
    if let Some(row) = state.nav_list.row_at_index(1) {
        state.nav_list.select_row(Some(&row));
    }
    state.stack.set_visible_child_name("passwords");
    state.content_page.set_title(&t("nav.passwords"));
}

#[derive(Clone)]
struct UnlockActions {
    unlock: gtk::Button,
    create: gtk::Button,
    inspect: gtk::Button,
    browse: gtk::Button,
    save_as: gtk::Button,
    password: libadwaita::PasswordEntryRow,
}

impl UnlockActions {
    fn set_busy(&self, busy: bool) {
        let sensitive = !busy;
        self.unlock.set_sensitive(sensitive);
        self.create.set_sensitive(sensitive);
        self.inspect.set_sensitive(sensitive);
        self.browse.set_sensitive(sensitive);
        self.save_as.set_sensitive(sensitive);
    }
}

fn theme_summary(style: &libadwaita::StyleManager) -> gtk::Label {
    gtk::Label::builder()
        .label(theme_summary_text(style))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["caption"])
        .build()
}

fn theme_summary_text(style: &libadwaita::StyleManager) -> String {
    let appearance = if style.is_dark() {
        t("unlock.theme_dark")
    } else {
        t("unlock.theme_light")
    };
    tf("unlock.theme", &[&appearance])
}

pub fn default_vault_path() -> PathBuf {
    std::env::temp_dir()
        .join("monica-gtk-phase0")
        .join("local.mdbx")
}
