use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::prelude::EditableExt;
use libadwaita::prelude::*;
use monica_vault::{
    secret_password, WalletDetail, WalletDraft, WalletKind, WalletSummary, VaultSession,
};
use secrecy::ExposeSecret;

use crate::i18n::t;
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{
    build_split, confirm_action, dash, editor_buttons, field, fill_list, hidden_secret,
    locked_empty, present_editor, value_label, SplitWorkspace,
};

#[derive(Clone)]
pub struct WalletPage {
    pub root: gtk::Widget,
    split: SplitWorkspace,
    title: gtk::Label,
    kind: gtk::Label,
    holder: gtk::Label,
    number: gtk::Label,
    extra: gtk::Label,
    expiry: gtk::Label,
    copy_number: gtk::Button,
    copy_cvv: gtk::Button,
    edit: gtk::Button,
    delete: gtk::Button,
    archive: gtk::Button,
    ids: Rc<RefCell<Vec<String>>>,
    selected: Rc<RefCell<Option<WalletDetail>>>,
    unlocked: Rc<Cell<bool>>,
    detail_gen: Rc<Cell<u64>>,
}

impl WalletPage {
    pub fn build(state: &AppState) -> Self {
        let split = build_split(&t("nav.wallet"), "emblem-documents-symbolic", &t("wallet.empty"));
        let title = value_label(&t("common.select_item"));
        title.add_css_class("title-2");
        let kind = value_label("—");
        let holder = value_label("—");
        let number = value_label(&hidden_secret());
        number.add_css_class("monospace");
        let extra = value_label("—");
        let expiry = value_label("—");
        let copy_number = gtk::Button::builder()
            .label(t("wallet.copy_number"))
            .css_classes(["suggested-action", "pill"])
            .build();
        let copy_cvv = gtk::Button::builder()
            .label(t("wallet.copy_cvv"))
            .css_classes(["pill"])
            .build();
        let edit = gtk::Button::builder()
            .label(t("common.edit"))
            .css_classes(["pill"])
            .build();
        let delete = gtk::Button::builder()
            .label(t("common.delete"))
            .css_classes(["destructive-action", "pill"])
            .build();
        let archive = gtk::Button::builder()
            .label(t("common.archive"))
            .css_classes(["pill", "flat"])
            .build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&copy_number);
        actions.append(&copy_cvv);
        actions.append(&edit);
        actions.append(&delete);
        actions.append(&archive);
        split.detail_box.append(&title);
        split.detail_box.append(&field(&t("common.type"), &kind));
        split.detail_box.append(&field(&t("wallet.holder"), &holder));
        split.detail_box.append(&field(&t("wallet.number"), &number));
        split.detail_box.append(&field(&t("wallet.extra"), &extra));
        split.detail_box.append(&field(&t("wallet.expiry"), &expiry));
        split.detail_box.append(&actions);

