//! Fullscreen vault access gate: first-run onboarding or daily unlock.
//!
//! Covers the main shell while locked. Create requires the master password
//! twice. Inspect-only stays on Settings.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{create_session, secret_password, unlock_session};

use crate::dialogs::{choose_open, choose_save};
use crate::i18n::{t, tf};
use crate::pages::Pages;
use crate::state::AppState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateMode {
    /// First run / missing file: path + password + confirm.
    Create,
    /// Existing library: path + password once.
    Unlock,
}

#[derive(Clone)]
pub struct UnlockGate {
    pub root: gtk::Widget,
    page: libadwaita::StatusPage,
    path_row: libadwaita::EntryRow,
    password_row: libadwaita::PasswordEntryRow,
    confirm_row: libadwaita::PasswordEntryRow,
    primary: gtk::Button,
    secondary: gtk::Button,
    status: gtk::Label,
    create_mode: Rc<Cell<bool>>,
    actions: GateActions,
}

#[derive(Clone)]
struct GateActions {
    primary: gtk::Button,
    secondary: gtk::Button,
    browse: gtk::Button,
}

impl GateActions {
    fn set_busy(&self, busy: bool) {
        let sensitive = !busy;
        self.primary.set_sensitive(sensitive);
        self.secondary.set_sensitive(sensitive);
        self.browse.set_sensitive(sensitive);
    }
}

pub fn build_unlock_gate(state: AppState, pages: Pages) -> UnlockGate {
    let path_row = libadwaita::EntryRow::builder()
        .title(t("unlock.path"))
        .build();
    path_row.set_text(&state.vault_path.borrow().to_string_lossy());

    let browse = gtk::Button::from_icon_name("document-open-symbolic");
    browse.set_valign(gtk::Align::Center);
    browse.set_tooltip_text(Some(t("unlock.browse").as_str()));
    path_row.add_suffix(&browse);

    let password_row = libadwaita::PasswordEntryRow::builder()
        .title(t("unlock.password"))
        .build();
    let confirm_row = libadwaita::PasswordEntryRow::builder()
        .title(t("unlock.confirm_password"))
        .build();

    let group = libadwaita::PreferencesGroup::builder()
        .title(t("unlock.group"))
        .build();
    group.add(&path_row);
    group.add(&password_row);
    group.add(&confirm_row);

    let primary = gtk::Button::builder()
        .label(t("unlock.button"))
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::Center)
        .hexpand(true)
        .build();
    let secondary = gtk::Button::builder()
        .label(t("unlock.create_new"))
        .css_classes(["pill", "flat"])
        .halign(gtk::Align::Center)
        .build();

    let buttons = gtk::Box::new(gtk::Orientation::Vertical, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&primary);
    buttons.append(&secondary);

    let status = gtk::Label::builder()
        .label(t("unlock.idle"))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    status.set_selectable(true);
    *state.unlock_status.borrow_mut() = Some(status.clone());

    let form = gtk::Box::new(gtk::Orientation::Vertical, 18);
    form.append(&group);
    form.append(&buttons);
    form.append(&status);

    let clamp = libadwaita::Clamp::builder()
        .maximum_size(460)
        .tightening_threshold(320)
        .child(&form)
        .build();

    let page = libadwaita::StatusPage::builder()
        .icon_name("system-lock-screen-symbolic")
        .title(t("unlock.heading"))
        .description(t("unlock.intro"))
        .child(&clamp)
        .build();

    let header = libadwaita::HeaderBar::new();
    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));

    let actions = GateActions {
        primary: primary.clone(),
        secondary: secondary.clone(),
        browse: browse.clone(),
    };
    let gate = UnlockGate {
        root: toolbar.upcast(),
        page,
        path_row: path_row.clone(),
        password_row: password_row.clone(),
        confirm_row: confirm_row.clone(),
        primary: primary.clone(),
        secondary: secondary.clone(),
        status: status.clone(),
        create_mode: Rc::new(Cell::new(false)),
        actions,
    };
    gate.sync_from_state(&state);

    browse.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        gate,
        move |_| {
            state.touch();
            if gate.create_mode.get() {
                choose_save(
                    &state,
                    &t("unlock.choose_create"),
                    &t("unlock.filter"),
                    "*.mdbx",
                    "local.mdbx",
                    {
                        let state = state.clone();
                        let gate = gate.clone();
                        move |path| {
                            state.remember_path(path.clone());
                            gate.path_row.set_text(&path.to_string_lossy());
                            gate.apply_mode(GateMode::Create);
                        }
                    },
                );
            } else {
                choose_open(
                    &state,
                    &t("unlock.choose_open"),
                    &t("unlock.filter"),
                    "*.mdbx",
                    {
                        let state = state.clone();
                        let gate = gate.clone();
                        move |path| {
                            state.remember_path(path.clone());
                            gate.path_row.set_text(&path.to_string_lossy());
                            gate.apply_mode(if library_exists(&path) {
                                GateMode::Unlock
                            } else {
                                GateMode::Create
                            });
                        }
                    },
                );
            }
        }
    ));

    secondary.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        gate,
        move |_| {
            state.touch();
            if gate.create_mode.get() {
                choose_open(
                    &state,
                    &t("unlock.choose_open"),
                    &t("unlock.filter"),
                    "*.mdbx",
                    {
                        let state = state.clone();
                        let gate = gate.clone();
                        move |path| {
                            state.remember_path(path.clone());
                            gate.path_row.set_text(&path.to_string_lossy());
                            gate.apply_mode(GateMode::Unlock);
                        }
                    },
                );
            } else {
                choose_save(
                    &state,
                    &t("unlock.choose_create"),
                    &t("unlock.filter"),
                    "*.mdbx",
                    "local.mdbx",
                    {
                        let state = state.clone();
                        let gate = gate.clone();
                        move |path| {
                            state.remember_path(path.clone());
                            gate.path_row.set_text(&path.to_string_lossy());
                            gate.apply_mode(GateMode::Create);
                        }
                    },
                );
            }
        }
    ));

    primary.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        #[strong]
        gate,
        move |_| submit_gate(&state, &pages, &gate)
    ));
    password_row.connect_entry_activated(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        #[strong]
        gate,
        move |_| submit_gate(&state, &pages, &gate)
    ));
    confirm_row.connect_entry_activated(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        #[strong]
        gate,
        move |_| submit_gate(&state, &pages, &gate)
    ));

    gate
}

