use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{create_session, inspect_vault, secret_password, unlock_session, VaultInfo};

use crate::icons;
use crate::passwords::PasswordPage;
use crate::state::AppState;

pub fn build_unlock_page(state: AppState, passwords: PasswordPage) -> gtk::Widget {
    let style = libadwaita::StyleManager::default();
    let theme_label = theme_summary(&style);
    style.connect_dark_notify(glib::clone!(
        #[weak]
        theme_label,
        move |style_manager| {
            theme_label.set_label(&theme_summary_text(style_manager));
        }
    ));

    let path_row = libadwaita::EntryRow::builder().title("保险库路径").build();
    path_row.set_text(&default_vault_path().to_string_lossy());

    let browse = icons::icon_button("folder-open-symbolic", "选择 .mdbx 文件");
    browse.set_valign(gtk::Align::Center);
    path_row.add_suffix(&browse);

    let password_row = libadwaita::PasswordEntryRow::builder()
        .title("主密码")
        .build();

    let group = libadwaita::PreferencesGroup::builder()
        .title("本地保险库")
        .description("「仅打开」为只读检查，不会原地升级。MDBX-1 / 旧 schema 必须先复制再解锁。创建与解锁会打开密码列表。")
        .build();
    group.add(&path_row);
    group.add(&password_row);

    let (unlock_button, _) = icons::labeled_icon_button(
        "dialog-password-symbolic",
        "解锁",
        &["suggested-action", "pill"],
    );
    let (create_button, _) =
        icons::labeled_icon_button("document-new-symbolic", "创建保险库", &["pill"]);
    let (inspect_button, _) = icons::labeled_icon_button(
        "document-open-symbolic",
        "仅打开（不解锁）",
        &["pill", "flat"],
    );

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&unlock_button);
    buttons.append(&create_button);
    buttons.append(&inspect_button);

    let status = gtk::Label::builder()
        .label("尚未打开保险库。")
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    status.set_selectable(true);
    *state.unlock_status.borrow_mut() = Some(status.clone());

    let form = gtk::Box::new(gtk::Orientation::Vertical, 18);
    form.set_valign(gtk::Align::Center);
    form.append(
        &gtk::Label::builder()
            .label("解锁")
            .css_classes(["title-1"])
            .build(),
    );
    form.append(
        &gtk::Label::builder()
            .label("输入主密码以打开本地优先保险库。界面文案使用系统 fontconfig 字体，不指定 Segoe UI / 微软雅黑。")
            .wrap(true)
            .xalign(0.0)
            .css_classes(["body"])
            .build(),
    );
    form.append(&theme_label);
    form.append(&group);
    form.append(&buttons);
    form.append(&status);

    let clamp = libadwaita::Clamp::builder()
        .maximum_size(460)
        .tightening_threshold(320)
        .child(&form)
        .build();

    let last_path: Rc<RefCell<PathBuf>> = Rc::new(RefCell::new(default_vault_path()));
    let actions = UnlockActions {
        unlock: unlock_button.clone(),
        create: create_button.clone(),
        inspect: inspect_button.clone(),
        browse: browse.clone(),
        password: password_row.clone(),
    };

    browse.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[weak]
        path_row,
        move |_| {
            state.touch();
            let dialog = gtk::FileDialog::builder()
                .title("选择 Monica 保险库")
                .build();
            let filter = gtk::FileFilter::new();
            filter.add_pattern("*.mdbx");
            filter.set_name(Some("Monica 保险库 (*.mdbx)"));
            dialog.set_default_filter(Some(&filter));
            dialog.open(
                Some(&state.window),
                None::<&gtk::gio::Cancellable>,
                glib::clone!(
                    #[weak]
                    path_row,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                path_row.set_text(&path.to_string_lossy());
                            }
                        }
                    }
                ),
            );
        }
    ));

    unlock_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        passwords,
        #[strong]
        actions,
        #[weak]
        path_row,
        #[weak]
        status,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            let typed = actions.password.text().to_string();
            if typed.is_empty() {
                state.show_error(Some(&status), "请输入主密码");
                return;
            }
            let password = secret_password(typed);
            actions.password.set_text("");
            let unlock_state = state.clone();
            let unlock_passwords = passwords.clone();
            let unlock_status = status.clone();
            state.spawn_job(
                Some(status.clone()),
                {
                    let actions = actions.clone();
                    move |busy| actions.set_busy(busy)
                },
                "正在解锁保险库…",
                move || unlock_session(&path, &password),
                move |session| {
                    show_session_opened(
                        &unlock_state,
                        &unlock_passwords,
                        &unlock_status,
                        session,
                        "已解锁",
                    );
                },
            );
        }
    ));

    inspect_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        actions,
        #[weak]
        path_row,
        #[weak]
        status,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            state.spawn_job(
                Some(status.clone()),
                {
                    let actions = actions.clone();
                    move |busy| actions.set_busy(busy)
                },
                "正在只读检查保险库…",
                move || inspect_vault(&path),
                {
                    let toast = state.toast.clone();
                    move |info: VaultInfo| {
                    let session = if info.unlocked {
                        "已解锁"
                    } else {
                        "未解锁（只读）"
                    };
                    let upgrade = if info.requires_upgrade {
                        "\n升级：当前格式需要迁移。本次为只读打开，未改写文件。请先复制/备份再解锁。"
                    } else {
                        ""
                    };
                    status.set_label(&format!(
                        "已打开（只读，未解锁）\n路径：{}\nvault_id：{}\n格式：{}  schema：{}  Tiga：{}\n会话：{}{upgrade}",
                        info.path.display(),
                        info.vault_id,
                        info.format_version,
                        info.schema_version,
                        info.tiga_mode,
                        session
                    ));
                    toast.add_toast(libadwaita::Toast::new("已打开（只读，未解锁）"));
                    }
                },
            );
        }
    ));

    create_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        passwords,
        #[strong]
        last_path,
        #[strong]
        actions,
        #[weak]
        path_row,
        #[weak]
        status,
        move |_| {
            let mut path = PathBuf::from(path_row.text().as_str());
            if path.as_os_str().is_empty() {
                path = last_path.borrow().clone();
                path_row.set_text(&path.to_string_lossy());
            }
            let typed = actions.password.text().to_string();
            if typed.is_empty() {
                state.show_error(Some(&status), "请输入主密码后再创建保险库");
                return;
            }
            let password = secret_password(typed);
            actions.password.set_text("");
            let remembered = last_path.clone();
            let created_path = path.clone();
            let create_state = state.clone();
            let create_passwords = passwords.clone();
            let create_status = status.clone();
            state.spawn_job(
                Some(status.clone()),
                {
                    let actions = actions.clone();
                    move |busy| actions.set_busy(busy)
                },
                "正在创建保险库…",
                move || create_session(&path, &password),
                move |session| {
                    *remembered.borrow_mut() = created_path;
                    show_session_opened(
                        &create_state,
                        &create_passwords,
                        &create_status,
                        session,
                        "已创建并解锁保险库",
                    );
                },
            );
        }
    ));

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build();
    scrolled.upcast()
}

