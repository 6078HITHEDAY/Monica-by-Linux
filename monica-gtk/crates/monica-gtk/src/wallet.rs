use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::EditableExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{
    expiry_parts, secret_password, CardType, DocumentType, VaultSession, WalletDetail, WalletDraft,
    WalletKind, WalletSummary,
};
use secrecy::ExposeSecret;

use crate::i18n::t;
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{
    build_split, combo_row, confirm_action, dash, editor_buttons, field, field_with_caption,
    fill_list, hidden_secret, locked_empty, present_editor, value_label, wallet_icon,
    SplitWorkspace,
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
    issued: gtk::Label,
    issued_field: gtk::Widget,
    nationality: gtk::Label,
    nationality_field: gtk::Widget,
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
        let issued = value_label("—");
        let nationality = value_label("—");
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
        let (issued_field, _) = field_with_caption(&t("wallet.issued"), &issued);
        let (nationality_field, _) = field_with_caption(&t("wallet.nationality"), &nationality);
        split.detail_box.append(&extra_field);
        split.detail_box.append(&issued_field);
        split
            .detail_box
            .append(&field(&t("wallet.expiry"), &expiry));
        split.detail_box.append(&nationality_field);
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
            issued,
            issued_field,
            nationality,
            nationality_field,
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
        self.issued.set_label("—");
        self.nationality.set_label("—");
        self.expiry.set_label("—");
        self.issued_field.set_visible(false);
        self.nationality_field.set_visible(false);
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
            WalletKind::Card => t("wallet.card"),
            WalletKind::Document => t("wallet.document"),
        };
        let subtype = match detail.kind {
            WalletKind::Card => card_type_label(detail.card_type),
            WalletKind::Document => document_type_label(detail.document_type),
        };
        self.kind.set_label(&format!("{kind} · {subtype}"));
        self.holder.set_label(dash(&detail.holder));
        self.number
            .set_label(&monica_vault::mask_digits(detail.number.expose_secret()));
        self.extra.set_label(dash(&detail.extra));
        let extra_caption = match detail.kind {
            WalletKind::Card => t("wallet.bank"),
            WalletKind::Document => t("wallet.issuer"),
        };
        self.extra_caption.set_label(&extra_caption);
        self.issued.set_label(dash(&detail.issued_date));
        self.issued_field
            .set_visible(detail.kind == WalletKind::Document);
        self.expiry.set_label(dash(&detail.expiry));
        self.nationality.set_label(dash(&detail.nationality));
        self.nationality_field.set_visible(
            detail.kind == WalletKind::Document && detail.document_type == DocumentType::Passport,
        );
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
            .map(|detail| detail.card_type.index())
            .unwrap_or(0),
    );
    let document_type_row = combo_row(
        &t("wallet.doc_type"),
        &[
            &t("wallet.id_card"),
            &t("wallet.passport"),
            &t("wallet.driver"),
            &t("wallet.ssn"),
            &t("wallet.other"),
        ],
        existing
            .as_ref()
            .map(|detail| detail.document_type.index())
            .unwrap_or(0),
    );
    let title = example_field(&t("wallet.card_name"), &t("wallet.card_name_ex"), false);
    let holder = example_field(&t("wallet.holder"), &t("wallet.holder_ex"), false);
    let number = example_field(&t("wallet.number"), &t("wallet.number_card_ex"), true);
    let extra = example_field(&t("wallet.bank"), &t("wallet.bank_ex"), false);
    let (month, year) = existing
        .as_ref()
        .map(|detail| expiry_parts(&detail.expiry))
        .unwrap_or_else(|| (String::new(), String::new()));
    let month_row = combo_row(&t("wallet.month"), &month_labels(), month_index(&month));
    let (year_labels, year_selected) = year_choices(&year);
    let year_refs: Vec<&str> = year_labels.iter().map(String::as_str).collect();
    let year_row = combo_row(&t("wallet.year"), &year_refs, year_selected);
    let issued = example_field(&t("wallet.issued"), &t("wallet.issued_ex"), false);
    let expiry = example_field(&t("wallet.expiry"), &t("wallet.doc_expiry_ex"), false);
    let nationality = example_field(&t("wallet.nationality"), &t("wallet.nationality_ex"), false);
    let cvv = example_field(&t("wallet.cvv"), &t("wallet.cvv_ex"), true);
    let notes_row = libadwaita::EntryRow::builder()
        .title(t("common.notes"))
        .build();
    if let Some(detail) = &existing {
        title.entry.set_text(&detail.title);
        holder.entry.set_text(&detail.holder);
        number.entry.set_text(detail.number.expose_secret());
        extra.entry.set_text(&detail.extra);
        issued.entry.set_text(&detail.issued_date);
        expiry.entry.set_text(&detail.expiry);
        nationality.entry.set_text(&detail.nationality);
        cvv.entry.set_text(detail.cvv.expose_secret());
        notes_row.set_text(&detail.notes);
    }
    let apply_kind = {
        let card_type_row = card_type_row.clone();
        let document_type_row = document_type_row.clone();
        let title = title.clone();
        let holder = holder.clone();
        let number = number.clone();
        let extra = extra.clone();
        let month_row = month_row.clone();
        let year_row = year_row.clone();
        let issued = issued.clone();
        let expiry = expiry.clone();
        let nationality = nationality.clone();
        let cvv = cvv.clone();
        let document_type_for_kind = document_type_row.clone();
        move |selected: u32| {
            let document = selected == 1;
            card_type_row.set_visible(!document);
            document_type_row.set_visible(document);
            month_row.set_visible(!document);
            year_row.set_visible(!document);
            cvv.row.set_visible(!document);
            issued.row.set_visible(document);
            expiry.row.set_visible(document);
            if document {
                relabel(&title, &t("common.title"), &t("wallet.doc_title_ex"));
                relabel(&holder, &t("wallet.full_name"), &t("wallet.full_name_ex"));
                relabel(&extra, &t("wallet.issuer"), &t("wallet.issuer_ex"));
                apply_document_type(&number, &nationality, document_type_for_kind.selected());
            } else {
                relabel(&title, &t("wallet.card_name"), &t("wallet.card_name_ex"));
                relabel(&holder, &t("wallet.holder"), &t("wallet.holder_ex"));
                relabel(&extra, &t("wallet.bank"), &t("wallet.bank_ex"));
                relabel(&number, &t("wallet.number"), &t("wallet.number_card_ex"));
                nationality.row.set_visible(false);
            }
        }
    };
    apply_kind(kind_row.selected());
    kind_row.connect_selected_notify(glib::clone!(
        #[strong]
        apply_kind,
        move |row| apply_kind(row.selected())
    ));
    document_type_row.connect_selected_notify(glib::clone!(
        #[strong]
        number,
        #[strong]
        nationality,
        #[weak]
        kind_row,
        move |row| {
            if kind_row.selected() == 1 {
                apply_document_type(&number, &nationality, row.selected());
            }
        }
    ));
    let group = libadwaita::PreferencesGroup::builder()
        .title(t("nav.wallet"))
        .build();
    group.add(&kind_row);
    group.add(&card_type_row);
    group.add(&document_type_row);
    group.add(&title.row);
    group.add(&holder.row);
    group.add(&number.row);
    group.add(&extra.row);
    group.add(&month_row);
    group.add(&year_row);
    group.add(&issued.row);
    group.add(&expiry.row);
    group.add(&nationality.row);
    group.add(&cvv.row);
    group.add(&notes_row);
    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&buttons);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&form)
        .build();
    let editor_title = if is_new {
        t("wallet.new")
    } else {
        t("wallet.edit")
    };
    let editor = present_editor(state, &editor_title, &scroll);
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
        #[strong]
        year_labels,
        #[strong]
        title,
        #[strong]
        holder,
        #[strong]
        number,
        #[strong]
        extra,
        #[strong]
        issued,
        #[strong]
        expiry,
        #[strong]
        nationality,
        #[strong]
        cvv,
        #[weak]
        kind_row,
        #[weak]
        card_type_row,
        #[weak]
        document_type_row,
        #[weak]
        month_row,
        #[weak]
        year_row,
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
            let expiry_text = if kind == WalletKind::Card {
                let months = month_labels();
                let month_idx = (month_row.selected() as usize).min(months.len() - 1);
                let year = year_labels
                    .get(year_row.selected() as usize)
                    .cloned()
                    .unwrap_or_else(|| "2030".to_string());
                format!("{}/{year}", months[month_idx])
            } else {
                expiry.entry.text().to_string()
            };
            let cvv_text = if kind == WalletKind::Card {
                cvv.entry.text().to_string()
            } else {
                String::new()
            };
            let draft = WalletDraft {
                entry_id: entry_id.clone(),
                kind,
                title: title.entry.text().to_string(),
                holder: holder.entry.text().to_string(),
                number: secret_password(number.entry.text().to_string()),
                extra: extra.entry.text().to_string(),
                expiry: expiry_text,
                cvv: secret_password(cvv_text),
                notes: notes_row.text().to_string(),
                card_type: CardType::from_index(card_type_row.selected()),
                document_type: DocumentType::from_index(document_type_row.selected()),
                issued_date: if kind == WalletKind::Document {
                    issued.entry.text().to_string()
                } else {
                    String::new()
                },
                nationality: if kind == WalletKind::Document {
                    nationality.entry.text().to_string()
                } else {
                    String::new()
                },
            };
            number.entry.set_text("");
            cvv.entry.set_text("");
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

