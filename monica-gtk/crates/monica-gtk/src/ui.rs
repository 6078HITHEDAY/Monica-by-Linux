use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{
    create_vault, inspect_vault, secret_password, unlock_vault, VaultError, VaultInfo,
};

const APP_ID: &str = "com.monicapass.MonicaGtk";
const NAV_ITEMS: [(&str, &str); 5] = [
    ("unlock", "解锁"),
    ("passwords", "密码库"),
    ("otp", "动态口令"),
    ("notes", "安全笔记"),
    ("wallet", "钱包"),
];

pub fn run() {
    let application = libadwaita::Application::builder()
        .application_id(APP_ID)
        .build();
    application.connect_activate(build_window);
    application.run();
}

fn build_window(application: &libadwaita::Application) {
    let window = libadwaita::ApplicationWindow::builder()
        .application(application)
        .title("Monica")
        .default_width(960)
        .default_height(620)
        .width_request(720)
        .height_request(480)
        .build();

    let split_view = libadwaita::NavigationSplitView::new();
    split_view.set_min_sidebar_width(220.0);
    split_view.set_sidebar_width_fraction(0.28);

    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);
    for (_id, title) in NAV_ITEMS {
        let row = libadwaita::ActionRow::builder()
            .title(title)
            .activatable(true)
            .build();
        list.append(&row);
    }
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }

    let sidebar_toolbar = libadwaita::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&libadwaita::HeaderBar::new());
    sidebar_toolbar.set_content(Some(&list));
    let sidebar_page = libadwaita::NavigationPage::builder()
        .title("工作区")
        .child(&sidebar_toolbar)
        .build();

    let toast_overlay = libadwaita::ToastOverlay::new();
    let content_toolbar = libadwaita::ToolbarView::new();
    content_toolbar.add_top_bar(&libadwaita::HeaderBar::new());

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.add_named(
        &build_unlock_page(window.clone(), toast_overlay.clone()),
        Some("unlock"),
    );
    let placeholder = libadwaita::StatusPage::builder()
        .icon_name("view-list-symbolic")
        .title("密码库")
        .description("Phase 0 占位：后续阶段实现此工作区。")
        .build();
    stack.add_named(&placeholder, Some("placeholder"));
    stack.set_visible_child_name("unlock");
    content_toolbar.set_content(Some(&stack));
    toast_overlay.set_child(Some(&content_toolbar));
    let content_page = libadwaita::NavigationPage::builder()
        .title("解锁")
        .child(&toast_overlay)
        .build();

    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));

    let breakpoint = libadwaita::Breakpoint::new(libadwaita::BreakpointCondition::new_length(
        libadwaita::BreakpointConditionLengthType::MaxWidth,
        720.0,
        libadwaita::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);

    list.connect_row_selected(glib::clone!(
        #[weak]
        split_view,
        #[weak]
        content_page,
        #[weak]
        stack,
        #[weak]
        placeholder,
        move |_list, row| {
            let Some(row) = row else {
                return;
            };
            let title = row
                .downcast_ref::<libadwaita::ActionRow>()
                .map(|action| action.title())
                .unwrap_or_else(|| "Monica".into());
            content_page.set_title(&title);
            if row.index() == 0 {
                stack.set_visible_child_name("unlock");
            } else {
                placeholder.set_title(&title);
                stack.set_visible_child_name("placeholder");
            }
            split_view.set_show_content(true);
        }
    ));

    window.set_content(Some(&split_view));
    window.present();
}

