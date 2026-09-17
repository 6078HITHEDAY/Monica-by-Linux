//! Persistent UI settings: auto-lock, clipboard-clear, notifications, tray.
//!
//! File: `$XDG_CONFIG_HOME/monica-gtk/settings.json` (fallback `~/.config`).
//! Environment variables still override the file when set.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use serde::{Deserialize, Serialize};

use crate::desktop::{probe, SHORTCUT_PREFERRED_TRIGGER};
use crate::dialogs::{choose_open, choose_save};
use crate::i18n::{t, tf};
use crate::security::{AUTO_LOCK_SECS, CLIPBOARD_CLEAR_SECS};
use crate::state::AppState;

static LIVE: OnceLock<Mutex<UiSettings>> = OnceLock::new();

fn default_true() -> bool {
    true
}

fn default_locale() -> String {
    "system".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub auto_lock_secs: u32,
    pub clipboard_clear_secs: u32,
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub vault_path: Option<String>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            auto_lock_secs: AUTO_LOCK_SECS,
            clipboard_clear_secs: CLIPBOARD_CLEAR_SECS,
            desktop_notifications: true,
            close_to_tray: true,
            locale: default_locale(),
            vault_path: None,
        }
    }
}

impl UiSettings {
    pub fn load() -> Self {
        let mut settings = Self::default();
        if let Ok(bytes) = fs::read(config_path()) {
            if let Ok(file) = serde_json::from_slice::<Self>(&bytes) {
                settings = file;
            }
        }
        settings.auto_lock_secs = settings.auto_lock_secs.clamp(3, 7200);
        settings.clipboard_clear_secs = settings.clipboard_clear_secs.clamp(1, 600);
        if let Some(value) = env_u32("MONICA_GTK_AUTO_LOCK_SECS") {
            settings.auto_lock_secs = value.clamp(3, 7200);
        }
        if let Some(value) = env_u32("MONICA_GTK_CLIPBOARD_CLEAR_SECS") {
            settings.clipboard_clear_secs = value.clamp(1, 600);
        }
        settings
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }
}

pub fn current() -> UiSettings {
    live().lock().map(|guard| guard.clone()).unwrap_or_default()
}

pub fn replace(next: UiSettings) {
    if let Ok(mut guard) = live().lock() {
        *guard = next;
        let _ = guard.save();
    }
}

fn live() -> &'static Mutex<UiSettings> {
    LIVE.get_or_init(|| Mutex::new(UiSettings::load()))
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("monica-gtk").join("settings.json")
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
}

