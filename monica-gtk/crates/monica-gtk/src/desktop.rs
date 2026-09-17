//! Runtime probe of xdg-desktop-portal and StatusNotifierWatcher.
//!
//! Issue #8 wants capability state detected at runtime instead of hardcoded
//! "platform limited". All D-Bus calls are best-effort and never fail the app.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use zbus::blocking::Connection;
use zbus::names::BusName;

pub const APP_ID: &str = "com.monicapass.MonicaGtk";
pub const SHORTCUT_TOGGLE_ID: &str = "toggle-window";
pub const SHORTCUT_PREFERRED_TRIGGER: &str = "<Control><Alt>M";
pub const GNOME_TRAY_HINT: &str =
    "GNOME 需 AppIndicator 扩展（gnome-shell-extension-appindicator）";

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const FDO_NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const SNI_WATCHERS: [&str; 2] = [
    "org.kde.StatusNotifierWatcher",
    "org.freedesktop.StatusNotifierWatcher",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub session_bus: bool,
    pub portal_service: bool,
    pub file_chooser: bool,
    pub notification_portal: bool,
    pub fdo_notifications: bool,
    pub global_shortcuts: bool,
    pub status_notifier: bool,
    pub sandboxed: bool,
    pub bus_error: Option<String>,
}

impl Capabilities {
    pub fn no_bus(reason: impl Into<String>) -> Self {
        Self {
            session_bus: false,
            portal_service: false,
            file_chooser: false,
            notification_portal: false,
            fdo_notifications: false,
            global_shortcuts: false,
            status_notifier: false,
            sandboxed: running_sandboxed(),
            bus_error: Some(reason.into()),
        }
    }

    pub fn from_portal_xml(
        session_bus: bool,
        portal_service: bool,
        xml: &str,
        fdo_notifications: bool,
        status_notifier: bool,
        bus_error: Option<String>,
    ) -> Self {
        Self {
            session_bus,
            portal_service,
            file_chooser: xml_has_iface(xml, "org.freedesktop.portal.FileChooser"),
            notification_portal: xml_has_iface(xml, "org.freedesktop.portal.Notification"),
            fdo_notifications,
            global_shortcuts: xml_has_iface(xml, "org.freedesktop.portal.GlobalShortcuts"),
            status_notifier,
            sandboxed: running_sandboxed(),
            bus_error,
        }
    }

    pub fn file_chooser_line(&self) -> String {
        if self.file_chooser {
            "GtkFileDialog · FileChooser portal 已探测".into()
        } else if self.session_bus {
            "GtkFileDialog · 无 FileChooser portal，GTK 后端回退".into()
        } else {
            format!("GtkFileDialog · 无会话总线（{}）", self.bus_reason())
        }
    }

    pub fn notification_line(&self) -> String {
        if self.notification_portal {
            "Gio Notification · Notification portal 已探测".into()
        } else if self.fdo_notifications {
            "Gio Notification · 仅有 org.freedesktop.Notifications".into()
        } else if self.session_bus {
            "会话无通知服务，仅应用内 toast".into()
        } else {
            format!("无会话总线（{}）", self.bus_reason())
        }
    }

    pub fn shortcut_probe_line(&self) -> String {
        if self.global_shortcuts {
            "portal 已探测，尚未绑定".into()
        } else if self.portal_service {
            "portal 无 GlobalShortcuts（会话/后端未提供）".into()
        } else if self.session_bus {
            "无 xdg-desktop-portal".into()
        } else {
            format!("无会话总线（{}）", self.bus_reason())
        }
    }

    pub fn tray_probe_line(&self) -> String {
        if self.status_notifier {
            "StatusNotifierWatcher 已探测".into()
        } else if self.session_bus {
            format!("未发现托盘 watcher。{GNOME_TRAY_HINT}")
        } else {
            format!("无会话总线（{}）", self.bus_reason())
        }
    }

    pub fn report_lines(&self) -> Vec<String> {
        vec![
            format!(
                "desktop: bus={} portal={} sandbox={}",
                yn(self.session_bus),
                yn(self.portal_service),
                yn(self.sandboxed)
            ),
            format!("file-chooser: {}", yn(self.file_chooser)),
            format!(
                "notification: portal={} fdo={}",
                yn(self.notification_portal),
                yn(self.fdo_notifications)
            ),
            format!("global-shortcuts: {}", yn(self.global_shortcuts)),
            format!("status-notifier: {}", yn(self.status_notifier)),
        ]
    }

    fn bus_reason(&self) -> &str {
        self.bus_error.as_deref().unwrap_or("未知")
    }
}

