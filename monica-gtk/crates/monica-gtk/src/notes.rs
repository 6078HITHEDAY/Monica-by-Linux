use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::prelude::EditableExt;
use libadwaita::prelude::*;
use monica_vault::{NoteDetail, NoteDraft, NoteSummary, VaultSession};

use crate::i18n::t;
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{
    build_split, confirm_action, dash, editor_buttons, field, fill_list, locked_empty,
    present_editor, short_time, value_label, SplitWorkspace,
};

#[derive(Clone)]
pub struct NotePage {
    pub root: gtk::Widget,
    split: SplitWorkspace,
    title: gtk::Label,
    preview: gtk::Label,
    tags: gtk::Label,
    content: gtk::Label,
    copy: gtk::Button,
    edit: gtk::Button,
    delete: gtk::Button,
    archive: gtk::Button,
    ids: Rc<RefCell<Vec<String>>>,
    selected: Rc<RefCell<Option<NoteDetail>>>,
    unlocked: Rc<Cell<bool>>,
    detail_gen: Rc<Cell<u64>>,
}

impl NotePage {
    pub fn build(state: &AppState) -> Self {
        let split = build_split(&t("nav.notes"), "text-x-generic-symbolic", &t("notes.empty"));
        let title = value_label(&t("common.select_item"));
        title.add_css_class("title-2");
        let preview = value_label("—");
        let tags = value_label("—");
        let content = value_label("—");
        content.set_selectable(true);
        let copy = gtk::Button::builder()
            .label(t("common.copy"))
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
        actions.append(&copy);
        actions.append(&edit);
        actions.append(&delete);
        actions.append(&archive);
        split.detail_box.append(&title);
        split.detail_box.append(&field(&t("notes.tags"), &tags));
        split.detail_box.append(&field(&t("notes.content"), &content));
        split.detail_box.append(&field(&t("notes.preview"), &preview));
        split.detail_box.append(&actions);

        let page = Self {
            root: split.root.clone(),
            split,
            title,
            preview,
            tags,
            content,
            copy,
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
                self.split.empty.set_title(&t("notes.empty"));
                self.split
                    .empty
                    .set_description(Some(t("notes.empty_add").as_str()));
                self.reload(state, session, None);
            }
            None => {
                self.unlocked.set(false);
                self.split.new_button.set_sensitive(false);
                self.ids.borrow_mut().clear();
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
        self.preview.set_label("—");
        self.tags.set_label("—");
        self.content.set_label("—");
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
                let selected = page.selected.borrow().clone();
                open_editor(&state, &page, selected);
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
                copy_secret_with_timeout(
                    button,
                    &monica_vault::secret_password(detail.content),
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
                confirm_action(&state, &t("notes.delete_q"), &t("notes.delete_d"), &t("common.delete"), {
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
                            move || session.delete_note(&entry_id),
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
        let state_ok = state.clone();
        state.spawn_job(
            None,
            {
                let page = page.clone();
                move |busy| page.split.new_button.set_sensitive(!busy && page.unlocked.get())
            },
            t("notes.reading"),
            move || session.list_notes(),
            move |entries| page.show_list(&state_ok, &entries, select_id.as_deref()),
        );
    }

    fn show_list(&self, _state: &AppState, entries: &[NoteSummary], select_id: Option<&str>) {
        let count = fill_list(
            &self.split.list,
            &self.ids,
            entries.iter().map(|entry| {
                (
                    entry.entry_id.clone(),
                    entry.title.clone(),
                    if entry.preview.is_empty() {
                        short_time(&entry.updated_at)
                    } else {
                        entry.preview.clone()
                    },
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
            move || session.get_note(&entry_id),
            move |detail| {
                if page.detail_gen.get() == request {
                    page.show_detail(detail);
                }
            },
        );
    }

    fn show_detail(&self, detail: NoteDetail) {
        self.title.set_label(&detail.title);
        self.tags.set_label(dash(&detail.tags));
        self.content.set_label(dash(&detail.content));
        let preview = if detail.markdown {
            "Markdown".to_string()
        } else {
            t("notes.plain")
        };
        self.preview.set_label(&preview);
        self.set_detail_sensitive(true);
        self.split.detail_stack.set_visible_child_name("detail");
        *self.selected.borrow_mut() = Some(detail);
    }

    fn set_detail_sensitive(&self, sensitive: bool) {
        self.copy.set_sensitive(sensitive);
        self.edit.set_sensitive(sensitive);
        self.delete.set_sensitive(sensitive);
        self.archive.set_sensitive(sensitive);
    }
}

fn open_editor(state: &AppState, page: &NotePage, existing: Option<NoteDetail>) {
    let is_new = existing.is_none();
    let title_row = libadwaita::EntryRow::builder().title(t("common.title")).build();
    let tags_row = libadwaita::EntryRow::builder().title(t("notes.tags")).build();
    let markdown = libadwaita::SwitchRow::builder().title("Markdown").build();
    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let view = gtk::TextView::builder()
        .buffer(&buffer)
        .wrap_mode(gtk::WrapMode::WordChar)
        .hexpand(true)
        .height_request(180)
        .css_classes(["card"])
        .build();
    if let Some(detail) = &existing {
        title_row.set_text(&detail.title);
        tags_row.set_text(&detail.tags);
        markdown.set_active(detail.markdown);
        buffer.set_text(&detail.content);
    }
    let group = libadwaita::PreferencesGroup::builder()
        .title(t("notes.form"))
        .build();
    group.add(&title_row);
    group.add(&tags_row);
    group.add(&markdown);
    let (buttons, save, cancel) = editor_buttons();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(18);
    form.set_margin_bottom(18);
    form.append(&group);
    form.append(&field(&t("notes.content"), &view));
    form.append(&buttons);
    let editor_title = if is_new {
        t("notes.new")
    } else {
        t("notes.edit")
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
        tags_row,
        #[weak]
        markdown,
        #[weak]
        buffer,
        move |_| {
            state.touch();
            let Some(session) = state.current_session() else {
                return;
            };
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let draft = NoteDraft {
                entry_id: entry_id.clone(),
                title: title_row.text().to_string(),
                content: buffer.text(&start, &end, false).to_string(),
                tags: tags_row.text().to_string(),
                markdown: markdown.is_active(),
            };
            let state_ok = state.clone();
            let page_ok = page.clone();
            let editor_ok = editor.clone();
            state.spawn_job(
                None,
                |_| {},
                t("common.saving"),
                move || session.save_note(&draft),
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
