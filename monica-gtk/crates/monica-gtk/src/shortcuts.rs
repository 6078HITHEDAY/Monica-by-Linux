//! GlobalShortcuts portal (org.freedesktop.portal.GlobalShortcuts) via ashpd.
//!
//! Binding is user-initiated (settings button) because the portal shows a
//! consent dialog. If the session has no GlobalShortcuts interface, we never
//! pretend the hotkey is registered.

use std::sync::mpsc::Sender;
use std::time::Duration;

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::desktop::{
    format_bound_shortcuts, DesktopCmd, ShortcutRequest, SHORTCUT_PREFERRED_TRIGGER,
    SHORTCUT_TOGGLE_ID,
};

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
                    let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(format!(
                        "快捷键运行时失败：{error}"
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
    let proxy = match GlobalShortcuts::new().await {
        Ok(proxy) => proxy,
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(format!(
                "无法连接 GlobalShortcuts：{error}"
            )));
            wait_shutdown(&mut bind_rx).await;
            return;
        }
    };

    let session = match proxy.create_session().await {
        Ok(session) => session,
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(format!(
                "CreateSession 失败：{error}"
            )));
            wait_shutdown(&mut bind_rx).await;
            return;
        }
    };

    match proxy.list_shortcuts(&session).await {
        Ok(request) => match request.response() {
            Ok(list) => {
                let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(status_from_list(
                    list.shortcuts(),
                )));
            }
            Err(error) => {
                let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(format!(
                    "读取已绑定快捷键失败：{error}"
                )));
            }
        },
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(format!(
                "ListShortcuts 失败：{error}"
            )));
        }
    }

    let mut activated = match proxy.receive_activated().await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(format!(
                "无法监听快捷键：{error}"
            )));
            wait_shutdown(&mut bind_rx).await;
            return;
        }
    };

    loop {
        tokio::select! {
            req = bind_rx.recv() => {
                match req {
                    Some(ShortcutRequest::Bind) => {
                        let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(
                            "等待系统对话框确认快捷键…".into(),
                        ));
                        let shortcuts = [NewShortcut::new(
                            SHORTCUT_TOGGLE_ID,
                            "显示或隐藏 Monica",
                        )
                        .preferred_trigger(Some(SHORTCUT_PREFERRED_TRIGGER))];
                        let result = tokio::time::timeout(
                            Duration::from_secs(120),
                            proxy.bind_shortcuts(&session, &shortcuts, None),
                        )
                        .await;
                        let status = match result {
                            Ok(Ok(request)) => match request.response() {
                                Ok(bound) => status_from_list(bound.shortcuts()),
                                Err(error) => format!("绑定被拒绝或失败：{error}"),
                            },
                            Ok(Err(error)) => format!("绑定失败：{error}"),
                            Err(_) => "绑定超时（请在系统对话框中确认）".into(),
                        };
                        let _ = cmd_tx.send(DesktopCmd::ShortcutStatus(status));
                    }
                    Some(ShortcutRequest::Shutdown) | None => break,
                }
            }
            event = activated.next() => {
                match event {
                    Some(event) => {
                        if event.shortcut_id() == SHORTCUT_TOGGLE_ID {
                            let _ = cmd_tx.send(DesktopCmd::ToggleWindow);
                        }
                    }
                    None => break,
                }
            }
        }
    }
}

fn status_from_list(shortcuts: &[ashpd::desktop::global_shortcuts::Shortcut]) -> String {
    if let Some(shortcut) = shortcuts
        .iter()
        .find(|shortcut| shortcut.id() == SHORTCUT_TOGGLE_ID)
    {
        format_bound_shortcuts("显示/隐藏", shortcut.trigger_description())
    } else if shortcuts.is_empty() {
        "portal 已探测，尚未绑定".into()
    } else {
        format!("已绑定 {} 个快捷键（不含显示/隐藏）", shortcuts.len())
    }
}

async fn wait_shutdown(bind_rx: &mut UnboundedReceiver<ShortcutRequest>) {
    while let Some(request) = bind_rx.recv().await {
        if matches!(request, ShortcutRequest::Shutdown) {
            break;
        }
    }
}