#[derive(Debug, Clone)]
pub enum DesktopCmd {
    ToggleWindow,
    ShowWindow,
    HideWindow,
    Lock,
    Quit,
    ShortcutStatus(String),
    TrayStatus(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutRequest {
    Bind,
    Shutdown,
}

#[derive(Clone)]
pub struct DesktopState {
    pub shortcut_tx: tokio::sync::mpsc::UnboundedSender<ShortcutRequest>,
    pub caps: Rc<RefCell<Capabilities>>,
    pub shortcut_status: Rc<RefCell<String>>,
    pub tray_status: Rc<RefCell<String>>,
    pub tray: Rc<RefCell<Option<ksni::blocking::Handle<crate::tray::MonicaTray>>>>,
    pub tray_hold: Rc<RefCell<Option<gtk4::gio::ApplicationHoldGuard>>>,
}

impl DesktopState {
    pub fn new(shortcut_tx: tokio::sync::mpsc::UnboundedSender<ShortcutRequest>) -> Self {
        Self {
            shortcut_tx,
            caps: Rc::new(RefCell::new(Capabilities::no_bus("尚未探测"))),
            shortcut_status: Rc::new(RefCell::new("尚未探测".into())),
            tray_status: Rc::new(RefCell::new("尚未探测".into())),
            tray: Rc::new(RefCell::new(None)),
            tray_hold: Rc::new(RefCell::new(None)),
        }
    }

    pub fn request_bind(&self) {
        let _ = self.shortcut_tx.send(ShortcutRequest::Bind);
    }

    pub fn set_shortcut_status(&self, text: impl Into<String>) {
        *self.shortcut_status.borrow_mut() = text.into();
    }

    pub fn set_tray_status(&self, text: impl Into<String>) {
        *self.tray_status.borrow_mut() = text.into();
    }
}

/// Probe the session bus. Safe to call off the GTK thread.
pub fn probe() -> Capabilities {
    match Connection::session() {
        Ok(conn) => probe_with_connection(&conn),
        Err(error) => Capabilities::no_bus(error.to_string()),
    }
}

fn probe_with_connection(conn: &Connection) -> Capabilities {
    let portal_service = name_has_owner(conn, PORTAL_DEST);
    let xml = if portal_service {
        introspect_portal(conn).unwrap_or_default()
    } else {
        String::new()
    };
    Capabilities::from_portal_xml(
        true,
        portal_service,
        &xml,
        name_has_owner(conn, FDO_NOTIFICATIONS),
        SNI_WATCHERS.iter().any(|name| name_has_owner(conn, name)),
        None,
    )
}

fn name_has_owner(conn: &Connection, name: &str) -> bool {
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(conn) else {
        return false;
    };
    let Ok(bus_name) = BusName::try_from(name) else {
        return false;
    };
    dbus.name_has_owner(bus_name).unwrap_or(false)
}

fn introspect_portal(conn: &Connection) -> Result<String, String> {
    let proxy = zbus::blocking::Proxy::new(conn, PORTAL_DEST, PORTAL_PATH, PORTAL_DEST)
        .map_err(|error| error.to_string())?;
    proxy.introspect().map_err(|error| error.to_string())
}

pub fn xml_has_iface(xml: &str, iface: &str) -> bool {
    let needle = format!("name=\"{iface}\"");
    xml.contains(&needle) || xml.contains(&format!("name='{iface}'"))
}

fn running_sandboxed() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
        || std::env::var_os("FLATPAK_ID").is_some()
        || std::env::var_os("SNAP").is_some()
}

fn yn(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub fn format_bound_shortcuts(id: &str, trigger: &str) -> String {
    if trigger.is_empty() {
        format!("已绑定：{id}")
    } else {
        format!("已绑定：{id}（{trigger}）")
    }
}

/// Headless report for `--self-test`. Never fails the process.
pub fn self_test_report() -> String {
    let caps = probe();
    let mut lines = vec!["--- desktop capabilities ---".to_string()];
    lines.extend(caps.report_lines());
    if let Some(error) = &caps.bus_error {
        lines.push(format!("bus-error: {error}"));
    }
    lines.push(format!("file-chooser-ui: {}", caps.file_chooser_line()));
    lines.push(format!("notification-ui: {}", caps.notification_line()));
    lines.push(format!("shortcut-ui: {}", caps.shortcut_probe_line()));
    lines.push(format!("tray-ui: {}", caps.tray_probe_line()));
    lines.join("\n")
}

pub fn send_cmd(tx: &Sender<DesktopCmd>, cmd: DesktopCmd) {
    let _ = tx.send(cmd);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_XML: &str = r#"
        <node>
          <interface name="org.freedesktop.portal.FileChooser"/>
          <interface name="org.freedesktop.portal.Notification"/>
          <interface name="org.freedesktop.DBus.Introspectable"/>
        </node>
    "#;

    #[test]
    fn xml_probe_detects_present_and_missing_interfaces() {
        let caps = Capabilities::from_portal_xml(true, true, SAMPLE_XML, true, false, None);
        assert!(caps.file_chooser);
        assert!(caps.notification_portal);
        assert!(caps.fdo_notifications);
        assert!(!caps.global_shortcuts);
        assert!(!caps.status_notifier);
        assert!(caps.file_chooser_line().contains("已探测"));
        assert!(caps.shortcut_probe_line().contains("GlobalShortcuts"));
        assert!(caps.tray_probe_line().contains("AppIndicator"));
    }

    #[test]
    fn xml_probe_without_portal_service() {
        let caps = Capabilities::from_portal_xml(true, false, "", false, true, None);
        assert!(!caps.file_chooser);
        assert!(caps.status_notifier);
        assert!(caps.notification_line().contains("toast"));
        assert!(caps.tray_probe_line().contains("已探测"));
    }

    #[test]
    fn no_bus_report_is_honest() {
        let caps = Capabilities::no_bus("connection refused");
        assert!(!caps.session_bus);
        let report = caps.report_lines().join("\n");
        assert!(report.contains("bus=no"));
        assert!(report.contains("global-shortcuts: no"));
        assert!(caps.file_chooser_line().contains("connection refused"));
    }

    #[test]
    fn bound_shortcut_copy_includes_trigger() {
        assert_eq!(
            format_bound_shortcuts("显示/隐藏", "Ctrl+Alt+M"),
            "已绑定：显示/隐藏（Ctrl+Alt+M）"
        );
        assert_eq!(format_bound_shortcuts("显示/隐藏", ""), "已绑定：显示/隐藏");
    }

    #[test]
    fn shortcut_id_is_stable() {
        assert_eq!(SHORTCUT_TOGGLE_ID, "toggle-window");
        assert!(SHORTCUT_PREFERRED_TRIGGER.contains("Control"));
    }
}