#[derive(Clone)]
struct ExampleField {
    row: libadwaita::ActionRow,
    entry: gtk::Entry,
}

fn example_field(title: &str, example: &str, secret: bool) -> ExampleField {
    let entry = gtk::Entry::builder()
        .placeholder_text(example)
        .hexpand(true)
        .visibility(!secret)
        .build();
    if secret {
        entry.set_input_purpose(gtk::InputPurpose::Password);
    }
    let row = libadwaita::ActionRow::builder()
        .title(title)
        .subtitle(example)
        .build();
    row.add_suffix(&entry);
    row.set_activatable_widget(Some(&entry));
    ExampleField { row, entry }
}

fn relabel(field: &ExampleField, title: &str, example: &str) {
    field.row.set_title(title);
    field.row.set_subtitle(example);
    field.entry.set_placeholder_text(Some(example));
}

fn apply_document_type(number: &ExampleField, nationality: &ExampleField, selected: u32) {
    let doc = DocumentType::from_index(selected);
    let example = match doc {
        DocumentType::IdCard => t("wallet.number_id_ex"),
        DocumentType::Passport => t("wallet.number_passport_ex"),
        DocumentType::DriverLicense => t("wallet.number_driver_ex"),
        DocumentType::Ssn => t("wallet.number_ssn_ex"),
        DocumentType::Other => t("wallet.number_other_ex"),
    };
    relabel(number, &t("wallet.number"), &example);
    nationality.row.set_visible(doc == DocumentType::Passport);
}

