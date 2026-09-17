use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{create_vault, inspect_vault, secret_password, unlock_vault, VaultInfo};

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
        .description("Phase 0：打开现有 local.mdbx 会走可写升级路径，请先备份。")
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
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            let password = secret_password(password_row.text().to_string());
            password_row.set_text("");
            match unlock_vault(&path, &password) {
                Ok(info) => show_vault_result(&status, &toast_overlay, "已解锁", &info),
                Err(error) => show_error(&status, &toast_overlay, &error.to_string()),
            }
        }
    ));

    inspect_button.connect_clicked(glib::clone!(
        #[weak]
        path_row,
        #[weak]
        status,
        #[weak]
        toast_overlay,
        move |_| {
            let path = PathBuf::from(path_row.text().as_str());
            match inspect_vault(&path) {
                Ok(info) => show_vault_result(&status, &toast_overlay, "已打开（未解锁）", &info),
                Err(error) => show_error(&status, &toast_overlay, &error.to_string()),
            }
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
            match create_vault(&path, &password) {
                Ok(info) => {
                    *last_path.borrow_mut() = path;
                    show_vault_result(
                        &status,
                        &toast_overlay,
                        "已创建演示保险库（主密码已用于配置解锁方式）",
                        &info,
                    );
                }
                Err(error) => show_error(&status, &toast_overlay, &error.to_string()),
            }
        }
    ));

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build();
    scrolled.upcast()
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
    let text = format!(
        "{heading}\n路径：{}\nvault_id：{}\n格式：{}  schema：{}  Tiga：{}\n会话：{}",
        info.path.display(),
        info.vault_id,
        info.format_version,
        info.schema_version,
        info.tiga_mode,
        if info.unlocked {
            "已解锁"
        } else {
            "未解锁"
        }
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