fn build_unlock_page(
    window: libadwaita::ApplicationWindow,
    toast_overlay: libadwaita::ToastOverlay,
) -> gtk::Widget {
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

    let browse = gtk::Button::from_icon_name("document-open-symbolic");
    browse.set_valign(gtk::Align::Center);
    browse.set_tooltip_text(Some("选择 .mdbx 文件"));
    path_row.add_suffix(&browse);

    let password_row = libadwaita::PasswordEntryRow::builder()
        .title("主密码")
        .build();

    let group = libadwaita::PreferencesGroup::builder()
        .title("本地保险库")
        .description("「仅打开」为只读检查，不会原地升级。MDBX-1 / 旧 schema 必须先复制再解锁。")
        .build();
    group.add(&path_row);
    group.add(&password_row);

    let unlock_button = gtk::Button::builder()
        .label("解锁")
        .css_classes(["suggested-action", "pill"])
        .build();
    let create_button = gtk::Button::builder()
        .label("创建演示保险库")
        .css_classes(["pill"])
        .build();
    let inspect_button = gtk::Button::builder()
        .label("仅打开（不解锁）")
        .css_classes(["pill", "flat"])
        .build();

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
    let actions = VaultActions {
        unlock: unlock_button.clone(),
        create: create_button.clone(),
        inspect: inspect_button.clone(),
        browse: browse.clone(),
    };

    browse.connect_clicked(glib::clone!(
        #[weak]
        window,
        #[weak]
        path_row,
        move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("选择 Monica 保险库")
                .build();
            let filter = gtk::FileFilter::new();
            filter.add_pattern("*.mdbx");
            filter.set_name(Some("Monica 保险库 (*.mdbx)"));
            dialog.set_default_filter(Some(&filter));
            dialog.open(
                Some(&window),
                None::<&gio::Cancellable>,
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
        #[weak]
        path_row,
        #[weak]
        password_row,
        #[weak]
        status,
        #[weak]
        toast_overlay,
        #[strong]
        actions,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            let password = secret_password(password_row.text().to_string());
            password_row.set_text("");
            spawn_vault_job(
                actions.clone(),
                status,
                toast_overlay,
                "正在解锁保险库…",
                move || unlock_vault(&path, &password),
                |status, toast, info| show_vault_result(status, toast, "已解锁", &info),
            );
        }
    ));

    inspect_button.connect_clicked(glib::clone!(
        #[weak]
        path_row,
        #[weak]
        status,
        #[weak]
        toast_overlay,
        #[strong]
        actions,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            spawn_vault_job(
                actions.clone(),
                status,
                toast_overlay,
                "正在只读检查保险库…",
                move || inspect_vault(&path),
                |status, toast, info| {
                    show_vault_result(status, toast, "已打开（只读，未解锁）", &info)
                },
            );
        }
    ));

    create_button.connect_clicked(glib::clone!(
        #[weak]
        path_row,
        #[weak]
        password_row,
        #[weak]
        status,
        #[weak]
        toast_overlay,
        #[strong]
        last_path,
        #[strong]
        actions,
        move |_| {
            let mut path = PathBuf::from(path_row.text().as_str());
            if path.as_os_str().is_empty() {
                path = last_path.borrow().clone();
                path_row.set_text(&path.to_string_lossy());
            }
            let typed = password_row.text().to_string();
            let password = if typed.is_empty() {
                secret_password("phase0-demo".to_string())
            } else {
                secret_password(typed)
            };
            password_row.set_text("");
            let remembered = last_path.clone();
            let created_path = path.clone();
            spawn_vault_job(
                actions.clone(),
                status,
                toast_overlay,
                "正在创建演示保险库…",
                move || create_vault(&path, &password),
                move |status, toast, info| {
                    *remembered.borrow_mut() = created_path;
                    show_vault_result(
                        status,
                        toast,
                        "已创建演示保险库（主密码已用于配置解锁方式）",
                        &info,
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

#[derive(Clone)]
struct VaultActions {
    unlock: gtk::Button,
    create: gtk::Button,
    inspect: gtk::Button,
    browse: gtk::Button,
}

impl VaultActions {
    fn set_busy(&self, busy: bool) {
        let sensitive = !busy;
        self.unlock.set_sensitive(sensitive);
        self.create.set_sensitive(sensitive);
        self.inspect.set_sensitive(sensitive);
        self.browse.set_sensitive(sensitive);
    }
}

fn spawn_vault_job<F, OnOk>(
    actions: VaultActions,
    status: gtk::Label,
    toast_overlay: libadwaita::ToastOverlay,
    busy_text: &str,
    work: F,
    on_ok: OnOk,
) where
    F: FnOnce() -> Result<VaultInfo, VaultError> + Send + 'static,
    OnOk: FnOnce(&gtk::Label, &libadwaita::ToastOverlay, VaultInfo) + 'static,
{
    actions.set_busy(true);
    status.set_label(busy_text);

    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(work).await;
        actions.set_busy(false);
        match result {
            Ok(Ok(info)) => on_ok(&status, &toast_overlay, info),
            Ok(Err(error)) => show_error(&status, &toast_overlay, &error.to_string()),
            Err(_) => show_error(&status, &toast_overlay, "后台任务失败"),
        }
    });
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

fn show_vault_result(
    status: &gtk::Label,
    toast_overlay: &libadwaita::ToastOverlay,
    heading: &str,
    info: &VaultInfo,
) {
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
    let text = format!(
        "{heading}\n路径：{}\nvault_id：{}\n格式：{}  schema：{}  Tiga：{}\n会话：{}{upgrade}",
        info.path.display(),
        info.vault_id,
        info.format_version,
        info.schema_version,
        info.tiga_mode,
        session
    );
    status.set_label(&text);
    toast_overlay.add_toast(libadwaita::Toast::new(heading));
}

fn show_error(status: &gtk::Label, toast_overlay: &libadwaita::ToastOverlay, message: &str) {
    status.set_label(&format!("失败：{message}"));
    toast_overlay.add_toast(libadwaita::Toast::new(message));
}

fn default_vault_path() -> PathBuf {
    std::env::temp_dir()
        .join("monica-gtk-phase0")
        .join("local.mdbx")
}
