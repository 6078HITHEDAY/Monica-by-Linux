use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::desktop::{self, DesktopCmd, DesktopState, ShortcutRequest, APP_ID};
use crate::i18n::{t, tf};
use crate::pages::Pages;
use crate::prefs;
use crate::security::auto_lock_secs;
use crate::state::AppState;
use crate::unlock;
use crate::widgets::{sidebar_header, wallet_icon};

const NAV_ITEMS: [(&str, &str, &str); 12] = [
    ("passwords", "nav.passwords", "dialog-password-symbolic"),
    ("generator", "nav.generator", "view-refresh-symbolic"),
    ("otp", "nav.otp", "security-high-symbolic"),
    ("notes", "nav.notes", "text-x-generic-symbolic"),
    ("wallet", "nav.wallet", ""),
    ("timeline", "nav.timeline", "document-open-recent-symbolic"),
    ("recycle", "nav.recycle", "user-trash-symbolic"),
    ("archive", "nav.archive", "folder-symbolic"),
    ("backup", "nav.backup", "document-save-symbolic"),
    ("transfer", "nav.transfer", "document-save-as-symbolic"),
    ("workbench", "nav.workbench", "drive-harddisk-symbolic"),
    ("settings", "nav.settings", "emblem-system-symbolic"),
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
        .title(t("app.name"))
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
    let wallet_symbolic = wallet_icon();
    for (_id, title_key, icon) in NAV_ITEMS {
        let icon_name = if icon.is_empty() {
            wallet_symbolic.as_str()
        } else {
            icon
        };
        let row = libadwaita::ActionRow::builder()
            .title(t(title_key))
            .activatable(true)
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(icon_name));
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
    sidebar_toolbar.add_top_bar(&sidebar_header());
    sidebar_toolbar.set_content(Some(&sidebar_scroll));
    let sidebar_page = libadwaita::NavigationPage::builder()
        .title(t("nav.workspace"))
        .child(&sidebar_toolbar)
        .build();

    let toast_overlay = libadwaita::ToastOverlay::new();
    let content_header = libadwaita::HeaderBar::new();
    let lock_button = gtk::Button::builder()
        .label(t("nav.lock"))
        .css_classes(["pill"])
        .visible(false)
        .build();
    lock_button.set_tooltip_text(Some(t("nav.lock_tooltip").as_str()));
    content_header.pack_end(&lock_button);
    let settings_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_button.set_tooltip_text(Some(t("nav.settings_tooltip").as_str()));
    settings_button.add_css_class("flat");
    content_header.pack_end(&settings_button);

    let content_toolbar = libadwaita::ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);

    let (cmd_tx, cmd_rx) = mpsc::channel::<DesktopCmd>();
    let (shortcut_tx, shortcut_rx) = tokio::sync::mpsc::unbounded_channel::<ShortcutRequest>();
    let desktop = DesktopState::new(shortcut_tx, cmd_tx.clone());
    let window_stack = gtk::Stack::new();
    window_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    let state = AppState {
        application: application.clone(),
        window: window.clone(),
        toast: toast_overlay.clone(),
        window_stack: window_stack.clone(),
        stack: gtk::Stack::new(),
        content_page: libadwaita::NavigationPage::builder()
            .title(t("nav.passwords"))
            .build(),
        nav_list: list.clone(),
        nav_ids: Rc::new(NAV_ITEMS.iter().map(|(id, _, _)| *id).collect()),
        lock_button: lock_button.clone(),
        session: Rc::new(RefCell::new(None)),
        last_activity: Rc::new(Cell::new(Instant::now())),
        clipboard_generation: Rc::new(Cell::new(0)),
        unlock_status: Rc::new(RefCell::new(None)),
        vault_path: Rc::new(RefCell::new(unlock::initial_vault_path())),
        desktop,
        relock: Rc::new(RefCell::new(None)),
        sync_gate: Rc::new(RefCell::new(None)),
        editor_windows: Rc::new(RefCell::new(Vec::new())),
    };

    let pages = Pages::build(&state);
    install_desktop(&state, &pages, cmd_tx, cmd_rx, shortcut_rx);
    install_actions(&state, &pages);

    state
        .stack
        .set_transition_type(gtk::StackTransitionType::Crossfade);
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
    state.stack.set_visible_child_name("passwords");
    content_toolbar.set_content(Some(&state.stack));
    state.content_page.set_child(Some(&content_toolbar));

    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&state.content_page));

    let gate = unlock::build_unlock_gate(state.clone(), pages.clone());
    window_stack.add_named(&gate.root, Some("gate"));
    window_stack.add_named(&split_view, Some("shell"));
    window_stack.set_visible_child_name("gate");
    toast_overlay.set_child(Some(&window_stack));

    *state.sync_gate.borrow_mut() = Some(Rc::new({
        let gate = gate.clone();
        let state = state.clone();
        move || {
            gate.sync_from_state(&state);
        }
    }));
    *state.relock.borrow_mut() = Some(Rc::new({
        let state = state.clone();
        let pages = pages.clone();
        move || lock_now(&state, &pages, &t("lock.done"), LockReason::Manual)
    }));

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
                .unwrap_or("passwords");
            match name {
                "passwords" => pages.passwords.on_session_changed(&state),
                "otp" => pages.otp.on_session_changed(&state),
                "notes" => pages.notes.on_session_changed(&state),
                "wallet" => pages.wallet.on_session_changed(&state),
                "timeline" => pages.timeline.on_session_changed(&state),
                "recycle" => pages.recycle.on_session_changed(&state),
                "archive" => pages.archive.on_session_changed(&state),
                "workbench" => pages.workbench.on_session_changed(&state),
                "settings" => pages.settings.sync_from_state(&state),
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
            state.select_nav("settings");
        }
    ));

    lock_button.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        move |_| {
            lock_now(&state, &pages, &t("lock.done"), LockReason::Manual);
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
            if prefs::current().close_to_tray && state.desktop.tray_live() {
                state.window.set_visible(false);
                return glib::Propagation::Stop;
            }
            lock_now(&state, &pages, &t("lock.done"), LockReason::Close);
            quit_app(&state);
            glib::Propagation::Stop
        }
    ));

    window.set_content(Some(&toast_overlay));
    window.present();
}

