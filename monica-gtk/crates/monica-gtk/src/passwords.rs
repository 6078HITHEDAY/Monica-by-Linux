use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{
    secret_password, totp_now, PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary,
    VaultSession,
};
use secrecy::ExposeSecret;

use crate::generator::generate_default;
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::confirm_action;

#[derive(Clone)]
pub struct PasswordPage {
    pub root: gtk::Widget,
    list: gtk::ListBox,
    empty: libadwaita::StatusPage,
    list_stack: gtk::Stack,
    title: gtk::Label,
    username: gtk::Label,
    url: gtk::Label,
    notes: gtk::Label,
    secret_label: gtk::Label,
    reveal_button: gtk::Button,
    copy_user: gtk::Button,
    copy_secret: gtk::Button,
    copy_totp: gtk::Button,
    edit_button: gtk::Button,
    delete_button: gtk::Button,
    archive_button: gtk::Button,
    totp_code: gtk::Label,
    new_button: gtk::Button,
    detail_stack: gtk::Stack,
    split: libadwaita::NavigationSplitView,
    ids: Rc<RefCell<Vec<String>>>,
    selected: Rc<RefCell<Option<PasswordEntryDetail>>>,
    revealed: Rc<Cell<bool>>,
    unlocked: Rc<Cell<bool>>,
    detail_gen: Rc<Cell<u64>>,
}

impl PasswordPage {
    pub fn build(state: &AppState) -> Self {
        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_vexpand(true);

        let empty = libadwaita::StatusPage::builder()
            .icon_name("dialog-password-symbolic")
            .title("还没有密码条目")
            .description(
                "解锁后可以新建登录项。列表显示标题、用户名和网址；密码只在详情中按需显示。",
            )
            .build();

        let list_stack = gtk::Stack::new();
        let list_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build();
        list_stack.add_named(&empty, Some("empty"));
        list_stack.add_named(&list_scroll, Some("list"));
        list_stack.set_visible_child_name("empty");

        let new_button = gtk::Button::from_icon_name("list-add-symbolic");
        new_button.set_tooltip_text(Some("新建条目"));
        new_button.add_css_class("flat");

        let list_header = libadwaita::HeaderBar::new();
        list_header.pack_end(&new_button);
        let list_toolbar = libadwaita::ToolbarView::new();
        list_toolbar.add_top_bar(&list_header);
        list_toolbar.set_content(Some(&list_stack));
        let list_page = libadwaita::NavigationPage::builder()
            .title("密码")
            .child(&list_toolbar)
            .build();

        let title = value_label("请选择一条目");
        title.add_css_class("title-2");
        let username = value_label("—");
        let url = value_label("—");
        let notes = value_label("—");
        let secret_label = gtk::Label::builder()
            .label(hidden_secret())
            .xalign(0.0)
            .selectable(false)
            .wrap(true)
            .css_classes(["monospace"])
            .build();

        let reveal_button = gtk::Button::builder()
            .label("显示")
            .css_classes(["pill", "flat"])
            .build();
        let copy_user = gtk::Button::builder()
            .label("复制用户名")
            .css_classes(["pill"])
            .build();
        let copy_secret = gtk::Button::builder()
            .label("复制密码")
            .css_classes(["suggested-action", "pill"])
            .build();
        let copy_totp = gtk::Button::builder()
            .label("复制口令")
            .css_classes(["pill"])
            .build();
        let edit_button = gtk::Button::builder()
            .label("编辑")
            .css_classes(["pill"])
            .build();
        let delete_button = gtk::Button::builder()
            .label("删除")
            .css_classes(["destructive-action", "pill"])
            .build();
        let archive_button = gtk::Button::builder()
            .label("归档")
            .css_classes(["pill", "flat"])
            .build();

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::Start);
        actions.append(&copy_user);
        actions.append(&copy_secret);
        actions.append(&copy_totp);
        actions.append(&edit_button);
        actions.append(&delete_button);
        actions.append(&archive_button);

        let secret_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        secret_row.append(&secret_label);
        secret_row.append(&reveal_button);

        let totp_code = gtk::Label::builder()
            .label("------")
            .xalign(0.0)
            .css_classes(["monospace", "title-3"])
            .build();

