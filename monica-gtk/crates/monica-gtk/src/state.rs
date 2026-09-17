use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{VaultError, VaultSession};

/// Shared window state for unlock, password workspace, clipboard, and auto-lock.
#[derive(Clone)]
pub struct AppState {
    pub window: libadwaita::ApplicationWindow,
    pub toast: libadwaita::ToastOverlay,
    pub stack: gtk::Stack,
    pub content_page: libadwaita::NavigationPage,
    pub nav_list: gtk::ListBox,
    pub lock_button: gtk::Button,
    pub placeholder: libadwaita::StatusPage,
    pub session: Rc<RefCell<Option<VaultSession>>>,
    pub last_activity: Rc<Cell<Instant>>,
    pub clipboard_generation: Rc<Cell<u64>>,
}

impl AppState {
    pub fn touch(&self) {
        self.last_activity.set(Instant::now());
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
    }

    pub fn show_error(&self, status: Option<&gtk::Label>, message: &str) {
        if let Some(status) = status {
            status.set_label(&format!("失败：{message}"));
        }
        self.toast.add_toast(libadwaita::Toast::new(message));
    }

    pub fn spawn_job<T, F, OnOk>(
        &self,
        status: Option<gtk::Label>,
        set_busy: impl Fn(bool) + 'static,
        busy_text: &str,
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
            status.set_label(busy_text);
        }
        let state = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(work).await;
            set_busy(false);
            match result {
                Ok(Ok(value)) => on_ok(value),
                Ok(Err(error)) => state.show_error(status.as_ref(), &error.to_string()),
                Err(_) => state.show_error(status.as_ref(), "后台任务失败"),
            }
        });
    }
}
