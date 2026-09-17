use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gtk4 as gtk;
use gtk4::gio;
use gtk4::gio::prelude::ApplicationExtManual;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{VaultError, VaultSession};

use crate::desktop::DesktopState;

/// Shared window state for unlock, password workspace, clipboard, and auto-lock.
#[derive(Clone)]
pub struct AppState {
    pub application: libadwaita::Application,
    pub window: libadwaita::ApplicationWindow,
    pub toast: libadwaita::ToastOverlay,
    pub window_stack: gtk::Stack,
    pub stack: gtk::Stack,
    pub content_page: libadwaita::NavigationPage,
    pub nav_list: gtk::ListBox,
    pub nav_ids: Rc<Vec<&'static str>>,
    pub lock_button: gtk::Button,
    pub session: Rc<RefCell<Option<VaultSession>>>,
    pub last_activity: Rc<Cell<Instant>>,
    pub clipboard_generation: Rc<Cell<u64>>,
    pub unlock_status: Rc<RefCell<Option<gtk::Label>>>,
    pub vault_path: Rc<RefCell<PathBuf>>,
    pub desktop: DesktopState,
    /// Set by the shell after pages exist so Settings can return to the gate.
    pub relock: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    pub sync_gate: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
}

impl AppState {
    pub fn touch(&self) {
        self.last_activity.set(Instant::now());
    }

    pub fn show_shell(&self) {
        self.window_stack.set_visible_child_name("shell");
    }

    pub fn show_gate(&self) {
        self.window_stack.set_visible_child_name("gate");
        if let Some(sync) = self.sync_gate.borrow().as_ref() {
            sync();
        }
    }

    pub fn relock_to_gate(&self) {
        if let Some(relock) = self.relock.borrow().as_ref() {
            relock();
        } else {
            self.show_gate();
        }
    }

    pub fn select_nav(&self, id: &str) {
        if let Some(index) = self.nav_ids.iter().position(|item| *item == id) {
            if let Some(row) = self.nav_list.row_at_index(index as i32) {
                self.nav_list.select_row(Some(&row));
            }
        }
    }

    pub fn remember_path(&self, path: PathBuf) {
        *self.vault_path.borrow_mut() = path.clone();
        let mut next = crate::prefs::current();
        next.vault_path = Some(path.to_string_lossy().into_owned());
        crate::prefs::replace(next);
    }

    pub fn current_session(&self) -> Option<VaultSession> {
        self.session.borrow().clone()
    }

    pub fn replace_session(&self, next: Option<VaultSession>) {
        if let Some(previous) = self.session.borrow_mut().take() {
            previous.lock();
        }
        *self.session.borrow_mut() = next;
        self.lock_button
            .set_visible(self.session.borrow().is_some());
        if let Some(session) = self.session.borrow().as_ref() {
            *self.vault_path.borrow_mut() = session.info().path.clone();
        }
        self.sync_tray_unlocked();
        self.apply_close_behavior();
    }

    pub fn apply_close_behavior(&self) {
        let hide = crate::desktop::tray_should_handle_close(
            crate::prefs::current().close_to_tray,
            self.desktop.tray.borrow().is_some(),
            self.desktop.tray_watcher_online.get(),
        );
        self.window.set_hide_on_close(hide);
    }

    /// Start or stop the SNI tray to match the latest capability probe.
    pub fn ensure_tray(&self) {
        let want = self.desktop.caps.borrow().status_notifier;
        if want {
            if self.desktop.tray.borrow().is_none() {
                match crate::tray::spawn(self.desktop.commands.clone()) {
                    Ok(handle) => {
                        *self.desktop.tray.borrow_mut() = Some(handle);
                        self.desktop.tray_watcher_online.set(true);
                        self.desktop
                            .set_tray_status(crate::i18n::t("desktop.tray_connected"));
                        if self.desktop.tray_hold.borrow().is_none() {
                            *self.desktop.tray_hold.borrow_mut() = Some(self.application.hold());
                        }
                    }
                    Err(error) => {
                        self.desktop.tray_watcher_online.set(false);
                        self.desktop
                            .set_tray_status(crate::i18n::tf("desktop.tray_failed", &[&error]));
                    }
                }
            }
        } else if self.desktop.tray.borrow().is_some() {
            if let Some(handle) = self.desktop.tray.borrow_mut().take() {
                let _ = handle.shutdown();
            }
            drop(self.desktop.tray_hold.borrow_mut().take());
            self.desktop.tray_watcher_online.set(false);
            self.desktop
                .set_tray_status(self.desktop.caps.borrow().tray_probe_line());
        }
        self.apply_close_behavior();
    }

    pub fn sync_tray_unlocked(&self) {
        let unlocked = self.session.borrow().is_some();
        if let Some(handle) = self.desktop.tray.borrow().as_ref() {
            handle.update(|tray| {
                tray.unlocked = unlocked;
            });
        }
    }

    pub fn notify(&self, id: &str, title: &str, body: &str) {
        if !crate::prefs::current().desktop_notifications {
            return;
        }
        let notification = gio::Notification::new(title);
        notification.set_body(Some(body));
        notification.set_default_action("app.show-window");
        self.application.send_notification(Some(id), &notification);
    }

    pub fn show_error(&self, status: Option<&gtk::Label>, message: &str) {
        if let Some(status) = status {
            status.set_label(&crate::i18n::tf("common.failed", &[message]));
        }
        self.toast.add_toast(libadwaita::Toast::new(message));
    }

    pub fn spawn_job<T, F, OnOk>(
        &self,
        status: Option<gtk::Label>,
        set_busy: impl Fn(bool) + 'static,
        busy_text: impl AsRef<str>,
        work: F,
        on_ok: OnOk,
    ) where
        T: Send + 'static,
        F: FnOnce() -> Result<T, VaultError> + Send + 'static,
        OnOk: FnOnce(T) + 'static,
    {
        self.touch();
        set_busy(true);
        if let Some(status) = &status {
            status.set_label(busy_text.as_ref());
        }
        let state = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(work).await;
            set_busy(false);
            match result {
                Ok(Ok(value)) => on_ok(value),
                Ok(Err(error)) => state.show_error(status.as_ref(), &error.to_string()),
                Err(_) => {
                    state.show_error(status.as_ref(), &crate::i18n::t("common.background_failed"))
                }
            }
        });
    }
}
