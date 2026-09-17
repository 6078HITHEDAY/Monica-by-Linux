use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::prelude::EditableExt;
use libadwaita::prelude::*;
use monica_vault::{
    secret_password, totp_now, TotpDetail, TotpDraft, TotpSource, TotpSummary, VaultSession,
};
use secrecy::ExposeSecret;

use crate::i18n::{t, tf};
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{
    build_split, confirm_action, dash, editor_buttons, field, locked_empty,
    present_editor, value_label, SplitWorkspace,
};

#[derive(Clone)]
struct TotpPick {
    id: String,
    source: TotpSource,
}

#[derive(Clone)]
pub struct OtpPage {
    pub root: gtk::Widget,
    split: SplitWorkspace,
    title: gtk::Label,
    issuer: gtk::Label,
    account: gtk::Label,
    code: gtk::Label,
    remaining: gtk::Label,
    progress: gtk::ProgressBar,
    copy: gtk::Button,
    edit: gtk::Button,
    delete: gtk::Button,
    ids: Rc<RefCell<Vec<TotpPick>>>,
    selected: Rc<RefCell<Option<TotpDetail>>>,
    unlocked: Rc<Cell<bool>>,
    detail_gen: Rc<Cell<u64>>,
}

impl OtpPage {
    pub fn build(state: &AppState) -> Self {
        let split = build_split(&t("nav.otp"), "channel-secure-symbolic", &t("otp.empty"));
        let title = value_label(&t("common.select_item"));
        title.add_css_class("title-2");
        let issuer = value_label("—");
        let account = value_label("—");
        let code = gtk::Label::builder()
            .label("------")
            .xalign(0.0)
            .css_classes(["title-1", "monospace"])
            .build();
        let remaining = value_label("—");
        let progress = gtk::ProgressBar::new();
        progress.set_fraction(0.0);
        let copy = gtk::Button::builder()
            .label(t("common.copy"))
            .css_classes(["suggested-action", "pill"])
            .build();
        let edit = gtk::Button::builder()
            .label(t("common.edit"))
            .css_classes(["pill"])
            .build();
        let delete = gtk::Button::builder()
            .label(t("common.delete"))
            .css_classes(["destructive-action", "pill"])
            .build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&copy);
        actions.append(&edit);
        actions.append(&delete);
        split.detail_box.append(&title);
        split.detail_box.append(&field(&t("otp.issuer"), &issuer));
        split.detail_box.append(&field(&t("otp.account"), &account));
        split.detail_box.append(&field(&t("otp.code"), &code));
        split.detail_box.append(&remaining);
        split.detail_box.append(&progress);
        split.detail_box.append(&actions);

        let page = Self {
            root: split.root.clone(),
            split,
            title,
            issuer,
            account,
            code,
            remaining,
            progress,
            copy,
            edit,
            delete,
            ids: Rc::new(RefCell::new(Vec::new())),
            selected: Rc::new(RefCell::new(None)),
            unlocked: Rc::new(Cell::new(false)),
            detail_gen: Rc::new(Cell::new(0)),
        };
        page.connect_signals(state);
        page.set_detail_sensitive(false);
        page.split.new_button.set_sensitive(false);
        glib::timeout_add_seconds_local(1, {
            let page = page.clone();
            move || {
                page.refresh_code();
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
                self.split.new_button.set_sensitive(true);
                self.split.empty.set_title(&t("otp.empty"));
                self.split
                    .empty
                    .set_description(Some(t("otp.empty_add").as_str()));
                self.reload(state, session, None);
            }
            None => {
                self.unlocked.set(false);
                self.split.new_button.set_sensitive(false);
                locked_empty(
                    &self.split.empty,
                    &self.split.list_stack,
                    &self.split.detail_stack,
                );
            }
        }
    }

    pub fn clear_sensitive(&self) {
        self.selected.borrow_mut().take();
        self.title.set_label(&t("common.select_item"));
        self.issuer.set_label("—");
        self.account.set_label("—");
        self.code.set_label("------");
        self.remaining.set_label("—");
        self.progress.set_fraction(0.0);
        self.set_detail_sensitive(false);
        self.split.detail_stack.set_visible_child_name("empty");
    }

