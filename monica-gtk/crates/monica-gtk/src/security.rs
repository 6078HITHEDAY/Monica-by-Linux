//! Clipboard timeout and idle auto-lock constants.
//!
//! Clipboard writes use [`gdk::Clipboard`] (`WidgetExt::clipboard`), the GTK4
//! API that talks to the Wayland clipboard / xdg-desktop-portal instead of
//! X11 `XSetSelectionOwner`. After the timeout we re-read the clipboard and
//! clear it only when the text is still the secret we wrote.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize;

/// Matches Avalonia `AppSettingsService.ClipboardClearSeconds` default.
pub const CLIPBOARD_CLEAR_SECS: u32 = 30;
/// Matches Avalonia `AppSettingsService.AutoLockMinutes` default (5 minutes).
pub const AUTO_LOCK_SECS: u32 = 300;

pub fn clipboard_clear_secs() -> u32 {
    crate::prefs::current().clipboard_clear_secs
}

pub fn auto_lock_secs() -> u32 {
    crate::prefs::current().auto_lock_secs
}

/// True when the clipboard still holds the exact secret Monica copied.
pub fn clipboard_still_ours(ours: &str, current: Option<&str>) -> bool {
    !ours.is_empty() && current == Some(ours)
}

/// Copy a secret with GdkClipboard and clear it after the timeout if Monica
/// still owns the text (the user has not copied something else).
pub fn copy_secret_with_timeout(
    widget: &impl IsA<gtk::Widget>,
    secret: &SecretString,
    toast_overlay: &libadwaita::ToastOverlay,
    generation: &Rc<Cell<u64>>,
) {
    let mut plaintext = secret.expose_secret().to_string();
    if plaintext.is_empty() {
        toast_overlay.add_toast(libadwaita::Toast::new("没有可复制的内容"));
        return;
    }

    let clipboard = widget.clipboard();
    clipboard.set_text(&plaintext);

    let next = generation.get().wrapping_add(1);
    generation.set(next);
    let seconds = clipboard_clear_secs();
    toast_overlay.add_toast(libadwaita::Toast::new(&format!(
        "已复制，{seconds} 秒后清除剪贴板（若仍是本内容）"
    )));

    let toast_overlay = toast_overlay.clone();
    let generation = generation.clone();
    glib::timeout_add_local(Duration::from_secs(u64::from(seconds)), move || {
        if generation.get() != next {
            plaintext.zeroize();
            return glib::ControlFlow::Break;
        }
        let expected = plaintext.clone();
        plaintext.zeroize();
        let clipboard = clipboard.clone();
        let toast_overlay = toast_overlay.clone();
        let generation = generation.clone();
        let clipboard_for_clear = clipboard.clone();
        clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
            if generation.get() != next {
                return;
            }
            let current = result.ok().flatten();
            let ours = clipboard_still_ours(&expected, current.as_deref());
            drop(current);
            if ours {
                let _ = clipboard_for_clear.set_content(None::<&gtk::gdk::ContentProvider>);
                toast_overlay.add_toast(libadwaita::Toast::new("已从剪贴板清除复制的秘密"));
            }
        });
        glib::ControlFlow::Break
    });
}

#[cfg(test)]
mod tests {
    use super::clipboard_still_ours;

    #[test]
    fn clear_only_when_clipboard_still_holds_our_secret() {
        assert!(clipboard_still_ours("s3cret", Some("s3cret")));
        assert!(!clipboard_still_ours("s3cret", Some("something-else")));
        assert!(!clipboard_still_ours("s3cret", None));
        assert!(!clipboard_still_ours("", Some("")));
    }
}