fn env_is_set(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

#[derive(Clone)]
pub struct SettingsPage {
    pub root: gtk::Widget,
    file_row: libadwaita::ActionRow,
    notify_row: libadwaita::ActionRow,
    shortcut_row: libadwaita::ActionRow,
    tray_row: libadwaita::ActionRow,
    vault_path_row: libadwaita::ActionRow,
    vault_status: gtk::Label,
}

impl SettingsPage {
    pub fn build(state: &AppState) -> Self {
        let settings = current();
        let vault_path_row = libadwaita::ActionRow::builder()
            .title(t("settings.vault_path"))
            .subtitle(state.vault_path.borrow().display().to_string())
            .subtitle_selectable(true)
            .build();
        let open_other = gtk::Button::builder()
            .label(t("settings.vault_open"))
            .css_classes(["pill"])
            .build();
        let create_new = gtk::Button::builder()
            .label(t("settings.vault_create"))
            .css_classes(["pill"])
            .build();
        let inspect = gtk::Button::builder()
            .label(t("unlock.inspect"))
            .css_classes(["pill", "flat"])
            .build();
        let vault_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        vault_buttons.append(&open_other);
        vault_buttons.append(&create_new);
        vault_buttons.append(&inspect);
        let vault_status = gtk::Label::builder()
            .label("")
            .wrap(true)
            .xalign(0.0)
            .selectable(true)
            .css_classes(["caption", "dim-label"])
            .build();
        let vault_group = libadwaita::PreferencesGroup::builder()
            .title(t("settings.vault"))
            .description(t("settings.vault_desc"))
            .build();
        vault_group.add(&vault_path_row);

        let auto_lock = libadwaita::SpinRow::builder()
            .title(t("settings.autolock"))
            .subtitle(t("settings.autolock_sub"))
            .adjustment(&gtk::Adjustment::new(
                f64::from((settings.auto_lock_secs / 60).max(1)),
                1.0,
                120.0,
                1.0,
                5.0,
                0.0,
            ))
            .digits(0)
            .build();
        let clipboard = libadwaita::SpinRow::builder()
            .title(t("settings.clipboard"))
            .subtitle(t("settings.clipboard_sub"))
            .adjustment(&gtk::Adjustment::new(
                f64::from(settings.clipboard_clear_secs),
                1.0,
                600.0,
                1.0,
                10.0,
                0.0,
            ))
            .digits(0)
            .build();

        let group = libadwaita::PreferencesGroup::builder()
            .title(t("settings.security"))
            .description(t("settings.security_desc"))
            .build();
        group.add(&auto_lock);
        group.add(&clipboard);

        let notify_switch = libadwaita::SwitchRow::builder()
            .title(t("settings.notify"))
            .subtitle(t("settings.notify_sub"))
            .active(settings.desktop_notifications)
            .build();
        let tray_switch = libadwaita::SwitchRow::builder()
            .title(t("settings.tray"))
            .subtitle(t("settings.tray_sub"))
            .active(settings.close_to_tray)
            .build();
        let desktop_group = libadwaita::PreferencesGroup::builder()
            .title(t("settings.desktop"))
            .description(t("settings.desktop_desc"))
            .build();
        desktop_group.add(&notify_switch);
        desktop_group.add(&tray_switch);

        let language_system = t("settings.language_system");
        let language_zh = t("settings.language_zh");
        let language_en = t("settings.language_en");
        let language_model = gtk::StringList::new(&[&language_system, &language_zh, &language_en]);
        let language = libadwaita::ComboRow::builder()
            .title(t("settings.language"))
            .subtitle(t("settings.language_sub"))
            .model(&language_model)
            .build();
        let language_index = match settings.locale.as_str() {
            "zh_CN" | "zh" => 1,
            "en" | "en_US" => 2,
            _ => 0,
        };
        language.set_selected(language_index);
        desktop_group.add(&language);

        let file_row = cap_row(&t("settings.file_chooser"));
        let notify_row = cap_row(&t("settings.notifications"));
        let shortcut_row = cap_row(&t("settings.shortcuts"));
        let tray_row = cap_row(&t("settings.tray_cap"));
        let caps_group = libadwaita::PreferencesGroup::builder()
            .title(t("settings.caps"))
            .description(t("settings.caps_desc"))
            .build();
        caps_group.add(&file_row);
        caps_group.add(&notify_row);
        caps_group.add(&shortcut_row);
        caps_group.add(&tray_row);

        let bind = gtk::Button::builder()
            .label(t("settings.bind"))
            .css_classes(["pill", "suggested-action"])
            .build();
        bind.set_tooltip_text(Some(
            tf("settings.bind_tip", &[SHORTCUT_PREFERRED_TRIGGER]).as_str(),
        ));
        let refresh = gtk::Button::builder()
            .label(t("settings.reprobe"))
            .css_classes(["pill"])
            .build();
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.append(&bind);
        buttons.append(&refresh);

        let path_label = gtk::Label::builder()
            .label(tf(
                "settings.config",
                &[&config_path().display().to_string()],
            ))
            .wrap(true)
            .xalign(0.0)
            .selectable(true)
            .css_classes(["caption", "dim-label"])
            .build();

        let mut notes = Vec::new();
        if env_is_set("MONICA_GTK_AUTO_LOCK_SECS") {
            notes.push(t("settings.env_autolock"));
        }
        if env_is_set("MONICA_GTK_CLIPBOARD_CLEAR_SECS") {
            notes.push(t("settings.env_clipboard"));
        }
        let env_note = gtk::Label::builder()
            .label(if notes.is_empty() {
                t("settings.env_none")
            } else {
                notes.join("；")
            })
            .wrap(true)
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build();

        auto_lock.connect_value_notify(glib::clone!(
            #[strong]
            state,
            #[weak]
            clipboard,
            move |row| {
                state.touch();
                let mut next = current();
                next.auto_lock_secs = (row.value() as u32).saturating_mul(60).clamp(3, 7200);
                next.clipboard_clear_secs = (clipboard.value() as u32).clamp(1, 600);
                replace(next);
            }
        ));
        clipboard.connect_value_notify(glib::clone!(
            #[strong]
            state,
            #[weak]
            auto_lock,
            move |row| {
                state.touch();
                let mut next = current();
                next.auto_lock_secs = (auto_lock.value() as u32).saturating_mul(60).clamp(3, 7200);
                next.clipboard_clear_secs = (row.value() as u32).clamp(1, 600);
                replace(next);
            }
        ));
        notify_switch.connect_active_notify(glib::clone!(
            #[strong]
            state,
            move |row| {
                state.touch();
                let mut next = current();
                next.desktop_notifications = row.is_active();
                replace(next);
            }
        ));
        tray_switch.connect_active_notify(glib::clone!(
            #[strong]
            state,
            move |row| {
                state.touch();
                let mut next = current();
                next.close_to_tray = row.is_active();
                replace(next);
                state.apply_close_behavior();
            }
        ));
        language.connect_selected_notify(glib::clone!(
            #[strong]
            state,
            move |row| {
                state.touch();
                let locale = match row.selected() {
                    1 => "zh_CN",
                    2 => "en",
                    _ => "system",
                };
                let mut next = current();
                next.locale = locale.to_string();
                replace(next);
                crate::i18n::apply_preference(locale);
                state
                    .toast
                    .add_toast(libadwaita::Toast::new(&t("settings.language_saved")));
            }
        ));

        let page = Self {
            root: {
                let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
                form.set_margin_start(18);
                form.set_margin_end(18);
                form.set_margin_top(18);
                form.set_margin_bottom(18);
                form.append(
                    &gtk::Label::builder()
                        .label(t("nav.settings"))
                        .css_classes(["title-1"])
                        .xalign(0.0)
                        .build(),
                );
                form.append(&vault_group);
                form.append(&vault_buttons);
                form.append(&vault_status);
                form.append(&group);
                form.append(&desktop_group);
                form.append(&caps_group);
                form.append(&buttons);
                form.append(&path_label);
                form.append(&env_note);
                scrolled_clamp(&form)
            },
            file_row,
            notify_row,
            shortcut_row,
            tray_row,
            vault_path_row,
            vault_status,
        };
        page.sync_from_state(state);

        open_other.connect_clicked(glib::clone!(
            #[strong]
            state,
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
                            state.remember_path(path);
                            state.relock_to_gate();
                        }
                    },
                );
            }
        ));
        create_new.connect_clicked(glib::clone!(
            #[strong]
            state,
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
                            state.remember_path(path);
                            state.relock_to_gate();
                        }
                    },
                );
            }
        ));
        inspect.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                let path = state.vault_path.borrow().clone();
                let status = page.vault_status.clone();
                state.spawn_job(
                    Some(status.clone()),
                    |_| {},
                    &t("unlock.inspecting"),
                    move || monica_vault::inspect_vault(&path),
                    {
                        let toast = state.toast.clone();
                        move |info: monica_vault::VaultInfo| {
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

        bind.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                state
                    .desktop
                    .set_shortcut_status(t("settings.waiting_bind"));
                page.sync_from_state(&state);
                state.desktop.request_bind();
            }
        ));
        refresh.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                let state_ok = state.clone();
                let page_ok = page.clone();
                glib::spawn_future_local(async move {
                    let caps = gio::spawn_blocking(probe).await.unwrap_or_else(|_| {
                        crate::desktop::Capabilities::no_bus(t("settings.probe_failed"))
                    });
                    *state_ok.desktop.caps.borrow_mut() = caps.clone();
                    if !caps.global_shortcuts {
                        state_ok
                            .desktop
                            .set_shortcut_status(caps.shortcut_probe_line());
                    }
                    if !caps.status_notifier {
                        state_ok.desktop.set_tray_status(caps.tray_probe_line());
                    }
                    state_ok.ensure_tray();
                    page_ok.sync_from_state(&state_ok);
                    state_ok
                        .toast
                        .add_toast(libadwaita::Toast::new(&t("settings.reprobed")));
                });
            }
        ));

        page
    }

    pub fn sync_from_state(&self, state: &AppState) {
        let caps = state.desktop.caps.borrow().clone();
        self.file_row.set_subtitle(&caps.file_chooser_line());
        self.notify_row.set_subtitle(&caps.notification_line());
        self.shortcut_row
            .set_subtitle(state.desktop.shortcut_status.borrow().as_str());
        self.tray_row
            .set_subtitle(state.desktop.tray_status.borrow().as_str());
        self.vault_path_row
            .set_subtitle(&state.vault_path.borrow().display().to_string());
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.sync_from_state(state);
    }

    pub fn clear_sensitive(&self) {}
}

fn cap_row(title: &str) -> libadwaita::ActionRow {
    libadwaita::ActionRow::builder()
        .title(title)
        .subtitle(t("settings.probing"))
        .subtitle_selectable(true)
        .build()
}

pub fn scrolled_clamp(child: &impl IsA<gtk::Widget>) -> gtk::Widget {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(
            &libadwaita::Clamp::builder()
                .maximum_size(560)
                .child(child)
                .build(),
        )
        .build()
        .upcast()
}

#[cfg(test)]
mod tests {
    use super::UiSettings;

    #[test]
    fn old_settings_json_keeps_new_flags_true() {
        let parsed: UiSettings =
            serde_json::from_str(r#"{"auto_lock_secs":120,"clipboard_clear_secs":10}"#).unwrap();
        assert_eq!(parsed.auto_lock_secs, 120);
        assert!(parsed.desktop_notifications);
        assert!(parsed.close_to_tray);
        assert_eq!(parsed.locale, "system");
        assert_eq!(parsed.vault_path, None);
    }
}