        let page = Self {
            root: split.root.clone(),
            split,
            title,
            kind,
            holder,
            number,
            extra,
            expiry,
            copy_number,
            copy_cvv,
            edit,
            delete,
            archive,
            ids: Rc::new(RefCell::new(Vec::new())),
            selected: Rc::new(RefCell::new(None)),
            unlocked: Rc::new(Cell::new(false)),
            detail_gen: Rc::new(Cell::new(0)),
        };
        page.connect_signals(state);
        page.set_detail_sensitive(false);
        page.split.new_button.set_sensitive(false);
        page
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.clear_sensitive();
        match state.current_session() {
            Some(session) => {
                self.unlocked.set(true);
                self.split.new_button.set_sensitive(true);
                self.split.empty.set_title(&t("wallet.empty"));
                self.split
                    .empty
                    .set_description(Some(t("wallet.empty_add").as_str()));
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
        self.kind.set_label("—");
        self.holder.set_label("—");
        self.number.set_label(&hidden_secret());
        self.extra.set_label("—");
        self.expiry.set_label("—");
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
                let Some(entry_id) = page.ids.borrow().get(row.index() as usize).cloned() else {
                    return;
                };
                page.split.split.set_show_content(true);
                page.load_detail(&state, entry_id);
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
        self.copy_number.connect_clicked(glib::clone!(
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
                    &detail.number,
                    &state,
                );
            }
        ));
        self.copy_cvv.connect_clicked(glib::clone!(
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
                    &detail.cvv,
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
                confirm_action(&state, &t("wallet.delete_q"), &t("wallet.delete_d"), &t("common.delete"), {
                    let state = state.clone();
                    let page = page.clone();
                    move || {
                        let Some(session) = state.current_session() else {
                            return;
                        };
                        let entry_id = detail.entry_id.clone();
                        let state_ok = state.clone();
                        let page_ok = page.clone();
                        state.spawn_job(
                            None,
                            |_| {},
                            t("common.deleting"),
                            move || session.delete_wallet(&entry_id),
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
        self.archive.connect_clicked(glib::clone!(
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
                        state_ok.toast.add_toast(libadwaita::Toast::new(&t("common.archived")));
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
        state.spawn_job(
            None,
            {
                let page = page.clone();
                move |busy| page.split.new_button.set_sensitive(!busy && page.unlocked.get())
            },
            t("wallet.reading"),
            move || session.list_wallet(),
            move |entries| page.show_list(&entries, select_id.as_deref()),
        );
    }

    fn show_list(&self, entries: &[WalletSummary], select_id: Option<&str>) {
        let count = fill_list(
            &self.split.list,
            &self.ids,
            entries.iter().map(|entry| {
                let kind = match entry.kind {
                    WalletKind::Card => t("wallet.card"),
                    WalletKind::Document => t("wallet.document"),
                };
                (
                    entry.entry_id.clone(),
                    entry.title.clone(),
                    format!("{kind} · {}", dash(&entry.subtitle)),
                )
            }),
        );
        if count == 0 {
            self.split.list_stack.set_visible_child_name("empty");
            self.clear_sensitive();
            return;
        }
        self.split.list_stack.set_visible_child_name("list");
        let select_index = entries
            .iter()
            .position(|entry| select_id == Some(entry.entry_id.as_str()))
            .unwrap_or(0);
        if let Some(row) = self.split.list.row_at_index(select_index as i32) {
            self.split.list.select_row(Some(&row));
        }
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
        state.spawn_job(
            None,
            |_| {},
            t("common.reading"),
            move || session.get_wallet(&entry_id),
            move |detail| {
                if page.detail_gen.get() == request {
                    page.show_detail(detail);
                }
            },
        );
    }

    fn show_detail(&self, detail: WalletDetail) {
        self.title.set_label(&detail.title);
        let kind = match detail.kind {
            WalletKind::Card => t("wallet.card"),
            WalletKind::Document => t("wallet.document"),
        };
        self.kind.set_label(&kind);
        self.holder.set_label(dash(&detail.holder));
        self.number
            .set_label(&monica_vault::mask_digits(detail.number.expose_secret()));
        self.extra.set_label(dash(&detail.extra));
        self.expiry.set_label(dash(&detail.expiry));
        self.copy_cvv.set_visible(detail.kind == WalletKind::Card);
        self.set_detail_sensitive(true);
        self.split.detail_stack.set_visible_child_name("detail");
        *self.selected.borrow_mut() = Some(detail);
    }

    fn set_detail_sensitive(&self, sensitive: bool) {
        self.copy_number.set_sensitive(sensitive);
        self.copy_cvv.set_sensitive(sensitive);
        self.edit.set_sensitive(sensitive);
        self.delete.set_sensitive(sensitive);
        self.archive.set_sensitive(sensitive);
    }
}

fn open_editor(state: &AppState, page: &WalletPage, existing: Option<WalletDetail>) {
    let is_new = existing.is_none();
    let card = t("wallet.card");
    let document = t("wallet.document");
    let model = gtk::StringList::new(&[&card, &document]);
    let kind_row = libadwaita::ComboRow::builder()
        .title(t("common.type"))
        .model(&model)
        .build();
    let title_row = libadwaita::EntryRow::builder().title(t("common.title")).build();
    let holder_row = libadwaita::EntryRow::builder().title(t("wallet.holder")).build();
    let number_row = libadwaita::PasswordEntryRow::builder().title(t("wallet.number")).build();
    let extra_row = libadwaita::EntryRow::builder()
        .title(t("wallet.extra"))
        .build();
    let expiry_row = libadwaita::EntryRow::builder()
        .title(t("wallet.expiry"))
        .build();
    let cvv_row = libadwaita::PasswordEntryRow::builder().title("CVV").build();
    let notes_row = libadwaita::EntryRow::builder().title(t("common.notes")).build();
    if let Some(detail) = &existing {
        kind_row.set_selected(match detail.kind {
            WalletKind::Card => 0,
            WalletKind::Document => 1,
        });
        kind_row.set_sensitive(false);
        title_row.set_text(&detail.title);
        holder_row.set_text(&detail.holder);
        number_row.set_text(detail.number.expose_secret());
        extra_row.set_text(&detail.extra);
        expiry_row.set_text(&detail.expiry);
        cvv_row.set_text(detail.cvv.expose_secret());
        notes_row.set_text(&detail.notes);
    }
    let group = libadwaita::PreferencesGroup::builder()
        .title(t("nav.wallet"))
        .build();
    group.add(&kind_row);
    group.add(&title_row);
    group.add(&holder_row);
    group.add(&number_row);
    group.add(&extra_row);
    group.add(&expiry_row);
    group.add(&cvv_row);
    group.add(&notes_row);
    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&buttons);
    let editor_title = if is_new {
        t("wallet.new")
    } else {
        t("wallet.edit")
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
        kind_row,
        #[weak]
        title_row,
        #[weak]
        holder_row,
        #[weak]
        number_row,
        #[weak]
        extra_row,
        #[weak]
        expiry_row,
        #[weak]
        cvv_row,
        #[weak]
        notes_row,
        move |_| {
            state.touch();
            let Some(session) = state.current_session() else {
                return;
            };
            let kind = if kind_row.selected() == 1 {
                WalletKind::Document
            } else {
                WalletKind::Card
            };
            let draft = WalletDraft {
                entry_id: entry_id.clone(),
                kind,
                title: title_row.text().to_string(),
                holder: holder_row.text().to_string(),
                number: secret_password(number_row.text().to_string()),
                extra: extra_row.text().to_string(),
                expiry: expiry_row.text().to_string(),
                cvv: secret_password(cvv_row.text().to_string()),
                notes: notes_row.text().to_string(),
            };
            number_row.set_text("");
            cvv_row.set_text("");
            let state_ok = state.clone();
            let page_ok = page.clone();
            let editor_ok = editor.clone();
            state.spawn_job(
                None,
                |_| {},
                t("common.saving"),
                move || session.save_wallet(&draft),
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
