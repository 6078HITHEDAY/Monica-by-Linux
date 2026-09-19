use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::EditableExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{
    secret_password, VaultSession, WalletCardType, WalletDetail, WalletDocumentType, WalletDraft,
    WalletKind, WalletSummary,
};
use secrecy::ExposeSecret;

use crate::i18n::t;
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{
    build_split, combo_row, confirm_action, dash, editor_buttons, field, field_with_caption,
    fill_list, hidden_secret, labeled_entry, labeled_secret, locked_empty, present_editor,
    value_label, wallet_icon, SplitWorkspace,
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
    extra_caption: gtk::Label,
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
        let split = build_split(&t("nav.wallet"), &wallet_icon(), &t("wallet.empty"));
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
        split
            .detail_box
            .append(&field(&t("wallet.holder"), &holder));
        split
            .detail_box
            .append(&field(&t("wallet.number"), &number));
        let (extra_field, extra_caption) = field_with_caption(&t("wallet.bank"), &extra);
        split.detail_box.append(&extra_field);
        split
            .detail_box
            .append(&field(&t("wallet.expiry"), &expiry));
        split.detail_box.append(&actions);

        let page = Self {
            root: split.root.clone(),
            split,
            title,
            kind,
            holder,
            number,
            extra,
            extra_caption,
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
                copy_secret_with_timeout(button, &detail.number, &state);
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
                copy_secret_with_timeout(button, &detail.cvv, &state);
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
                confirm_action(
                    &state,
                    &t("wallet.delete_q"),
                    &t("wallet.delete_d"),
                    &t("common.delete"),
                    {
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
                                    state_ok
                                        .toast
                                        .add_toast(libadwaita::Toast::new(&t("common.deleted")));
                                    if let Some(session) = state_ok.current_session() {
                                        page_ok.reload(&state_ok, session, None);
                                    }
                                },
                            );
                        }
                    },
                );
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
        state.spawn_job(
            None,
            {
                let page = page.clone();
                move |busy| {
                    page.split
                        .new_button
                        .set_sensitive(!busy && page.unlocked.get())
                }
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
            WalletKind::Card => format!("{} · {}", t("wallet.card"), card_type_label(detail.card_type)),
            WalletKind::Document => format!(
                "{} · {}",
                t("wallet.document"),
                document_type_label(detail.document_type)
            ),
        };
        self.kind.set_label(&kind);
        self.holder.set_label(dash(&detail.holder));
        self.number
            .set_label(&monica_vault::mask_digits(detail.number.expose_secret()));
        self.extra.set_label(dash(&detail.extra));
        let extra_caption = match detail.kind {
            WalletKind::Card => t("wallet.bank"),
            WalletKind::Document => t("wallet.issuer"),
        };
        self.extra_caption.set_label(&extra_caption);
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
    let kind_row = combo_row(
        &t("common.type"),
        &[&t("wallet.card"), &t("wallet.document")],
        match existing.as_ref().map(|detail| detail.kind) {
            Some(WalletKind::Document) => 1,
            _ => 0,
        },
    );
    kind_row.set_sensitive(is_new);
    let card_type_row = combo_row(
        &t("wallet.card_type"),
        &[
            &t("wallet.debit"),
            &t("wallet.credit"),
            &t("wallet.prepaid"),
        ],
        existing
            .as_ref()
            .map(|detail| card_type_index(detail.card_type))
            .unwrap_or(0),
    );
    let document_type_row = combo_row(
        &t("wallet.doc_type"),
        &[
            &t("wallet.doc_id"),
            &t("wallet.doc_passport"),
            &t("wallet.doc_license"),
            &t("wallet.doc_ssn"),
            &t("wallet.doc_other"),
        ],
        existing
            .as_ref()
            .map(|detail| document_type_index(detail.document_type))
            .unwrap_or(0),
    );
    let (title_box, title_entry) = labeled_entry(&t("wallet.card_name"), &t("wallet.ph_card_name"));
    let (bank_box, bank_entry) = labeled_entry(&t("wallet.bank"), &t("wallet.ph_bank"));
    let (number_box, number_entry) = labeled_secret(&t("wallet.number"), &t("wallet.ph_number"));
    let (holder_box, holder_entry) = labeled_entry(&t("wallet.holder"), &t("wallet.ph_holder"));
    let (month_box, month_entry) = labeled_entry(&t("wallet.month"), &t("wallet.ph_month"));
    let (year_box, year_entry) = labeled_entry(&t("wallet.year"), &t("wallet.ph_year"));
    let (cvv_box, cvv_entry) = labeled_secret(&t("wallet.cvv"), &t("wallet.ph_cvv"));
    let (issuer_box, issuer_entry) = labeled_entry(&t("wallet.issuer"), &t("wallet.ph_issuer"));
    let (issued_box, issued_entry) = labeled_entry(&t("wallet.issued"), &t("wallet.ph_issued"));
    let (expiry_box, expiry_entry) = labeled_entry(&t("wallet.expiry"), &t("wallet.ph_expiry"));
    let (nationality_box, nationality_entry) =
        labeled_entry(&t("wallet.nationality"), &t("wallet.ph_nationality"));
    let (notes_box, notes_entry) = labeled_entry(&t("common.notes"), &t("wallet.ph_notes"));
    let dates = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    dates.append(&month_box);
    dates.append(&year_box);
    if let Some(detail) = &existing {
        title_entry.set_text(&detail.title);
        holder_entry.set_text(&detail.holder);
        number_entry.set_text(detail.number.expose_secret());
        bank_entry.set_text(&detail.extra);
        issuer_entry.set_text(&detail.extra);
        let (month, year) = split_card_expiry(&detail.expiry);
        month_entry.set_text(&month);
        year_entry.set_text(&year);
        expiry_entry.set_text(&detail.expiry);
        issued_entry.set_text(&detail.issued);
        nationality_entry.set_text(&detail.nationality);
        cvv_entry.set_text(detail.cvv.expose_secret());
        notes_entry.set_text(&detail.notes);
    }
    let apply_kind = {
        let card_type_row = card_type_row.clone();
        let document_type_row = document_type_row.clone();
        let title_box = title_box.clone();
        let bank_box = bank_box.clone();
        let holder_box = holder_box.clone();
        let dates = dates.clone();
        let cvv_box = cvv_box.clone();
        let issuer_box = issuer_box.clone();
        let issued_box = issued_box.clone();
        let expiry_box = expiry_box.clone();
        let nationality_box = nationality_box.clone();
        let title_entry = title_entry.clone();
        let holder_entry = holder_entry.clone();
        let number_entry = number_entry.clone();
        move |kind_selected: u32, document_selected: u32| {
            let document = kind_selected == 1;
            card_type_row.set_visible(!document);
            document_type_row.set_visible(document);
            bank_box.set_visible(!document);
            dates.set_visible(!document);
            cvv_box.set_visible(!document);
            issuer_box.set_visible(document);
            issued_box.set_visible(document);
            expiry_box.set_visible(document);
            let doc_type = document_type_from_index(document_selected);
            nationality_box.set_visible(document && doc_type == WalletDocumentType::Passport);
            if document {
                title_entry.set_placeholder_text(Some(t("wallet.ph_doc_title").as_str()));
                holder_entry.set_placeholder_text(Some(t("wallet.ph_doc_name").as_str()));
                number_entry.set_placeholder_text(Some(document_number_placeholder(doc_type).as_str()));
            } else {
                title_entry.set_placeholder_text(Some(t("wallet.ph_card_name").as_str()));
                holder_entry.set_placeholder_text(Some(t("wallet.ph_holder").as_str()));
                number_entry.set_placeholder_text(Some(t("wallet.ph_number").as_str()));
            }
            title_box.set_visible(true);
            holder_box.set_visible(true);
        }
    };
    apply_kind(kind_row.selected(), document_type_row.selected());
    kind_row.connect_selected_notify(glib::clone!(
        #[strong]
        apply_kind,
        #[weak]
        document_type_row,
        move |row| apply_kind(row.selected(), document_type_row.selected())
    ));
    document_type_row.connect_selected_notify(glib::clone!(
        #[strong]
        apply_kind,
        #[weak]
        kind_row,
        move |row| apply_kind(kind_row.selected(), row.selected())
    ));
    let group = libadwaita::PreferencesGroup::builder()
        .title(t("nav.wallet"))
        .build();
    group.add(&kind_row);
    group.add(&card_type_row);
    group.add(&document_type_row);
    let fields = gtk::Box::new(gtk::Orientation::Vertical, 10);
    fields.append(&title_box);
    fields.append(&bank_box);
    fields.append(&number_box);
    fields.append(&holder_box);
    fields.append(&dates);
    fields.append(&cvv_box);
    fields.append(&issuer_box);
    fields.append(&issued_box);
    fields.append(&expiry_box);
    fields.append(&nationality_box);
    fields.append(&notes_box);
    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&fields);
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
        card_type_row,
        #[weak]
        document_type_row,
        #[weak]
        title_entry,
        #[weak]
        holder_entry,
        #[weak]
        number_entry,
        #[weak]
        bank_entry,
        #[weak]
        issuer_entry,
        #[weak]
        month_entry,
        #[weak]
        year_entry,
        #[weak]
        expiry_entry,
        #[weak]
        issued_entry,
        #[weak]
        nationality_entry,
        #[weak]
        cvv_entry,
        #[weak]
        notes_entry,
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
            let extra = if kind == WalletKind::Document {
                issuer_entry.text().to_string()
            } else {
                bank_entry.text().to_string()
            };
            let expiry = if kind == WalletKind::Card {
                let month = month_entry.text().to_string();
                let year = year_entry.text().to_string();
                match (month.trim().is_empty(), year.trim().is_empty()) {
                    (true, true) => String::new(),
                    (false, true) => month,
                    (true, false) => year,
                    (false, false) => format!("{month}/{year}"),
                }
            } else {
                expiry_entry.text().to_string()
            };
            let cvv = if kind == WalletKind::Card {
                cvv_entry.text().to_string()
            } else {
                String::new()
            };
            let draft = WalletDraft {
                entry_id: entry_id.clone(),
                kind,
                title: title_entry.text().to_string(),
                holder: holder_entry.text().to_string(),
                number: secret_password(number_entry.text().to_string()),
                extra,
                expiry,
                issued: issued_entry.text().to_string(),
                nationality: nationality_entry.text().to_string(),
                card_type: card_type_from_index(card_type_row.selected()),
                document_type: document_type_from_index(document_type_row.selected()),
                cvv: secret_password(cvv),
                notes: notes_entry.text().to_string(),
            };
            number_entry.set_text("");
            cvv_entry.set_text("");
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
                    state_ok
                        .toast
                        .add_toast(libadwaita::Toast::new(&t("common.saved")));
                    if let Some(session) = state_ok.current_session() {
                        page_ok.reload(&state_ok, session, Some(saved.entry_id));
                    }
                },
            );
        }
    ));
    editor.present();
}

