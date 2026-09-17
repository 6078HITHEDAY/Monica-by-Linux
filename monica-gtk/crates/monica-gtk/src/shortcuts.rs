//! GlobalShortcuts portal (org.freedesktop.portal.GlobalShortcuts) via ashpd.
//!
//! Binding is user-initiated (settings button) because the portal shows a
//! consent dialog. The worker is always spawned so a later Bind can retry
//! after a session without GlobalShortcuts gains the portal.

use std::sync::mpsc::Sender;
use std::time::Duration;

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::Session;
use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::desktop::{
    format_bound_shortcuts, DesktopCmd, ShortcutRequest, SHORTCUT_PREFERRED_TRIGGER,
    SHORTCUT_TOGGLE_ID,
};
use crate::i18n::{t, tf};

const PORTAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

enum LoopControl {
    Shutdown,
    Reconnect { bind: bool },
}

pub fn spawn(cmd_tx: Sender<DesktopCmd>, bind_rx: UnboundedReceiver<ShortcutRequest>) {
    let _ = std::thread::Builder::new()
        .name("monica-shortcuts".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(tf(
                        "shortcut.runtime_failed",
                        &[&error.to_string()],
                    )));
                    return;
                }
            };
            runtime.block_on(shortcut_loop(cmd_tx, bind_rx));
        });
}

async fn shortcut_loop(
    cmd_tx: Sender<DesktopCmd>,
    mut bind_rx: UnboundedReceiver<ShortcutRequest>,
) {
    let mut pending_bind = false;
    loop {
        match connect_and_serve(&cmd_tx, &mut bind_rx, pending_bind).await {
            LoopControl::Shutdown => break,
            LoopControl::Reconnect { bind } => pending_bind = bind,
        }
    }
}

async fn connect_and_serve(
    cmd_tx: &Sender<DesktopCmd>,
    bind_rx: &mut UnboundedReceiver<ShortcutRequest>,
    pending_bind: bool,
) -> LoopControl {
    let proxy = match GlobalShortcuts::new().await {
        Ok(proxy) => proxy,
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(tf(
                "shortcut.connect_failed",
                &[&error.to_string()],
            )));
            return wait_retry_or_shutdown(bind_rx).await;
        }
    };

    let session = match proxy.create_session().await {
        Ok(session) => session,
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(tf(
                "shortcut.session_failed",
                &[&error.to_string()],
            )));
            return wait_retry_or_shutdown(bind_rx).await;
        }
    };

    match tokio::time::timeout(PORTAL_REQUEST_TIMEOUT, proxy.list_shortcuts(&session)).await {
        Ok(Ok(request)) => match request.response() {
            Ok(list) => {
                let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(status_from_list(
                    list.shortcuts(),
                )));
            }
            Err(error) => {
                let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(tf(
                    "shortcut.list_failed",
                    &[&error.to_string()],
                )));
            }
        },
        Ok(Err(error)) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(tf(
                "shortcut.list_err",
                &[&error.to_string()],
            )));
        }
        Err(_) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(t("shortcut.list_timeout")));
        }
    }

    if pending_bind {
        bind_toggle(cmd_tx, &proxy, &session).await;
    }

    let mut activated = match proxy.receive_activated().await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(tf(
                "shortcut.listen_failed",
                &[&error.to_string()],
            )));
            return wait_retry_or_shutdown(bind_rx).await;
        }
    };

    loop {
        tokio::select! {
            req = bind_rx.recv() => {
                match req {
                    Some(ShortcutRequest::Bind) => {
                        bind_toggle(cmd_tx, &proxy, &session).await;
                    }
                    Some(ShortcutRequest::Shutdown) | None => return LoopControl::Shutdown,
                }
            }
            event = activated.next() => {
                match event {
                    Some(event) => {
                        if event.shortcut_id() == SHORTCUT_TOGGLE_ID {
                            let _ = cmd_tx.send(DesktopCmd::ToggleWindow);
                        }
                    }
                    None => return LoopControl::Reconnect { bind: false },
                }
            }
        }
    }
}

async fn bind_toggle(
    cmd_tx: &Sender<DesktopCmd>,
    proxy: &GlobalShortcuts<'_>,
    session: &Session<'_, GlobalShortcuts<'_>>,
) {
    let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(t("settings.waiting_bind")));
    let shortcuts = [NewShortcut::new(SHORTCUT_TOGGLE_ID, t("desktop.toggle_desc"))
        .preferred_trigger(Some(SHORTCUT_PREFERRED_TRIGGER))];
    let result = tokio::time::timeout(
        PORTAL_REQUEST_TIMEOUT,
        proxy.bind_shortcuts(session, &shortcuts, None),
    )
    .await;
    let status = match result {
        Ok(Ok(request)) => match request.response() {
            Ok(bound) => status_from_list(bound.shortcuts()),
            Err(error) => tf("shortcut.rejected", &[&error.to_string()]),
        },
        Ok(Err(error)) => tf("shortcut.bind_failed", &[&error.to_string()]),
        Err(_) => t("shortcut.bind_timeout"),
    };
    let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(status));
}

fn status_from_list(shortcuts: &[ashpd::desktop::global_shortcuts::Shortcut]) -> String {
    if let Some(shortcut) = shortcuts
        .iter()
        .find(|shortcut| shortcut.id() == SHORTCUT_TOGGLE_ID)
    {
        format_bound_shortcuts(&t("desktop.toggle_id"), shortcut.trigger_description())
    } else if shortcuts.is_empty() {
        t("desktop.shortcut_unbound")
    } else {
        tf("shortcut.other_bound", &[&shortcuts.len().to_string()])
    }
}

async fn wait_retry_or_shutdown(bind_rx: &mut UnboundedReceiver<ShortcutRequest>) -> LoopControl {
    loop {
        match bind_rx.recv().await {
            Some(ShortcutRequest::Bind) => return LoopControl::Reconnect { bind: true },
            Some(ShortcutRequest::Shutdown) | None => return LoopControl::Shutdown,
        }
    }
}