impl UnlockGate {
    pub fn sync_from_state(&self, state: &AppState) {
        let path = state.vault_path.borrow().clone();
        self.path_row.set_text(&path.to_string_lossy());
        self.apply_mode(if library_exists(&path) {
            GateMode::Unlock
        } else {
            GateMode::Create
        });
    }

    pub fn apply_mode(&self, mode: GateMode) {
        let create = mode == GateMode::Create;
        self.create_mode.set(create);
        self.confirm_row.set_visible(create);
        if create {
            self.page.set_icon_name(Some("dialog-password-symbolic"));
            self.page.set_title(&t("unlock.onboard_heading"));
            self.page
                .set_description(Some(t("unlock.onboard_intro").as_str()));
            self.primary.set_label(&t("unlock.create_enter"));
            self.secondary.set_label(&t("unlock.open_existing"));
            self.actions
                .browse
                .set_tooltip_text(Some(t("unlock.save_as").as_str()));
            self.actions
                .browse
                .set_icon_name("document-save-as-symbolic");
        } else {
            self.page.set_icon_name(Some("system-lock-screen-symbolic"));
            self.page.set_title(&t("unlock.heading"));
            self.page.set_description(Some(t("unlock.intro").as_str()));
            self.primary.set_label(&t("unlock.button"));
            self.secondary.set_label(&t("unlock.create_new"));
            self.actions
                .browse
                .set_tooltip_text(Some(t("unlock.browse").as_str()));
            self.actions.browse.set_icon_name("document-open-symbolic");
        }
    }
}