    fn connect_signals(&self, state: &AppState) {
        self.split.list.connect_row_selected(glib::clone!(
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
                let Some(pick) = page.ids.borrow().get(row.index() as usize).cloned() else {
                    return;
                };
                page.split.split.set_show_content(true);
                page.load_detail(&state, pick);
            }
        ));
        self.split.new_button.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                open_editor(&state, &page, None);
            }
        ));
        self.edit.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                open_editor(&state, &page, page.selected.borrow().clone());
            }
        ));
        self.copy.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |button| {
                state.touch();
                let Some(detail) = page.selected.borrow().clone() else {
                    return;
                };
                let code = totp_now(
                    detail.secret.expose_secret(),
                    detail.period,
                    detail.digits,
                );
                copy_secret_with_timeout(
                    button,
                    &secret_password(code.code),
                    &state,
                );
            }
        ));
        self.delete.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                let Some(detail) = page.selected.borrow().clone() else {
                    return;
                };
                let message = if detail.source == TotpSource::Login {
                    t("otp.clear_login")
                } else {
                    t("otp.delete_q")
                };
                confirm_action(&state, &message, &t("otp.delete_d"), &t("common.delete"), {
                    let state = state.clone();
                    let page = page.clone();
                    move || {
                        let Some(session) = state.current_session() else {
                            return;
                        };
                        let entry_id = detail.entry_id.clone();
                        let source = detail.source;
                        let state_ok = state.clone();
                        let page_ok = page.clone();
                        state.spawn_job(
                            None,
                            |_| {},
                            t("common.deleting"),
                            move || session.delete_totp_entry(&entry_id, source),
                            move |()| {
                                state_ok.toast.add_toast(libadwaita::Toast::new(&t("common.deleted")));
                                if let Some(session) = state_ok.current_session() {
                                    page_ok.reload(&state_ok, session, None);
                                }
                            },
                        );
                    }
                });
            }
        ));
    }

    fn reload(&self, state: &AppState, session: VaultSession, select_id: Option<String>) {
        let page = self.clone();
        state.spawn_job(
            None,
            {
                let page = page.clone();
                move |busy| page.split.new_button.set_sensitive(!busy && page.unlocked.get())
            },
            t("otp.reading"),
            move || session.list_totp_entries(),
            move |entries| page.show_list(&entries, select_id.as_deref()),
        );
    }

    fn show_list(&self, entries: &[TotpSummary], select_id: Option<&str>) {
        while let Some(row) = self.split.list.row_at_index(0) {
            self.split.list.remove(&row);
        }
        self.ids.borrow_mut().clear();
        if entries.is_empty() {
            self.split.list_stack.set_visible_child_name("empty");
            self.clear_sensitive();
            return;
        }
        self.split.list_stack.set_visible_child_name("list");
        let mut select_index = 0;
        for (index, entry) in entries.iter().enumerate() {
            let source = match entry.source {
                TotpSource::Standalone => t("otp.standalone"),
                TotpSource::Login => t("otp.login"),
            };
            let subtitle = match (entry.issuer.as_str(), entry.account.as_str()) {
                ("", "") => source.to_string(),
                (issuer, "") => format!("{issuer} · {source}"),
                ("", account) => format!("{account} · {source}"),
                (issuer, account) => format!("{issuer} · {account} · {source}"),
            };
            let row = libadwaita::ActionRow::builder()
                .title(&entry.title)
                .subtitle(&subtitle)
                .activatable(true)
                .build();
            self.split.list.append(&row);
            self.ids.borrow_mut().push(TotpPick {
                id: entry.entry_id.clone(),
                source: entry.source,
            });
            if select_id == Some(entry.entry_id.as_str()) {
                select_index = index;
            }
        }
        if let Some(row) = self.split.list.row_at_index(select_index as i32) {
            self.split.list.select_row(Some(&row));
        }
    }

    fn load_detail(&self, state: &AppState, pick: TotpPick) {
        let Some(session) = state.current_session() else {
            self.on_session_changed(state);
            return;
        };
        self.clear_sensitive();
        let request = self.detail_gen.get().wrapping_add(1);
        self.detail_gen.set(request);
        let page = self.clone();
        state.spawn_job(
            None,
            |_| {},
            t("common.reading"),
            move || session.get_totp_entry(&pick.id, pick.source),
            move |detail| {
                if page.detail_gen.get() == request {
                    page.show_detail(detail);
                }
            },
        );
    }

    fn show_detail(&self, detail: TotpDetail) {
        self.title.set_label(&detail.title);
        self.issuer.set_label(dash(&detail.issuer));
        self.account.set_label(dash(&detail.account));
        self.set_detail_sensitive(true);
        self.split.detail_stack.set_visible_child_name("detail");
        *self.selected.borrow_mut() = Some(detail);
        self.refresh_code();
    }

    fn refresh_code(&self) {
        let Some(detail) = self.selected.borrow().clone() else {
            return;
        };
        let code = totp_now(
            detail.secret.expose_secret(),
            detail.period,
            detail.digits,
        );
        self.code.set_label(&code.code);
        self.remaining
            .set_label(&tf("otp.seconds", &[&code.remaining.to_string()]));
        let used = f64::from(code.period.saturating_sub(code.remaining)) / f64::from(code.period);
        self.progress.set_fraction(used.clamp(0.0, 1.0));
    }

    fn set_detail_sensitive(&self, sensitive: bool) {
        self.copy.set_sensitive(sensitive);
        self.edit.set_sensitive(sensitive);
        self.delete.set_sensitive(sensitive);
    }
}

