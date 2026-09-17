use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::pages::Pages;
use crate::security::auto_lock_secs;
use crate::state::AppState;
use crate::unlock;

const APP_ID: &str = "com.monicapass.MonicaGtk";
const NAV_ITEMS: [(&str, &str, &str); 13] = [
    ("unlock", "解锁", "system-lock-screen-symbolic"),
    ("passwords", "密码库", "dialog-password-symbolic"),
    ("generator", "生成器", "view-refresh-symbolic"),
    ("otp", "动态口令", "security-high-symbolic"),
    ("notes", "安全笔记", "text-x-generic-symbolic"),
    ("wallet", "钱包", "emblem-documents-symbolic"),
    ("timeline", "时间线", "document-open-recent-symbolic"),
    ("recycle", "回收站", "user-trash-symbolic"),
    ("archive", "归档", "folder-symbolic"),
    ("backup", "备份同步", "document-save-symbolic"),
    ("transfer", "导入导出", "document-save-as-symbolic"),
    ("workbench", "工作台", "drive-harddisk-symbolic"),
    ("settings", "设置", "emblem-system-symbolic"),
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
        .default_width(1280)
        .default_height(800)
        .width_request(720)
        .height_request(480)
        .build();

    let split_view = libadwaita::NavigationSplitView::new();
    split_view.set_min_sidebar_width(220.0);
    split_view.set_sidebar_width_fraction(0.22);

    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);
    for (_id, title, icon) in NAV_ITEMS {
        let row = libadwaita::ActionRow::builder()
            .title(title)
            .activatable(true)
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(icon));
        list.append(&row);
    }
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }

    let sidebar_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .vexpand(true)
        .build();
    let sidebar_toolbar = libadwaita::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&libadwaita::HeaderBar::new());
    sidebar_toolbar.set_content(Some(&sidebar_scroll));
    let sidebar_page = libadwaita::NavigationPage::builder()
        .title("工作区")
        .child(&sidebar_toolbar)
        .build();

    let toast_overlay = libadwaita::ToastOverlay::new();
    let content_header = libadwaita::HeaderBar::new();
    let lock_button = gtk::Button::builder()
        .label("锁定")
        .css_classes(["pill"])
        .visible(false)
        .build();
    lock_button.set_tooltip_text(Some("锁定保险库并回到解锁页"));
    content_header.pack_end(&lock_button);
    let settings_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_button.set_tooltip_text(Some("设置"));
    settings_button.add_css_class("flat");
    content_header.pack_end(&settings_button);

    let content_toolbar = libadwaita::ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);

    let state = AppState {
        window: window.clone(),
        toast: toast_overlay.clone(),
        stack: gtk::Stack::new(),
        content_page: libadwaita::NavigationPage::builder().title("解锁").build(),
        nav_list: list.clone(),
        lock_button: lock_button.clone(),
        session: Rc::new(RefCell::new(None)),
        last_activity: Rc::new(Cell::new(Instant::now())),
        clipboard_generation: Rc::new(Cell::new(0)),
        unlock_status: Rc::new(RefCell::new(None)),
        vault_path: Rc::new(RefCell::new(unlock::default_vault_path())),
    };

    let pages = Pages::build(&state);
    state
        .stack
        .set_transition_type(gtk::StackTransitionType::Crossfade);
    state.stack.add_named(
        &unlock::build_unlock_page(state.clone(), pages.clone()),
        Some("unlock"),
    );
    state
        .stack
        .add_named(&pages.passwords.root, Some("passwords"));
    state
        .stack
        .add_named(&pages.generator.root, Some("generator"));
    state.stack.add_named(&pages.otp.root, Some("otp"));
    state.stack.add_named(&pages.notes.root, Some("notes"));
    state.stack.add_named(&pages.wallet.root, Some("wallet"));
    state
        .stack
        .add_named(&pages.timeline.root, Some("timeline"));
    state.stack.add_named(&pages.recycle.root, Some("recycle"));
    state.stack.add_named(&pages.archive.root, Some("archive"));
    state
        .stack
        .add_named(&pages.backup_sync.root, Some("backup"));
    state
        .stack
        .add_named(&pages.transfer.root, Some("transfer"));
    state
        .stack
        .add_named(&pages.workbench.root, Some("workbench"));
    state
        .stack
        .add_named(&pages.settings.root, Some("settings"));
    state.stack.set_visible_child_name("unlock");
    content_toolbar.set_content(Some(&state.stack));
    toast_overlay.set_child(Some(&content_toolbar));
    state.content_page.set_child(Some(&toast_overlay));

    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&state.content_page));

    let breakpoint = libadwaita::Breakpoint::new(libadwaita::BreakpointCondition::new_length(
        libadwaita::BreakpointConditionLengthType::MaxWidth,
        720.0,
        libadwaita::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);

    list.connect_row_selected(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        #[weak]
        split_view,
        move |_list, row| {
            state.touch();
            let Some(row) = row else {
                return;
            };
            let title = row
                .downcast_ref::<libadwaita::ActionRow>()
                .map(|action| action.title())
                .unwrap_or_else(|| "Monica".into());
            state.content_page.set_title(&title);
            let name = NAV_ITEMS
                .get(row.index() as usize)
                .map(|(id, _, _)| *id)
                .unwrap_or("unlock");
            match name {
                "passwords" => pages.passwords.on_session_changed(&state),
                "otp" => pages.otp.on_session_changed(&state),
                "notes" => pages.notes.on_session_changed(&state),
                "wallet" => pages.wallet.on_session_changed(&state),
                "timeline" => pages.timeline.on_session_changed(&state),
                "recycle" => pages.recycle.on_session_changed(&state),
                "archive" => pages.archive.on_session_changed(&state),
                "workbench" => pages.workbench.on_session_changed(&state),
                _ => {}
            }
            state.stack.set_visible_child_name(name);
            split_view.set_show_content(true);
        }
    ));

    settings_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        move |_| {
            state.touch();
            if let Some(row) = state.nav_list.row_at_index(12) {
                state.nav_list.select_row(Some(&row));
            }
        }
    ));

    lock_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        move |_| {
            lock_now(&state, &pages, "已锁定保险库");
        }
    ));

    install_idle_lock(state.clone(), pages.clone());
    install_activity_controllers(&window, state.last_activity.clone());

    window.connect_close_request(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        move |_| {
            lock_now(&state, &pages, "已锁定保险库");
            glib::Propagation::Proceed
        }
    ));

    window.set_content(Some(&split_view));
    window.present();
}