fn submit_gate(state: &AppState, pages: &Pages, gate: &UnlockGate) {
    state.touch();
    let path = PathBuf::from(gate.path_row.text().as_str());
    if path.as_os_str().is_empty() {
        state.remember_path(default_vault_path());
        gate.path_row
            .set_text(&state.vault_path.borrow().to_string_lossy());
    } else {
        state.remember_path(path.clone());
    }
    let path = state.vault_path.borrow().clone();
    let typed = gate.password_row.text().to_string();
    if typed.is_empty() {
        let message = if gate.create_mode.get() {
            t("unlock.need_password_create")
        } else {
            t("unlock.need_password")
        };
        state.show_error(Some(&gate.status), &message);
        return;
    }
    if gate.create_mode.get() {
        if let Err(key) = validate_create_passwords(&typed, &gate.confirm_row.text()) {
            let message = match key {
                "unlock.need_password_create" => t("unlock.need_password_create"),
                "unlock.need_confirm" => t("unlock.need_confirm"),
                _ => t("unlock.password_mismatch"),
            };
            state.show_error(Some(&gate.status), &message);
            return;
        }
    }
    let password = secret_password(typed);
    gate.password_row.set_text("");
    gate.confirm_row.set_text("");
    let create = gate.create_mode.get();
    let opened_state = state.clone();
    let opened_pages = pages.clone();
    let opened_status = gate.status.clone();
    let actions = gate.actions.clone();
    let busy_text = if create {
        t("unlock.creating")
    } else {
        t("unlock.unlocking")
    };
    let heading = if create {
        t("unlock.created")
    } else {
        t("unlock.unlocked")
    };
    state.spawn_job(
        Some(gate.status.clone()),
        move |busy| actions.set_busy(busy),
        &busy_text,
        move || {
            if create {
                create_session(&path, &password)
            } else {
                unlock_session(&path, &password)
            }
        },
        move |session| {
            show_session_opened(
                &opened_state,
                &opened_pages,
                &opened_status,
                session,
                &heading,
            );
        },
    );
}

fn show_session_opened(
    state: &AppState,
    pages: &Pages,
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
    state.remember_path(info.path.clone());
    state.replace_session(Some(session));
    state.toast.add_toast(libadwaita::Toast::new(heading));
    pages.on_session_changed(state);
    state.show_shell();
    state.select_nav("passwords");
}

pub fn library_exists(path: &Path) -> bool {
    path.is_file()
}

fn validate_create_passwords(password: &str, confirm: &str) -> Result<(), &'static str> {
    if password.is_empty() {
        return Err("unlock.need_password_create");
    }
    if confirm.is_empty() {
        return Err("unlock.need_confirm");
    }
    if password != confirm {
        return Err("unlock.password_mismatch");
    }
    Ok(())
}

pub fn default_vault_path() -> PathBuf {
    data_home().join("monica-gtk").join("local.mdbx")
}

pub fn initial_vault_path() -> PathBuf {
    crate::prefs::current()
        .vault_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_vault_path)
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })
        .unwrap_or_else(|| PathBuf::from(".local").join("share"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_vault_path_uses_xdg_data_not_tmp_phase0() {
        let path = default_vault_path();
        let text = path.to_string_lossy();
        assert!(text.ends_with("monica-gtk/local.mdbx"));
        assert!(!text.contains("monica-gtk-phase0"));
        assert!(!text.starts_with("/tmp/monica-gtk"));
    }

    #[test]
    fn missing_file_is_not_an_existing_library() {
        assert!(!library_exists(Path::new(
            "/no/such/monica-gtk-missing.mdbx"
        )));
    }

    #[test]
    fn create_requires_matching_password_twice() {
        assert_eq!(
            validate_create_passwords("", "x"),
            Err("unlock.need_password_create")
        );
        assert_eq!(
            validate_create_passwords("secret", ""),
            Err("unlock.need_confirm")
        );
        assert_eq!(
            validate_create_passwords("secret", "other"),
            Err("unlock.password_mismatch")
        );
        assert_eq!(validate_create_passwords("secret", "secret"), Ok(()));
    }
}