fn open_editor(state: &AppState, page: &OtpPage, existing: Option<TotpDetail>) {
    let is_new = existing.is_none();
    let title_row = libadwaita::EntryRow::builder().title(t("common.title")).build();
    let issuer_row = libadwaita::EntryRow::builder().title(t("otp.issuer")).build();
    let account_row = libadwaita::EntryRow::builder().title(t("otp.account")).build();
    let secret_row = libadwaita::PasswordEntryRow::builder().title(t("otp.secret")).build();
    let period = libadwaita::SpinRow::builder()
        .title(t("otp.period"))
        .adjustment(&gtk::Adjustment::new(30.0, 10.0, 120.0, 1.0, 5.0, 0.0))
        .digits(0)
        .build();
    let digits = libadwaita::SpinRow::builder()
        .title(t("otp.digits"))
        .adjustment(&gtk::Adjustment::new(6.0, 6.0, 8.0, 1.0, 1.0, 0.0))
        .digits(0)
        .build();
    let source = existing
        .as_ref()
        .map(|detail| detail.source)
        .unwrap_or(TotpSource::Standalone);
    if let Some(detail) = &existing {
        title_row.set_text(&detail.title);
        issuer_row.set_text(&detail.issuer);
        account_row.set_text(&detail.account);
        secret_row.set_text(detail.secret.expose_secret());
        period.set_value(f64::from(detail.period));
        digits.set_value(f64::from(detail.digits));
    }
    let group = libadwaita::PreferencesGroup::builder()
        .title(t("nav.otp"))
        .description(t("otp.form_desc"))
        .build();
    group.add(&title_row);
    group.add(&issuer_row);
    group.add(&account_row);
    group.add(&secret_row);
    group.add(&period);
    group.add(&digits);
    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&buttons);
    let editor_title = if is_new {
        t("otp.new")
    } else {
        t("otp.edit")
    };
    let editor = present_editor(state, &editor_title, &form);
    let entry_id = existing.map(|detail| detail.entry_id);
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
        title_row,
        #[weak]
        issuer_row,
        #[weak]
        account_row,
        #[weak]
        secret_row,
        #[weak]
        period,
        #[weak]
        digits,
        move |_| {
            state.touch();
            let Some(session) = state.current_session() else {
                return;
            };
            let draft = TotpDraft {
                entry_id: entry_id.clone(),
                source,
                title: title_row.text().to_string(),
                issuer: issuer_row.text().to_string(),
                account: account_row.text().to_string(),
                secret: secret_password(secret_row.text().to_string()),
                period: period.value() as u32,
                digits: digits.value() as u32,
            };
            secret_row.set_text("");
            let state_ok = state.clone();
            let page_ok = page.clone();
            let editor_ok = editor.clone();
            state.spawn_job(
                None,
                |_| {},
                t("common.saving"),
                move || session.save_totp_entry(&draft),
                move |saved| {
                    editor_ok.close();
                    state_ok.toast.add_toast(libadwaita::Toast::new(&t("common.saved")));
                    if let Some(session) = state_ok.current_session() {
                        page_ok.reload(&state_ok, session, Some(saved.entry_id));
                    }
                },
            );
        }
    ));
    editor.present();
}
