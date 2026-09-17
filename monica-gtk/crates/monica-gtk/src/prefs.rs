//! Persistent UI settings: auto-lock and clipboard-clear timeouts.
//!
//! File: `$XDG_CONFIG_HOME/monica-gtk/settings.json` (fallback `~/.config`).
//! Environment variables still override the file when set.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use serde::{Deserialize, Serialize};

use crate::security::{AUTO_LOCK_SECS, CLIPBOARD_CLEAR_SECS};
use crate::state::AppState;

static LIVE: OnceLock<Mutex<UiSettings>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub auto_lock_secs: u32,
    pub clipboard_clear_secs: u32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            auto_lock_secs: AUTO_LOCK_SECS,
            clipboard_clear_secs: CLIPBOARD_CLEAR_SECS,
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
    live()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
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
    std::env::var(name).ok().and_then(|value| value.parse().ok())
}

fn env_is_set(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

#[derive(Clone)]
pub struct SettingsPage {
    pub root: gtk::Widget,
}

impl SettingsPage {
    pub fn build(state: &AppState) -> Self {
        let settings = current();
        let auto_lock = libadwaita::SpinRow::builder()
            .title("自动锁定")
            .subtitle("空闲分钟后锁定")
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
            .title("剪贴板清除")
            .subtitle("复制后秒数")
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
            .title("安全")
            .description("短超时，写在本机配置文件。环境变量若已设置会在下次启动覆盖。")
            .build();
        group.add(&auto_lock);
        group.add(&clipboard);

        let path_label = gtk::Label::builder()
            .label(format!("配置：{}", config_path().display()))
            .wrap(true)
            .xalign(0.0)
            .selectable(true)
            .css_classes(["caption", "dim-label"])
            .build();

        let mut notes = Vec::new();
        if env_is_set("MONICA_GTK_AUTO_LOCK_SECS") {
            notes.push("已设 MONICA_GTK_AUTO_LOCK_SECS");
        }
        if env_is_set("MONICA_GTK_CLIPBOARD_CLEAR_SECS") {
            notes.push("已设 MONICA_GTK_CLIPBOARD_CLEAR_SECS");
        }
        let env_note = gtk::Label::builder()
            .label(if notes.is_empty() {
                "未设环境变量覆盖。".to_string()
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

        let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label("设置")
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&group);
        form.append(&path_label);
        form.append(&env_note);

        Self {
            root: scrolled_clamp(&form),
        }
    }

    pub fn on_session_changed(&self, _state: &AppState) {}

    pub fn clear_sensitive(&self) {}
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