        let detail_form = gtk::Box::new(gtk::Orientation::Vertical, 14);
        detail_form.set_margin_start(18);
        detail_form.set_margin_end(18);
        detail_form.set_margin_top(18);
        detail_form.set_margin_bottom(18);
        detail_form.append(&title);
        detail_form.append(&field("用户名", &username));
        detail_form.append(&field("网址", &url));
        detail_form.append(&field("密码", &secret_row));
        detail_form.append(&field("动态口令", &totp_code));
        detail_form.append(&field("备注", &notes));
        detail_form.append(&actions);
        detail_form.append(
            &gtk::Label::builder()
                .label(format!(
                    "复制密码后 {} 秒清除剪贴板（GdkClipboard / 门户）。空闲 {} 秒自动锁定。",
                    crate::security::clipboard_clear_secs(),
                    crate::security::auto_lock_secs()
                ))
                .wrap(true)
                .xalign(0.0)
                .css_classes(["caption", "dim-label"])
                .build(),
        );

        let detail_empty = libadwaita::StatusPage::builder()
            .icon_name("view-reveal-symbolic")
            .title("选择一条目")
            .description("密码默认隐藏。点「显示」才会把明文放进界面，离开或锁定时立即清掉。")
            .build();

        let detail_stack = gtk::Stack::new();
        detail_stack.add_named(&detail_empty, Some("empty"));
        detail_stack.add_named(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(&detail_form)
                .build(),
            Some("detail"),
        );
        detail_stack.set_visible_child_name("empty");

        let detail_toolbar = libadwaita::ToolbarView::new();
        detail_toolbar.add_top_bar(&libadwaita::HeaderBar::new());
        detail_toolbar.set_content(Some(&detail_stack));
        let detail_page = libadwaita::NavigationPage::builder()
            .title("详情")
            .child(&detail_toolbar)
            .build();

        let split = libadwaita::NavigationSplitView::new();
        split.set_min_sidebar_width(240.0);
        split.set_sidebar_width_fraction(0.36);
        split.set_sidebar(Some(&list_page));
        split.set_content(Some(&detail_page));