fn show_session_opened(
    state: &AppState,
    passwords: &PasswordPage,
    status: &gtk::Label,
    session: monica_vault::VaultSession,
    heading: &str,
) {
    let info = session.info().clone();
    status.set_label(&format!(
        "{heading}\n路径：{}\nvault_id：{}\n格式：{}  schema：{}  Tiga：{}\n会话：已解锁",
        info.path.display(),
        info.vault_id,
        info.format_version,
        info.schema_version,
        info.tiga_mode
    ));
    state.replace_session(Some(session));
    state.toast.add_toast(libadwaita::Toast::new(heading));
    passwords.on_session_changed(state);
    if let Some(row) = state.nav_list.row_at_index(1) {
        state.nav_list.select_row(Some(&row));
    }
    state.stack.set_visible_child_name("passwords");
    state.content_page.set_title("密码库");
}

#[derive(Clone)]
struct UnlockActions {
    unlock: gtk::Button,
    create: gtk::Button,
    inspect: gtk::Button,
    browse: gtk::Button,
    password: libadwaita::PasswordEntryRow,
}

impl UnlockActions {
    fn set_busy(&self, busy: bool) {
        let sensitive = !busy;
        self.unlock.set_sensitive(sensitive);
        self.create.set_sensitive(sensitive);
        self.inspect.set_sensitive(sensitive);
        self.browse.set_sensitive(sensitive);
    }
}

fn theme_summary(style: &libadwaita::StyleManager) -> gtk::Label {
    gtk::Label::builder()
        .label(theme_summary_text(style))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["caption"])
        .build()
}

fn theme_summary_text(style: &libadwaita::StyleManager) -> String {
    let appearance = if style.is_dark() { "深色" } else { "浅色" };
    format!(
        "主题：跟随系统（当前为{appearance}）。强调色使用 libadwaita 默认，无自定义浅色画刷。字体走 Pango / fontconfig。"
    )
}

pub fn default_vault_path() -> PathBuf {
    std::env::temp_dir()
        .join("monica-gtk-phase0")
        .join("local.mdbx")
}
