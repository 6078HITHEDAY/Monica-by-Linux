use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::passwords::PasswordPage;
use crate::security::auto_lock_secs;
use crate::state::AppState;
use crate::unlock;

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
        .default_width(1100)
        .default_height(680)
        .width_request(720)
        .height_request(480)
        .build();

    let split_view = libadwaita::NavigationSplitView::new();
    split_view.set_min_sidebar_width(220.0);
    split_view.set_sidebar_width_fraction(0.22);

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
    let content_header = libadwaita::HeaderBar::new();
    let lock_button = gtk::Button::builder()
        .label("锁定")
        .css_classes(["pill"])
        .visible(false)
        .build();
    lock_button.set_tooltip_text(Some("锁定保险库并回到解锁页"));
    content_header.pack_end(&lock_button);

    let content_toolbar = libadwaita::ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);

    let placeholder = libadwaita::StatusPage::builder()
        .icon_name("view-list-symbolic")
        .title("密码库")
        .description("Phase 2 将实现此工作区。")
        .build();

    let state = AppState {
        window: window.clone(),
        toast: toast_overlay.clone(),
        stack: gtk::Stack::new(),
        content_page: libadwaita::NavigationPage::builder().title("解锁").build(),
        nav_list: list.clone(),
        lock_button: lock_button.clone(),
        placeholder: placeholder.clone(),
        session: Rc::new(RefCell::new(None)),
        last_activity: Rc::new(Cell::new(Instant::now())),
        clipboard_generation: Rc::new(Cell::new(0)),
    };

    let passwords = PasswordPage::build(&state);
    state
        .stack
        .set_transition_type(gtk::StackTransitionType::Crossfade);
    state.stack.add_named(
        &unlock::build_unlock_page(state.clone(), passwords.clone()),
        Some("unlock"),
    );
    state.stack.add_named(&passwords.root, Some("passwords"));
    state.stack.add_named(&placeholder, Some("placeholder"));
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
        passwords,
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
            match row.index() {
                0 => state.stack.set_visible_child_name("unlock"),
                1 => {
                    state.stack.set_visible_child_name("passwords");
                    passwords.on_session_changed(&state);
                }
                _ => {
                    state.placeholder.set_title(&title);
                    state
                        .placeholder
                        .set_description(Some("Phase 2 将实现此工作区。"));
                    state.stack.set_visible_child_name("placeholder");
                }
            }
            split_view.set_show_content(true);
        }
    ));

    lock_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        passwords,
        move |_| {
            lock_now(&state, &passwords, "已锁定保险库");
        }
    ));

    install_idle_lock(state.clone(), passwords.clone());
    install_activity_controllers(&window, state.last_activity.clone());

    window.connect_close_request(glib::clone!(
        #[strong]
        state,
        #[strong]
        passwords,
        move |_| {
            lock_now(&state, &passwords, "已锁定保险库");
            glib::Propagation::Proceed
        }
    ));

    window.set_content(Some(&split_view));
    window.present();
}

fn lock_now(state: &AppState, passwords: &PasswordPage, toast: &str) {
    let was_unlocked = state.session.borrow().is_some();
    state.replace_session(None);
    passwords.on_session_changed(state);
    state
        .clipboard_generation
        .set(state.clipboard_generation.get().wrapping_add(1));
    if let Some(display) = gtk::gdk::Display::default() {
        let _ = display
            .clipboard()
            .set_content(None::<&gtk::gdk::ContentProvider>);
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

fn install_idle_lock(state: AppState, passwords: PasswordPage) {
    let seconds = auto_lock_secs();
    glib::timeout_add_seconds_local(5, move || {
        if state.current_session().is_none() {
            return glib::ControlFlow::Continue;
        }
        if state.last_activity.get().elapsed().as_secs() >= u64::from(seconds) {
            lock_now(&state, &passwords, "空闲超时，已自动锁定");
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