        let page = Self {
            root: split.clone().upcast(),
            list,
            empty,
            list_stack,
            title,
            username,
            url,
            notes,
            secret_label,
            reveal_button,
            copy_user,
            copy_secret,
            copy_totp,
            edit_button,
            delete_button,
            archive_button,
            totp_code,
            new_button,
            detail_stack,
            split,
            ids: Rc::new(RefCell::new(Vec::new())),
            selected: Rc::new(RefCell::new(None)),
            revealed: Rc::new(Cell::new(false)),
            unlocked: Rc::new(Cell::new(false)),
            detail_gen: Rc::new(Cell::new(0)),
        };
        page.connect_signals(state);
        page.set_detail_sensitive(false);
        page.new_button.set_sensitive(false);
        glib::timeout_add_seconds_local(1, {
            let page = page.clone();
            move || {
                page.refresh_totp();
                glib::ControlFlow::Continue
            }
        });
        page
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.clear_sensitive();
        match state.current_session() {
            Some(session) => {
                self.unlocked.set(true);
                self.new_button.set_sensitive(true);
                self.empty.set_title("还没有密码条目");
                self.empty.set_description(Some("点右上角 + 新建登录项。"));
                self.reload(state, session, None);
            }
            None => {
                self.unlocked.set(false);
                self.new_button.set_sensitive(false);
                self.ids.borrow_mut().clear();
                while let Some(row) = self.list.row_at_index(0) {
                    self.list.remove(&row);
                }
                self.empty.set_title("请先解锁保险库");
                self.empty
                    .set_description(Some("在「解锁」页创建或打开保险库后，这里会列出登录项。"));
                self.list_stack.set_visible_child_name("empty");
                self.detail_stack.set_visible_child_name("empty");
                self.new_button.set_sensitive(false);
            }
        }
    }

    pub fn clear_sensitive(&self) {
        self.revealed.set(false);
        self.selected.borrow_mut().take();
        self.secret_label.set_label(&hidden_secret());
        self.reveal_button.set_label("显示");
        self.title.set_label("请选择一条目");
        self.username.set_label("—");
        self.url.set_label("—");
        self.notes.set_label("—");
        self.set_detail_sensitive(false);
        self.detail_stack.set_visible_child_name("empty");
    }

    fn connect_signals(&self, state: &AppState) {
        self.list.connect_row_selected(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_, row| {
                state.touch();
                let Some(row) = row else {
                    page.clear_sensitive();
                    return;
                };
                let index = row.index() as usize;
                let Some(entry_id) = page.ids.borrow().get(index).cloned() else {
                    return;
                };
                page.split.set_show_content(true);
                page.load_detail(&state, entry_id);
            }
        ));

        self.reveal_button.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                page.toggle_reveal();
            }
        ));

        self.copy_user.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |button| {
                state.touch();
                let username = page
                    .selected
                    .borrow()
                    .as_ref()
                    .map(|detail| detail.username.clone())
                    .unwrap_or_default();
                if username.is_empty() {
                    state.show_error(None, "没有可复制的用户名");
                    return;
                }
                button.clipboard().set_text(&username);
                state
                    .toast
                    .add_toast(libadwaita::Toast::new("已复制用户名"));
            }
        ));

        self.copy_secret.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |button| {
                state.touch();
                let Some(detail) = page.selected.borrow().clone() else {
                    return;
                };
                copy_secret_with_timeout(
                    button,
                    &detail.password,
                    &state,
                );
            }
        ));

        self.new_button.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                open_editor(&state, &page, None);
            }
        ));

        self.edit_button.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                let selected = page.selected.borrow().clone();
                open_editor(&state, &page, selected);
            }
        ));

        self.delete_button.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                confirm_delete(&state, &page);
            }
        ));

        self.copy_totp.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |button| {
                state.touch();
                let Some(detail) = page.selected.borrow().clone() else {
                    return;
                };
                let code = totp_now(detail.totp_secret.expose_secret(), 30, 6);
                copy_secret_with_timeout(
                    button,
                    &secret_password(code.code),
                    &state,
                );
            }
        ));

        self.archive_button.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                let Some(detail) = page.selected.borrow().clone() else {
                    return;
                };
                let Some(session) = state.current_session() else {
                    return;
                };
                let entry_id = detail.entry_id.clone();
                let state_ok = state.clone();
                let page_ok = page.clone();
                state.spawn_job(
                    None,
                    |_| {},
                    "正在归档…",
                    move || session.set_archived(&entry_id, true),
                    move |_| {
                        state_ok.toast.add_toast(libadwaita::Toast::new("已归档"));
                        if let Some(session) = state_ok.current_session() {
                            page_ok.reload(&state_ok, session, None);
                        }
                    },
                );
            }
        ));
    }

    fn reload(&self, state: &AppState, session: VaultSession, select_id: Option<String>) {
        let page = self.clone();
        let state = state.clone();
        self.set_list_busy(true);
        let page_ok = page.clone();
        let state_ok = state.clone();
        state.spawn_job(
            None,
            {
                let page = page.clone();
                move |busy| page.set_list_busy(busy)
            },
            "正在读取密码列表…",
            move || session.list_password_entries(),
            move |entries| {
                page_ok.show_list(&state_ok, &entries, select_id.as_deref());
            },
        );
    }

    fn show_list(
        &self,
        state: &AppState,
        entries: &[PasswordEntrySummary],
        select_id: Option<&str>,
    ) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        self.ids.borrow_mut().clear();
        if entries.is_empty() {
            self.list_stack.set_visible_child_name("empty");
            self.clear_sensitive();
            return;
        }
        self.list_stack.set_visible_child_name("list");
        let mut select_index = 0;
        for (index, entry) in entries.iter().enumerate() {
            let subtitle = match (entry.username.as_str(), entry.url.as_str()) {
                ("", "") => "无用户名".to_string(),
                (username, "") => username.to_string(),
                ("", url) => url.to_string(),
                (username, url) => format!("{username} · {url}"),
            };
            let row = libadwaita::ActionRow::builder()
                .title(&entry.title)
                .subtitle(&subtitle)
                .activatable(true)
                .build();
            self.list.append(&row);
            self.ids.borrow_mut().push(entry.entry_id.clone());
            if select_id == Some(entry.entry_id.as_str()) {
                select_index = index;
            }
        }
        if let Some(row) = self.list.row_at_index(select_index as i32) {
            self.list.select_row(Some(&row));
        }
        let _ = state;
    }

    fn load_detail(&self, state: &AppState, entry_id: String) {
        let Some(session) = state.current_session() else {
            self.on_session_changed(state);
            return;
        };
        self.clear_sensitive();
        let request = self.detail_gen.get().wrapping_add(1);
        self.detail_gen.set(request);
        let page = self.clone();
        let state = state.clone();
        state.spawn_job(
            None,
            {
                let page = page.clone();
                move |busy| page.set_detail_busy(busy)
            },
            "正在读取条目…",
            move || session.get_password_entry(&entry_id),
            move |detail| {
                if page.detail_gen.get() == request {
                    page.show_detail(detail);
                }
            },
        );
    }

    fn show_detail(&self, detail: PasswordEntryDetail) {
        self.title.set_label(&detail.title);
        self.username.set_label(if detail.username.is_empty() {
            "—"
        } else {
            &detail.username
        });
        self.url.set_label(if detail.url.is_empty() {
            "—"
        } else {
            &detail.url
        });
        self.notes.set_label(if detail.notes.is_empty() {
            "—"
        } else {
            &detail.notes
        });
        self.revealed.set(false);
        self.secret_label.set_label(&hidden_secret());
        self.reveal_button.set_label("显示");
        let has_totp = !detail.totp_secret.expose_secret().trim().is_empty();
        self.copy_totp.set_visible(has_totp);
        self.totp_code.set_visible(has_totp);
        if has_totp {
            let code = totp_now(detail.totp_secret.expose_secret(), 30, 6);
            self.totp_code
                .set_label(&format!("{}  ·  {} 秒", code.code, code.remaining));
        } else {
            self.totp_code.set_label("------");
        }
        self.set_detail_sensitive(true);
        self.detail_stack.set_visible_child_name("detail");
        *self.selected.borrow_mut() = Some(detail);
    }

    fn refresh_totp(&self) {
        let Some(detail) = self.selected.borrow().clone() else {
            return;
        };
        if detail.totp_secret.expose_secret().trim().is_empty() {
            return;
        }
        let code = totp_now(detail.totp_secret.expose_secret(), 30, 6);
        self.totp_code
            .set_label(&format!("{}  ·  {} 秒", code.code, code.remaining));
    }

    fn toggle_reveal(&self) {
        let Some(detail) = self.selected.borrow().clone() else {
            return;
        };
        if self.revealed.get() {
            self.revealed.set(false);
            self.secret_label.set_label(&hidden_secret());
            self.reveal_button.set_label("显示");
        } else {
            self.revealed.set(true);
            self.secret_label.set_label(detail.password.expose_secret());
            self.reveal_button.set_label("隐藏");
        }
    }

    fn set_list_busy(&self, busy: bool) {
        self.list.set_sensitive(!busy);
        self.new_button.set_sensitive(!busy && self.unlocked.get());
    }

    fn set_detail_busy(&self, busy: bool) {
        self.set_detail_sensitive(!busy && self.selected.borrow().is_some());
    }

    fn set_detail_sensitive(&self, sensitive: bool) {
        self.reveal_button.set_sensitive(sensitive);
        self.copy_user.set_sensitive(sensitive);
        self.copy_secret.set_sensitive(sensitive);
        self.copy_totp.set_sensitive(sensitive);
        self.edit_button.set_sensitive(sensitive);
        self.delete_button.set_sensitive(sensitive);
        self.archive_button.set_sensitive(sensitive);
    }
}

