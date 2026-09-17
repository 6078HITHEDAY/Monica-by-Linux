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
use crate::i18n::{t, tf};
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{apply_status_page_icon, confirm_action, nested_header};

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
            .title(t("passwords.empty"))
            .description(t("passwords.empty_add"))
            .build();
        apply_status_page_icon(&empty, "dialog-password-symbolic");

        let list_stack = gtk::Stack::new();
        let list_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build();
        list_stack.add_named(&empty, Some("empty"));
        list_stack.add_named(&list_scroll, Some("list"));
        list_stack.set_visible_child_name("empty");

        let new_button = gtk::Button::from_icon_name("list-add-symbolic");
        new_button.set_tooltip_text(Some(t("passwords.new_tooltip").as_str()));
        new_button.add_css_class("flat");

        let list_header = nested_header();
        list_header.pack_end(&new_button);
        let list_toolbar = libadwaita::ToolbarView::new();
        list_toolbar.add_top_bar(&list_header);
        list_toolbar.set_content(Some(&list_stack));
        let list_page = libadwaita::NavigationPage::builder()
            .title(t("passwords.list_title"))
            .child(&list_toolbar)
            .build();

        let title = value_label(&t("common.select_item"));
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
            .label(t("passwords.show"))
            .css_classes(["pill", "flat"])
            .build();
        let copy_user = gtk::Button::builder()
            .label(t("passwords.copy_user"))
            .css_classes(["pill"])
            .build();
        let copy_secret = gtk::Button::builder()
            .label(t("passwords.copy_password"))
            .css_classes(["suggested-action", "pill"])
            .build();
        let copy_totp = gtk::Button::builder()
            .label(t("passwords.copy_otp"))
            .css_classes(["pill"])
            .build();
        let edit_button = gtk::Button::builder()
            .label(t("common.edit"))
            .css_classes(["pill"])
            .build();
        let delete_button = gtk::Button::builder()
            .label(t("common.delete"))
            .css_classes(["destructive-action", "pill"])
            .build();
        let archive_button = gtk::Button::builder()
            .label(t("common.archive"))
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
        detail_form.append(&field(&t("passwords.username"), &username));
        detail_form.append(&field(&t("passwords.url"), &url));
        detail_form.append(&field(&t("passwords.password"), &secret_row));
        detail_form.append(&field(&t("passwords.otp"), &totp_code));
        detail_form.append(&field(&t("common.notes"), &notes));
        detail_form.append(&actions);
        detail_form.append(
            &gtk::Label::builder()
                .label(tf(
                    "passwords.clipboard_hint",
                    &[
                        &crate::security::clipboard_clear_secs().to_string(),
                        &crate::security::auto_lock_secs().to_string(),
                    ],
                ))
                .wrap(true)
                .xalign(0.0)
                .css_classes(["caption", "dim-label"])
                .build(),
        );

        let detail_empty = libadwaita::StatusPage::builder()
            .title(t("common.select_item_short"))
            .description(t("passwords.detail_empty"))
            .build();
        apply_status_page_icon(&detail_empty, "view-reveal-symbolic");

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
        detail_toolbar.add_top_bar(&nested_header());
        detail_toolbar.set_content(Some(&detail_stack));
        let detail_page = libadwaita::NavigationPage::builder()
            .title(t("common.detail"))
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
                self.empty.set_title(&t("passwords.empty"));
                self.empty
                    .set_description(Some(t("passwords.empty_add").as_str()));
                self.reload(state, session, None);
            }
            None => {
                self.unlocked.set(false);
                self.new_button.set_sensitive(false);
                self.ids.borrow_mut().clear();
                while let Some(row) = self.list.row_at_index(0) {
                    self.list.remove(&row);
                }
                self.empty.set_title(&t("common.unlock_first"));
                self.empty
                    .set_description(Some(t("passwords.locked_desc").as_str()));
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
        self.reveal_button.set_label(&t("passwords.show"));
        self.title.set_label(&t("common.select_item"));
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
                    state.show_error(None, &t("passwords.no_username"));
                    return;
                }
                button.clipboard().set_text(&username);
                state
                    .toast
                    .add_toast(libadwaita::Toast::new(&t("passwords.copied_user")));
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
                copy_secret_with_timeout(button, &detail.password, &state);
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
                copy_secret_with_timeout(button, &secret_password(code.code), &state);
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
                    t("common.archiving"),
                    move || session.set_archived(&entry_id, true),
                    move |_| {
                        state_ok
                            .toast
                            .add_toast(libadwaita::Toast::new(&t("common.archived")));
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
            t("passwords.reading_list"),
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
                ("", "") => t("passwords.no_user"),
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
            t("passwords.reading_item"),
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
        self.reveal_button.set_label(&t("passwords.show"));
        let has_totp = !detail.totp_secret.expose_secret().trim().is_empty();
        self.copy_totp.set_visible(has_totp);
        self.totp_code.set_visible(has_totp);
        if has_totp {
            let code = totp_now(detail.totp_secret.expose_secret(), 30, 6);
            self.totp_code.set_label(&tf(
                "passwords.otp_remaining",
                &[&code.code, &code.remaining.to_string()],
            ));
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
        self.totp_code.set_label(&tf(
            "passwords.otp_remaining",
            &[&code.code, &code.remaining.to_string()],
        ));
    }

    fn toggle_reveal(&self) {
        let Some(detail) = self.selected.borrow().clone() else {
            return;
        };
        if self.revealed.get() {
            self.revealed.set(false);
            self.secret_label.set_label(&hidden_secret());
            self.reveal_button.set_label(&t("passwords.show"));
        } else {
            self.revealed.set(true);
            self.secret_label.set_label(detail.password.expose_secret());
            self.reveal_button.set_label(&t("passwords.hide"));
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
    confirm_action(
        state,
        &t("passwords.delete_q"),
        &t("passwords.delete_d"),
        &t("common.delete"),
        {
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
                    t("common.deleting"),
                    move || session.delete_password_entry(&entry_id),
                    move |()| {
                        state_ok
                            .toast
                            .add_toast(libadwaita::Toast::new(&t("passwords.deleted")));
                        if let Some(session) = state_ok.current_session() {
                            page_ok.reload(&state_ok, session, None);
                        }
                    },
                );
            }
        },
    );
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
            t("passwords.new_title")
        } else {
            t("passwords.edit_title")
        })
        .default_width(440)
        .default_height(520)
        .build();

    let title_row = libadwaita::EntryRow::builder()
        .title(t("common.title"))
        .build();
    let username_row = libadwaita::EntryRow::builder()
        .title(t("passwords.username"))
        .build();
    let url_row = libadwaita::EntryRow::builder()
        .title(t("passwords.url"))
        .build();
    let notes_row = libadwaita::EntryRow::builder()
        .title(t("common.notes"))
        .build();
    let totp_row = libadwaita::PasswordEntryRow::builder()
        .title(t("passwords.otp_secret"))
        .build();
    let password_row = libadwaita::PasswordEntryRow::builder()
        .title(t("passwords.list_title"))
        .build();
    let generate = gtk::Button::from_icon_name("view-refresh-symbolic");
    generate.set_tooltip_text(Some(t("passwords.generate_tip").as_str()));
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
        .title(t("passwords.form"))
        .description(t("passwords.form_desc"))
        .build();
    group.add(&title_row);
    group.add(&username_row);
    group.add(&url_row);
    group.add(&password_row);
    group.add(&totp_row);
    group.add(&notes_row);

    let save = gtk::Button::builder()
        .label(t("common.save"))
        .css_classes(["suggested-action", "pill"])
        .build();
    let cancel = gtk::Button::builder()
        .label(t("common.cancel"))
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
            state
                .toast
                .add_toast(libadwaita::Toast::new(&t("passwords.filled")));
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
                state.show_error(None, &t("common.need_title"));
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
                t("common.saving"),
                move || session.save_password_entry(&draft),
                move |saved| {
                    clear_ok();
                    editor_ok.close();
                    state_ok
                        .toast
                        .add_toast(libadwaita::Toast::new(&t("passwords.saved")));
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
