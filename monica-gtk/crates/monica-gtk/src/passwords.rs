use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{
    encode_authenticator_key, parse_totp_spec, secret_password, totp_from_input, OtpType,
    PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary, TotpAlgorithm, TotpSpec,
    VaultProject, VaultSession,
};
use secrecy::ExposeSecret;

use crate::generator::generate_default;
use crate::i18n::{t, tf};
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{
    apply_status_page_icon, combo_row, confirm_action, editor_buttons, nested_header, present_editor,
};

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
    folder_label: gtk::Label,
    new_button: gtk::Button,
    folder_dropdown: gtk::DropDown,
    folder_new: gtk::Button,
    detail_stack: gtk::Stack,
    split: libadwaita::NavigationSplitView,
    ids: Rc<RefCell<Vec<String>>>,
    selected: Rc<RefCell<Option<PasswordEntryDetail>>>,
    revealed: Rc<Cell<bool>>,
    unlocked: Rc<Cell<bool>>,
    detail_gen: Rc<Cell<u64>>,
    all_entries: Rc<RefCell<Vec<PasswordEntrySummary>>>,
    projects: Rc<RefCell<Vec<VaultProject>>>,
    folder_ids: Rc<RefCell<Vec<Option<String>>>>,
    filter_project: Rc<RefCell<Option<String>>>,
    updating_folders: Rc<Cell<bool>>,
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
        let folder_new = gtk::Button::from_icon_name("folder-new-symbolic");
        folder_new.set_tooltip_text(Some(t("folders.new").as_str()));
        folder_new.add_css_class("flat");
        folder_new.set_sensitive(false);

        let folder_dropdown = gtk::DropDown::from_strings(&[&t("folders.all")]);
        folder_dropdown.set_hexpand(true);
        let folder_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        folder_bar.set_margin_start(8);
        folder_bar.set_margin_end(8);
        folder_bar.set_margin_top(6);
        folder_bar.set_margin_bottom(6);
        folder_bar.append(&folder_dropdown);
        folder_bar.append(&folder_new);

        let list_header = nested_header();
        list_header.pack_end(&new_button);
        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.append(&folder_bar);
        sidebar_box.append(&list_stack);
        let list_toolbar = libadwaita::ToolbarView::new();
        list_toolbar.add_top_bar(&list_header);
        list_toolbar.set_content(Some(&sidebar_box));
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
        let folder_label = value_label("—");

        let detail_form = gtk::Box::new(gtk::Orientation::Vertical, 14);
        detail_form.set_margin_start(18);
        detail_form.set_margin_end(18);
        detail_form.set_margin_top(18);
        detail_form.set_margin_bottom(18);
        detail_form.append(&title);
        detail_form.append(&field(&t("folders.assign"), &folder_label));
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
            folder_label,
            new_button,
            folder_dropdown,
            folder_new,
            detail_stack,
            split,
            ids: Rc::new(RefCell::new(Vec::new())),
            selected: Rc::new(RefCell::new(None)),
            revealed: Rc::new(Cell::new(false)),
            unlocked: Rc::new(Cell::new(false)),
            detail_gen: Rc::new(Cell::new(0)),
            all_entries: Rc::new(RefCell::new(Vec::new())),
            projects: Rc::new(RefCell::new(Vec::new())),
            folder_ids: Rc::new(RefCell::new(vec![None])),
            filter_project: Rc::new(RefCell::new(None)),
            updating_folders: Rc::new(Cell::new(false)),
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
                self.folder_new.set_sensitive(true);
                self.folder_dropdown.set_sensitive(true);
                self.empty.set_title(&t("passwords.empty"));
                self.empty
                    .set_description(Some(t("passwords.empty_add").as_str()));
                self.reload(state, session, None);
            }
            None => {
                self.unlocked.set(false);
                self.new_button.set_sensitive(false);
                self.folder_new.set_sensitive(false);
                self.folder_dropdown.set_sensitive(false);
                self.ids.borrow_mut().clear();
                self.all_entries.borrow_mut().clear();
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
        self.folder_label.set_label("—");
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
        self.folder_new.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                open_folder_editor(&state, &page);
            }
        ));
        self.folder_dropdown.connect_selected_notify(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |dropdown| {
                if page.updating_folders.get() {
                    return;
                }
                state.touch();
                let index = dropdown.selected() as usize;
                *page.filter_project.borrow_mut() =
                    page.folder_ids.borrow().get(index).cloned().flatten();
                let entries = page.all_entries.borrow().clone();
                page.show_list(&state, &entries, None);
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
                let code = totp_from_input(detail.totp_secret.expose_secret());
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
            move || {
                let projects = session.list_projects()?;
                let entries = session.list_password_entries()?;
                Ok((projects, entries))
            },
            move |(projects, entries)| {
                page_ok.fill_folders(&projects);
                *page_ok.all_entries.borrow_mut() = entries.clone();
                page_ok.show_list(&state_ok, &entries, select_id.as_deref());
            },
        );
    }

    fn fill_folders(&self, projects: &[VaultProject]) {
        self.updating_folders.set(true);
        *self.projects.borrow_mut() = projects.to_vec();
        let model = gtk::StringList::new(&[&t("folders.all")]);
        let mut ids = vec![None];
        for project in projects {
            model.append(&project.title);
            ids.push(Some(project.project_id.clone()));
        }
        self.folder_dropdown.set_model(Some(&model));
        let current = self.filter_project.borrow().clone();
        let selected = current
            .as_ref()
            .and_then(|id| ids.iter().position(|item| item.as_deref() == Some(id.as_str())))
            .unwrap_or(0);
        *self.folder_ids.borrow_mut() = ids;
        self.folder_dropdown.set_selected(selected as u32);
        self.updating_folders.set(false);
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
        let filter = self.filter_project.borrow().clone();
        let visible: Vec<&PasswordEntrySummary> = entries
            .iter()
            .filter(|entry| match filter.as_deref() {
                None => true,
                Some(id) => entry.project_id == id,
            })
            .collect();
        if visible.is_empty() {
            self.list_stack.set_visible_child_name("empty");
            self.clear_sensitive();
            return;
        }
        self.list_stack.set_visible_child_name("list");
        let mut select_index = 0;
        for (index, entry) in visible.iter().enumerate() {
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
        let folder_name = self
            .projects
            .borrow()
            .iter()
            .find(|project| project.project_id == detail.project_id)
            .map(|project| project.title.clone())
            .unwrap_or_else(|| t("folders.assign"));
        self.folder_label.set_label(&folder_name);
        self.revealed.set(false);
        self.secret_label.set_label(&hidden_secret());
        self.reveal_button.set_label(&t("passwords.show"));
        let has_totp = !detail.totp_secret.expose_secret().trim().is_empty();
        self.copy_totp.set_visible(has_totp);
        self.totp_code.set_visible(has_totp);
        if has_totp {
            let code = totp_from_input(detail.totp_secret.expose_secret());
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
        let code = totp_from_input(detail.totp_secret.expose_secret());
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
        self.folder_new.set_sensitive(!busy && self.unlocked.get());
        self.folder_dropdown
            .set_sensitive(!busy && self.unlocked.get());
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
    let projects = page.projects.borrow().clone();
    let folder_titles: Vec<String> = projects.iter().map(|project| project.title.clone()).collect();
    let preferred_folder = existing
        .as_ref()
        .map(|detail| detail.project_id.clone())
        .or_else(|| page.filter_project.borrow().clone());
    let folder_selected = preferred_folder
        .as_ref()
        .and_then(|id| {
            projects
                .iter()
                .position(|project| project.project_id == *id)
        })
        .unwrap_or(0) as u32;
    let folder_refs: Vec<&str> = folder_titles.iter().map(String::as_str).collect();
    let folder_row = if folder_refs.is_empty() {
        None
    } else {
        Some(combo_row(&t("folders.assign"), &folder_refs, folder_selected))
    };

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
    let type_row = combo_row(
        &t("otp.otp_type"),
        &[
            &t("otp.type_totp"),
            &t("otp.type_hotp"),
            &t("otp.type_steam"),
        ],
        0,
    );
    let algorithm_row = combo_row(
        &t("otp.algorithm"),
        &[&t("otp.sha1"), &t("otp.sha256"), &t("otp.sha512")],
        0,
    );
    let period = libadwaita::SpinRow::builder()
        .title(t("otp.period"))
        .adjustment(&gtk::Adjustment::new(30.0, 10.0, 120.0, 1.0, 5.0, 0.0))
        .digits(0)
        .build();
    let digits = libadwaita::SpinRow::builder()
        .title(t("otp.digits"))
        .adjustment(&gtk::Adjustment::new(6.0, 4.0, 10.0, 1.0, 1.0, 0.0))
        .digits(0)
        .build();
    let counter = libadwaita::SpinRow::builder()
        .title(t("otp.counter"))
        .adjustment(&gtk::Adjustment::new(0.0, 0.0, 1_000_000.0, 1.0, 10.0, 0.0))
        .digits(0)
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
        let spec = parse_totp_spec(detail.totp_secret.expose_secret());
        totp_row.set_text(&spec.secret);
        type_row.set_selected(otp_type_index(spec.otp_type));
        algorithm_row.set_selected(algorithm_index(spec.algorithm));
        period.set_value(f64::from(spec.period));
        digits.set_value(f64::from(spec.digits));
        counter.set_value(spec.counter as f64);
    }

    let sync_type = {
        let period = period.clone();
        let digits = digits.clone();
        let counter = counter.clone();
        move |selected: u32| {
            let otp_type = otp_type_from_index(selected);
            period.set_visible(otp_type != OtpType::Hotp);
            counter.set_visible(otp_type == OtpType::Hotp);
            if otp_type == OtpType::Steam {
                digits.set_value(5.0);
            }
        }
    };
    sync_type(type_row.selected());
    type_row.connect_selected_notify(glib::clone!(
        #[strong]
        sync_type,
        move |row| sync_type(row.selected())
    ));

    let group = libadwaita::PreferencesGroup::builder()
        .title(t("passwords.form"))
        .description(t("passwords.form_desc"))
        .build();
    if let Some(folder_row) = &folder_row {
        group.add(folder_row);
    }
    group.add(&title_row);
    group.add(&username_row);
    group.add(&url_row);
    group.add(&password_row);
    group.add(&totp_row);
    group.add(&type_row);
    group.add(&algorithm_row);
    group.add(&period);
    group.add(&digits);
    group.add(&counter);
    group.add(&notes_row);

    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 18);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&buttons);

    let editor_title = if is_new {
        t("passwords.new_title")
    } else {
        t("passwords.edit_title")
    };
    let editor = present_editor(state, &editor_title, &form);

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
        #[strong]
        projects,
        #[strong]
        folder_row,
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
        #[weak]
        type_row,
        #[weak]
        algorithm_row,
        #[weak]
        period,
        #[weak]
        digits,
        #[weak]
        counter,
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
            let project_id = folder_row.as_ref().and_then(|row| {
                projects
                    .get(row.selected() as usize)
                    .map(|project| project.project_id.clone())
            });
            let parsed = parse_totp_spec(&totp_row.text());
            let totp_secret = if parsed.secret.trim().is_empty() {
                secret_password(String::new())
            } else {
                secret_password(encode_authenticator_key(&TotpSpec {
                    secret: parsed.secret,
                    issuer: parsed.issuer,
                    account: parsed.account,
                    period: period.value() as u32,
                    digits: digits.value() as u32,
                    algorithm: algorithm_from_index(algorithm_row.selected()),
                    otp_type: otp_type_from_index(type_row.selected()),
                    counter: counter.value() as u64,
                }))
            };
            let draft = PasswordEntryDraft {
                entry_id: entry_id.clone(),
                title,
                username: username_row.text().to_string(),
                url: url_row.text().to_string(),
                notes: notes_row.text().to_string(),
                password: secret_password(password_row.text().to_string()),
                totp_secret,
                project_id,
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

fn open_folder_editor(state: &AppState, page: &PasswordPage) {
    if state.current_session().is_none() {
        page.on_session_changed(state);
        return;
    }
    let name_row = libadwaita::EntryRow::builder()
        .title(t("folders.name"))
        .build();
    let group = libadwaita::PreferencesGroup::builder()
        .title(t("folders.new"))
        .build();
    group.add(&name_row);
    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&buttons);
    let editor = present_editor(state, &t("folders.new"), &form);
    cancel.connect_clicked(glib::clone!(
        #[strong]
        editor,
        move |_| editor.close()
    ));
    save.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        page,
        #[strong]
        editor,
        #[weak]
        name_row,
        move |_| {
            state.touch();
            let title = name_row.text().to_string();
            if title.trim().is_empty() {
                state.show_error(None, &t("folders.need_name"));
                return;
            }
            let Some(session) = state.current_session() else {
                page.on_session_changed(&state);
                return;
            };
            let state_ok = state.clone();
            let page_ok = page.clone();
            let editor_ok = editor.clone();
            state.spawn_job(
                None,
                |_| {},
                t("common.saving"),
                move || session.create_project(&title),
                move |created| {
                    editor_ok.close();
                    *page_ok.filter_project.borrow_mut() = Some(created.project_id.clone());
                    state_ok
                        .toast
                        .add_toast(libadwaita::Toast::new(&t("folders.created")));
                    if let Some(session) = state_ok.current_session() {
                        page_ok.reload(&state_ok, session, None);
                    }
                },
            );
        }
    ));
    editor.present();
}

fn otp_type_index(otp_type: OtpType) -> u32 {
    match otp_type {
        OtpType::Totp => 0,
        OtpType::Hotp => 1,
        OtpType::Steam => 2,
    }
}

fn otp_type_from_index(index: u32) -> OtpType {
    match index {
        1 => OtpType::Hotp,
        2 => OtpType::Steam,
        _ => OtpType::Totp,
    }
}

fn algorithm_index(algorithm: TotpAlgorithm) -> u32 {
    match algorithm {
        TotpAlgorithm::Sha1 => 0,
        TotpAlgorithm::Sha256 => 1,
        TotpAlgorithm::Sha512 => 2,
    }
}

fn algorithm_from_index(index: u32) -> TotpAlgorithm {
    match index {
        1 => TotpAlgorithm::Sha256,
        2 => TotpAlgorithm::Sha512,
        _ => TotpAlgorithm::Sha1,
    }
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