fn card_type_label(card_type: WalletCardType) -> String {
    match card_type {
        WalletCardType::Debit => t("wallet.debit"),
        WalletCardType::Credit => t("wallet.credit"),
        WalletCardType::Prepaid => t("wallet.prepaid"),
    }
}

fn document_type_label(document_type: WalletDocumentType) -> String {
    match document_type {
        WalletDocumentType::IdCard => t("wallet.doc_id"),
        WalletDocumentType::Passport => t("wallet.doc_passport"),
        WalletDocumentType::DriverLicense => t("wallet.doc_license"),
        WalletDocumentType::SocialSecurity => t("wallet.doc_ssn"),
        WalletDocumentType::Other => t("wallet.doc_other"),
    }
}

fn card_type_index(card_type: WalletCardType) -> u32 {
    match card_type {
        WalletCardType::Debit => 0,
        WalletCardType::Credit => 1,
        WalletCardType::Prepaid => 2,
    }
}

fn card_type_from_index(index: u32) -> WalletCardType {
    match index {
        1 => WalletCardType::Credit,
        2 => WalletCardType::Prepaid,
        _ => WalletCardType::Debit,
    }
}

fn document_type_index(document_type: WalletDocumentType) -> u32 {
    match document_type {
        WalletDocumentType::IdCard => 0,
        WalletDocumentType::Passport => 1,
        WalletDocumentType::DriverLicense => 2,
        WalletDocumentType::SocialSecurity => 3,
        WalletDocumentType::Other => 4,
    }
}

fn document_type_from_index(index: u32) -> WalletDocumentType {
    match index {
        1 => WalletDocumentType::Passport,
        2 => WalletDocumentType::DriverLicense,
        3 => WalletDocumentType::SocialSecurity,
        4 => WalletDocumentType::Other,
        _ => WalletDocumentType::IdCard,
    }
}

fn document_number_placeholder(document_type: WalletDocumentType) -> String {
    match document_type {
        WalletDocumentType::IdCard => t("wallet.ph_doc_id"),
        WalletDocumentType::Passport => t("wallet.ph_doc_passport"),
        WalletDocumentType::DriverLicense => t("wallet.ph_doc_license"),
        WalletDocumentType::SocialSecurity => t("wallet.ph_doc_ssn"),
        WalletDocumentType::Other => t("wallet.ph_doc_other"),
    }
}

fn split_card_expiry(expiry: &str) -> (String, String) {
    let cleaned = expiry.replace(' ', "");
    if let Some((month, year)) = cleaned.split_once(['/', '-']) {
        (month.to_string(), year.to_string())
    } else {
        (cleaned, String::new())
    }
}