#[derive(Clone, Copy)]
enum LockReason {
    Manual,
    AutoIdle,
    Close,
    Tray,
}

fn install_desktop(
    state: &AppState,
    pages: &Pages,
    cmd_tx: mpsc::Sender<DesktopCmd>,
    cmd_rx: mpsc::Receiver<DesktopCmd>,
    shortcut_rx: tokio::sync::mpsc::UnboundedReceiver<ShortcutRequest>,
) {
    crate::shortcuts::spawn(cmd_tx, shortcut_rx);
    pages.settings.sync_from_state(state);

    glib::spawn_future_local(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        async move {
            let caps = gio::spawn_blocking(desktop::probe)
                .await
                .unwrap_or_else(|_| desktop::Capabilities::no_bus(t("settings.probe_failed")));
            *state.desktop.caps.borrow_mut() = caps.clone();
            state
                .desktop
                .set_shortcut_status(caps.shortcut_probe_line());
            state.desktop.set_tray_status(caps.tray_probe_line());
            state.ensure_tray();
            pages.settings.sync_from_state(&state);
        }
    ));

    glib::timeout_add_local(Duration::from_millis(50), {
        let state = state.clone();
        let pages = pages.clone();
        move || {
            while let Ok(cmd) = cmd_rx.try_recv() {
                dispatch(&state, &pages, cmd);
            }
            glib::ControlFlow::Continue
        }
    });
}

fn install_actions(state: &AppState, pages: &Pages) {
    let show = gio::SimpleAction::new("show-window", None);
    show.connect_activate(glib::clone!(
        #[strong]
        state,
        move |_, _| {
            state.window.set_visible(true);
            state.window.present();
        }
    ));
    state.application.add_action(&show);

    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        move |_, _| {
            lock_now(&state, &pages, &t("lock.done"), LockReason::Manual);
            quit_app(&state);
        }
    ));
    state.application.add_action(&quit);
    state
        .application
        .set_accels_for_action("app.quit", &["<Primary>q"]);

    let lock = gio::SimpleAction::new("lock", None);
    lock.connect_activate(glib::clone!(
        #[strong]
        state,
        #[strong]
        pages,
        move |_, _| {
            lock_now(&state, &pages, &t("lock.done"), LockReason::Manual);
        }
    ));
    state.window.add_action(&lock);
    state
        .application
        .set_accels_for_action("win.lock", &["<Primary>l"]);
}

fn dispatch(state: &AppState, pages: &Pages, cmd: DesktopCmd) {
    match cmd {
        DesktopCmd::ToggleWindow => {
            if state.window.is_visible() {
                state.window.set_visible(false);
            } else {
                state.window.set_visible(true);
                state.window.present();
            }
        }
        DesktopCmd::ShowWindow => {
            state.window.set_visible(true);
            state.window.present();
        }
        DesktopCmd::HideWindow => state.window.set_visible(false),
        DesktopCmd::Lock => lock_now(state, pages, &t("lock.done"), LockReason::Tray),
        DesktopCmd::Quit => {
            lock_now(state, pages, &t("lock.done"), LockReason::Tray);
            quit_app(state);
        }
        DesktopCmd::ShortcutStatus(text) => {
            state.desktop.set_shortcut_status(text);
            pages.settings.sync_from_state(state);
        }
        DesktopCmd::TrayWatcher { online } => {
            state.desktop.tray_watcher_online.set(online);
            if online {
                state.desktop.set_tray_status(t("desktop.tray_connected"));
            } else {
                state
                    .desktop
                    .set_tray_status(tf("desktop.tray_offline", &[&t("desktop.gnome_tray_hint")]));
                if !state.window.is_visible() {
                    state.window.set_visible(true);
                    state.window.present();
                }
            }
            state.apply_close_behavior();
            pages.settings.sync_from_state(state);
        }
    }
}

fn quit_app(state: &AppState) {
    let _ = state.desktop.shortcut_tx.send(ShortcutRequest::Shutdown);
    if let Some(handle) = state.desktop.tray.borrow_mut().take() {
        let _ = handle.shutdown();
    }
    state.desktop.tray_watcher_online.set(false);
    drop(state.desktop.tray_hold.borrow_mut().take());
    state.application.quit();
}

fn lock_now(state: &AppState, pages: &Pages, toast: &str, reason: LockReason) {
    let was_unlocked = state.session.borrow().is_some();
    let closed = state.close_editors();
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
        status.set_label(&t("lock.status"));
    }
    state.show_gate();
    if was_unlocked {
        state.toast.add_toast(libadwaita::Toast::new(toast));
        if closed > 0 {
            state
                .toast
                .add_toast(libadwaita::Toast::new(&t("lock.discard_editors")));
        }
        if matches!(reason, LockReason::AutoIdle) {
            state.notify("auto-lock", &t("app.name"), &t("lock.idle"));
        }
    }
}

fn install_idle_lock(state: AppState, pages: Pages) {
    glib::timeout_add_seconds_local(5, move || {
        if state.current_session().is_none() {
            return glib::ControlFlow::Continue;
        }
        if state.last_activity.get().elapsed().as_secs() >= u64::from(auto_lock_secs()) {
            lock_now(&state, &pages, &t("lock.idle"), LockReason::AutoIdle);
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
