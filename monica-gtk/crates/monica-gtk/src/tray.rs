//! StatusNotifierItem tray via `ksni`.
//!
//! GNOME Shell does not show SNI items unless the AppIndicator extension is
//! installed. KDE Plasma and most wlroots bars do. We probe the watcher at
//! runtime and skip spawn when it is absent.

use std::sync::mpsc::Sender;

use ksni::menu::StandardItem;

use crate::desktop::{send_cmd, DesktopCmd, APP_ID, GNOME_TRAY_HINT};

pub struct MonicaTray {
    pub unlocked: bool,
    commands: Sender<DesktopCmd>,
}

impl MonicaTray {
    pub fn new(commands: Sender<DesktopCmd>) -> Self {
        Self {
            unlocked: false,
            commands,
        }
    }
}

impl ksni::Tray for MonicaTray {
    fn id(&self) -> String {
        APP_ID.into()
    }

    fn title(&self) -> String {
        "Monica".into()
    }

    fn icon_name(&self) -> String {
        "dialog-password".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: "dialog-password".into(),
            icon_pixmap: Vec::new(),
            title: "Monica".into(),
            description: if self.unlocked {
                "已解锁 · 点击显示或隐藏".into()
            } else {
                "已锁定 · 点击显示或隐藏".into()
            },
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        send_cmd(&self.commands, DesktopCmd::ToggleWindow);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let commands = self.commands.clone();
        let unlocked = self.unlocked;
        vec![
            item("显示", {
                let commands = commands.clone();
                move |_| send_cmd(&commands, DesktopCmd::ShowWindow)
            }),
            item("隐藏", {
                let commands = commands.clone();
                move |_| send_cmd(&commands, DesktopCmd::HideWindow)
            }),
            StandardItem {
                label: "锁定".into(),
                enabled: unlocked,
                activate: {
                    let commands = commands.clone();
                    Box::new(move |_| send_cmd(&commands, DesktopCmd::Lock))
                },
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            item("退出", move |_| send_cmd(&commands, DesktopCmd::Quit)),
        ]
    }

    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        send_cmd(
            &self.commands,
            DesktopCmd::TrayStatus(format!("托盘 watcher 离线。{GNOME_TRAY_HINT}")),
        );
        true
    }
}

fn item(
    label: &str,
    activate: impl Fn(&mut MonicaTray) + Send + 'static,
) -> ksni::MenuItem<MonicaTray> {
    StandardItem {
        label: label.into(),
        activate: Box::new(activate),
        ..Default::default()
    }
    .into()
}

pub fn spawn(commands: Sender<DesktopCmd>) -> Result<ksni::blocking::Handle<MonicaTray>, String> {
    use ksni::blocking::TrayMethods;
    MonicaTray::new(commands)
        .spawn()
        .map_err(|error| error.to_string())
}
