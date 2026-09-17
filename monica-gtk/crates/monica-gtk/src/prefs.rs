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
use crate::security::{AUTO_LOCK_SECS, CLIPBOARD_CLEAR_SECS};
use crate::state::AppState;

static LIVE: OnceLock<Mutex<UiSettings>> = OnceLock::new();

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub auto_lock_secs: u32,
    pub clipboard_clear_secs: u32,
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            auto_lock_secs: AUTO_LOCK_SECS,
            clipboard_clear_secs: CLIPBOARD_CLEAR_SECS,
            desktop_notifications: true,
            close_to_tray: true,
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

        let notify_switch = libadwaita::SwitchRow::builder()
            .title("桌面通知")
            .subtitle("剪贴板清除、自动锁定、备份完成")
            .active(settings.desktop_notifications)
            .build();
        let tray_switch = libadwaita::SwitchRow::builder()
            .title("关闭时留在托盘")
            .subtitle("无托盘时关闭即退出")
            .active(settings.close_to_tray)
            .build();
        let desktop_group = libadwaita::PreferencesGroup::builder()
            .title("桌面")
            .description(
                "通知走 Gio Notification（Wayland 上为 portal）。托盘为 StatusNotifierItem。",
            )
            .build();
        desktop_group.add(&notify_switch);
        desktop_group.add(&tray_switch);

        let file_row = cap_row("文件选择");
        let notify_row = cap_row("通知");
        let shortcut_row = cap_row("全局快捷键");
        let tray_row = cap_row("托盘");
        let caps_group = libadwaita::PreferencesGroup::builder()
            .title("运行时能力")
            .description("启动时探测 D-Bus / portal，不硬编码「平台受限」。")
            .build();
        caps_group.add(&file_row);
        caps_group.add(&notify_row);
        caps_group.add(&shortcut_row);
        caps_group.add(&tray_row);

        let bind = gtk::Button::builder()
            .label("注册全局快捷键")
            .css_classes(["pill", "suggested-action"])
            .build();
        bind.set_tooltip_text(Some(&format!(
            "首选 {SHORTCUT_PREFERRED_TRIGGER} 显示/隐藏。需系统对话框确认。"
        )));
        let refresh = gtk::Button::builder()
            .label("重新探测")
            .css_classes(["pill"])
            .build();
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.append(&bind);
        buttons.append(&refresh);

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
                "未设环境变量覆盖。窗口内 Ctrl+L 锁定，Ctrl+Q 退出。".to_string()
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

        let page = Self {
            root: {
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
        };
        page.sync_from_state(state);

        bind.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                state
                    .desktop
                    .set_shortcut_status("等待系统对话框确认快捷键…");
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
                    let caps = gio::spawn_blocking(probe)
                        .await
                        .unwrap_or_else(|_| crate::desktop::Capabilities::no_bus("探测任务失败"));
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
                        .add_toast(libadwaita::Toast::new("已重新探测桌面能力"));
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
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.sync_from_state(state);
    }

    pub fn clear_sensitive(&self) {}
}

fn cap_row(title: &str) -> libadwaita::ActionRow {
    libadwaita::ActionRow::builder()
        .title(title)
        .subtitle("探测中…")
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
    }
}