fn lock_now(state: &AppState, pages: &Pages, toast: &str) {
    let was_unlocked = state.session.borrow().is_some();
    state.replace_session(None);
    pages.clear_sensitive();
    pages.on_session_changed(state);
    state
        .clipboard_generation
        .set(state.clipboard_generation.get().wrapping_add(1));
    if let Some(display) = gtk::gdk::Display::default() {
        let _ = display
            .clipboard()
            .set_content(None::<&gtk::gdk::ContentProvider>);
    }
    if let Some(status) = state.unlock_status.borrow().as_ref() {
        status.set_label("保险库已锁定。请输入主密码重新解锁。");
    }
    if let Some(row) = state.nav_list.row_at_index(0) {
        state.nav_list.select_row(Some(&row));
    }
    state.stack.set_visible_child_name("unlock");
    state.content_page.set_title("解锁");
    if was_unlocked {
        state.toast.add_toast(libadwaita::Toast::new(toast));
    }
}

fn install_idle_lock(state: AppState, pages: Pages) {
    glib::timeout_add_seconds_local(5, move || {
        if state.current_session().is_none() {
            return glib::ControlFlow::Continue;
        }
        if state.last_activity.get().elapsed().as_secs() >= u64::from(auto_lock_secs()) {
            lock_now(&state, &pages, "空闲超时，已自动锁定");
        }
        glib::ControlFlow::Continue
    });
}

fn install_activity_controllers(
    window: &libadwaita::ApplicationWindow,
    last_activity: Rc<Cell<Instant>>,
) {
    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(glib::clone!(
        #[strong]
        last_activity,
        move |_, _, _| {
            last_activity.set(Instant::now());
        }
    ));
    window.add_controller(motion);

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong]
        last_activity,
        move |_, _, _, _| {
            last_activity.set(Instant::now());
            glib::Propagation::Proceed
        }
    ));
    window.add_controller(keys);

    let click = gtk::GestureClick::new();
    click.connect_pressed(glib::clone!(
        #[strong]
        last_activity,
        move |_, _, _, _| {
            last_activity.set(Instant::now());
        }
    ));
    window.add_controller(click);
}