fn card_type_label(card_type: CardType) -> String {
    match card_type {
        CardType::Debit => t("wallet.debit"),
        CardType::Credit => t("wallet.credit"),
        CardType::Prepaid => t("wallet.prepaid"),
    }
}

fn document_type_label(document_type: DocumentType) -> String {
    match document_type {
        DocumentType::IdCard => t("wallet.id_card"),
        DocumentType::Passport => t("wallet.passport"),
        DocumentType::DriverLicense => t("wallet.driver"),
        DocumentType::Ssn => t("wallet.ssn"),
        DocumentType::Other => t("wallet.other"),
    }
}

fn month_labels() -> [&'static str; 12] {
    [
        "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12",
    ]
}

fn month_index(month: &str) -> u32 {
    let digits: String = month.chars().filter(|ch| ch.is_ascii_digit()).collect();
    let parsed = digits.parse::<u32>().unwrap_or(1).clamp(1, 12);
    parsed - 1
}

fn year_choices(preferred: &str) -> (Vec<String>, u32) {
    let mut years: Vec<String> = (2024..=2050).map(|year| year.to_string()).collect();
    if !preferred.is_empty() && !years.iter().any(|year| year == preferred) {
        years.insert(0, preferred.to_string());
    }
    let selected = years
        .iter()
        .position(|year| year == preferred)
        .unwrap_or_else(|| years.iter().position(|year| year == "2030").unwrap_or(0))
        as u32;
    (years, selected)
}