fn confirm_delete(state: &AppState, page: &PasswordPage) {
    let Some(detail) = page.selected.borrow().clone() else {
        return;
    };
    confirm_action(state, "删除此条目？", "将移入回收站。", "删除", {
        let state = state.clone();
        let page = page.clone();
        move || {
            let Some(session) = state.current_session() else {
                page.on_session_changed(&state);
                return;
            };
            let entry_id = detail.entry_id.clone();
            let state_ok = state.clone();
            let page_ok = page.clone();
            state.spawn_job(
                None,
                {
                    let page = page.clone();
                    move |busy| page.set_detail_busy(busy)
                },
                "正在删除…",
                move || session.delete_password_entry(&entry_id),
                move |()| {
                    state_ok
                        .toast
                        .add_toast(libadwaita::Toast::new("已删除条目"));
                    if let Some(session) = state_ok.current_session() {
                        page_ok.reload(&state_ok, session, None);
                    }
                },
            );
        }
    });
}

fn open_editor(state: &AppState, page: &PasswordPage, existing: Option<PasswordEntryDetail>) {
    if state.current_session().is_none() {
        page.on_session_changed(state);
        return;
    }
    let is_new = existing.is_none();
    let editor = libadwaita::Window::builder()
        .transient_for(&state.window)
        .modal(true)
        .title(if is_new {
            "新建条目"
        } else {
            "编辑条目"
        })
        .default_width(440)
        .default_height(520)
        .build();

    let title_row = libadwaita::EntryRow::builder().title("标题").build();
    let username_row = libadwaita::EntryRow::builder().title("用户名").build();
    let url_row = libadwaita::EntryRow::builder().title("网址").build();
    let notes_row = libadwaita::EntryRow::builder().title("备注").build();
    let totp_row = libadwaita::PasswordEntryRow::builder()
        .title("动态口令密钥")
        .build();
    let password_row = libadwaita::PasswordEntryRow::builder()
        .title("密码")
        .build();
    let generate = gtk::Button::from_icon_name("view-refresh-symbolic");
    generate.set_tooltip_text(Some("生成"));
    generate.add_css_class("flat");
    password_row.add_suffix(&generate);

    if let Some(detail) = &existing {
        title_row.set_text(&detail.title);
        username_row.set_text(&detail.username);
        url_row.set_text(&detail.url);
        notes_row.set_text(&detail.notes);
        password_row.set_text(detail.password.expose_secret());
        totp_row.set_text(detail.totp_secret.expose_secret());
    }

    let group = libadwaita::PreferencesGroup::builder()
        .title("登录信息")
        .description("密码仅在此对话框中短暂显示；关闭或保存后会从控件清掉。")
        .build();
    group.add(&title_row);
    group.add(&username_row);
    group.add(&url_row);
    group.add(&password_row);
    group.add(&totp_row);
    group.add(&notes_row);

    let save = gtk::Button::builder()
        .label("保存")
        .css_classes(["suggested-action", "pill"])
        .build();
    let cancel = gtk::Button::builder()
        .label("取消")
        .css_classes(["pill"])
        .build();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.append(&cancel);
    buttons.append(&save);

    let form = gtk::Box::new(gtk::Orientation::Vertical, 18);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&buttons);

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());
    toolbar.set_content(Some(&form));
    editor.set_content(Some(&toolbar));

    let clear_fields = {
        let title_row = title_row.clone();
        let username_row = username_row.clone();
        let url_row = url_row.clone();
        let notes_row = notes_row.clone();
        let password_row = password_row.clone();
        let totp_row = totp_row.clone();
        move || {
            title_row.set_text("");
            username_row.set_text("");
            url_row.set_text("");
            notes_row.set_text("");
            password_row.set_text("");
            totp_row.set_text("");
        }
    };

    cancel.connect_clicked(glib::clone!(
        #[strong]
        editor,
        #[strong]
        clear_fields,
        move |_| {
            clear_fields();
            editor.close();
        }
    ));

    generate.connect_clicked(glib::clone!(
        #[weak]
        password_row,
        #[strong]
        state,
        move |_| {
            state.touch();
            let generated = generate_default();
            password_row.set_text(generated.expose_secret());
            state.toast.add_toast(libadwaita::Toast::new("已填入"));
        }
    ));

    let entry_id = existing.map(|detail| detail.entry_id);
    let save_for_click = save.clone();
    let cancel_for_click = cancel.clone();
    save.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        page,
        #[strong]
        editor,
        #[strong]
        clear_fields,
        #[weak]
        title_row,
        #[weak]
        username_row,
        #[weak]
        url_row,
        #[weak]
        notes_row,
        #[weak]
        password_row,
        #[weak]
        totp_row,
        move |_| {
            state.touch();
            let title = title_row.text().to_string();
            if title.trim().is_empty() {
                state.show_error(None, "请填写标题");
                return;
            }
            let Some(session) = state.current_session() else {
                page.on_session_changed(&state);
                return;
            };
            let draft = PasswordEntryDraft {
                entry_id: entry_id.clone(),
                title,
                username: username_row.text().to_string(),
                url: url_row.text().to_string(),
                notes: notes_row.text().to_string(),
                password: secret_password(password_row.text().to_string()),
                totp_secret: secret_password(totp_row.text().to_string()),
                archived: false,
            };
            password_row.set_text("");
            totp_row.set_text("");
            let state_ok = state.clone();
            let page_ok = page.clone();
            let editor_ok = editor.clone();
            let clear_ok = clear_fields.clone();
            save_for_click.set_sensitive(false);
            cancel_for_click.set_sensitive(false);
            state.spawn_job(
                None,
                {
                    let save = save_for_click.clone();
                    let cancel = cancel_for_click.clone();
                    move |busy| {
                        save.set_sensitive(!busy);
                        cancel.set_sensitive(!busy);
                    }
                },
                "正在保存…",
                move || session.save_password_entry(&draft),
                move |saved| {
                    clear_ok();
                    editor_ok.close();
                    state_ok
                        .toast
                        .add_toast(libadwaita::Toast::new("已保存条目"));
                    if let Some(session) = state_ok.current_session() {
                        page_ok.reload(&state_ok, session, Some(saved.entry_id));
                    }
                },
            );
        }
    ));

    editor.connect_close_request(glib::clone!(
        #[strong]
        clear_fields,
        move |_| {
            clear_fields();
            glib::Propagation::Proceed
        }
    ));

    editor.present();
}

fn field(title: &str, child: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_.append(
        &gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build(),
    );
    box_.append(child);
    box_.upcast()
}

fn value_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["body"])
        .build()
}

fn hidden_secret() -> String {
    "••••••••".to_string()
}
