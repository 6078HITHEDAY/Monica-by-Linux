use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{ArchivedItem, TrashItem, VaultSession};

use crate::i18n::t;
use crate::state::AppState;
use crate::widgets::{
    build_split, confirm_action, dash, field, fill_list, locked_empty, short_time, value_label,
    SplitWorkspace,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LifecycleKind {
    Trash,
    Archive,
}

#[derive(Clone)]
enum RowData {
    Trash(TrashItem),
    Archive(ArchivedItem),
}

#[derive(Clone)]
pub struct LifecyclePage {
    pub root: gtk::Widget,
    kind: LifecycleKind,
    split: SplitWorkspace,
    title: gtk::Label,
    kind_label: gtk::Label,
    time: gtk::Label,
    primary: gtk::Button,
    secondary: gtk::Button,
    ids: Rc<RefCell<Vec<String>>>,
    rows: Rc<RefCell<Vec<RowData>>>,
    unlocked: Rc<Cell<bool>>,
}

impl LifecyclePage {
    pub fn build(state: &AppState, kind: LifecycleKind) -> Self {
        let (list_title, icon, empty_title, primary_label) = match kind {
            LifecycleKind::Trash => (
                t("nav.recycle"),
                "user-trash-symbolic",
                t("recycle.empty"),
                t("recycle.restore"),
            ),
            LifecycleKind::Archive => (
                t("nav.archive"),
                "folder-symbolic",
                t("archive.empty"),
                t("archive.unarchive"),
            ),
        };
        let split = build_split(&list_title, icon, &empty_title);
        split.new_button.set_visible(false);
        let title = value_label(&t("common.select_item"));
        title.add_css_class("title-2");
        let kind_label = value_label("—");
        let time = value_label("—");
        let primary = gtk::Button::builder()
            .label(primary_label)
            .css_classes(["suggested-action", "pill"])
            .build();
        let secondary = gtk::Button::builder()
            .label(if kind == LifecycleKind::Trash {
                t("recycle.purge")
            } else {
                t("common.delete")
            })
            .css_classes(["destructive-action", "pill"])
            .build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&primary);
        actions.append(&secondary);
        split.detail_box.append(&title);
        split.detail_box.append(&field(&t("common.type"), &kind_label));
        split.detail_box.append(&field(&t("common.time"), &time));
        if kind == LifecycleKind::Trash {
            split.detail_box.append(
                &gtk::Label::builder()
                    .label(t("recycle.purge_blocked"))
                    .wrap(true)
                    .xalign(0.0)
                    .css_classes(["caption", "dim-label"])
                    .build(),
            );
        }
        split.detail_box.append(&actions);

        let page = Self {
            root: split.root.clone(),
            kind,
            split,
            title,
            kind_label,
            time,
            primary,
            secondary,
            ids: Rc::new(RefCell::new(Vec::new())),
            rows: Rc::new(RefCell::new(Vec::new())),
            unlocked: Rc::new(Cell::new(false)),
        };
        page.connect_signals(state);
        page.set_detail_sensitive(false);
        page
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.clear_sensitive();
        match state.current_session() {
            Some(session) => {
                self.unlocked.set(true);
                match self.kind {
                    LifecycleKind::Trash => {
                        self.split.empty.set_title(&t("recycle.empty"));
                        self.split
                            .empty
                            .set_description(Some(t("recycle.empty_desc").as_str()));
                    }
                    LifecycleKind::Archive => {
                        self.split.empty.set_title(&t("archive.empty"));
                        self.split
                            .empty
                            .set_description(Some(t("archive.empty_desc").as_str()));
                    }
                }
                self.reload(state, session);
            }
            None => {
                self.unlocked.set(false);
                locked_empty(
                    &self.split.empty,
                    &self.split.list_stack,
                    &self.split.detail_stack,
                );
            }
        }
    }

    pub fn clear_sensitive(&self) {
        self.title.set_label(&t("common.select_item"));
        self.kind_label.set_label("—");
        self.time.set_label("—");
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
                let Some(item) = page.rows.borrow().get(row.index() as usize).cloned() else {
                    return;
                };
                page.split.split.set_show_content(true);
                page.show_item(item);
            }
        ));
        self.primary.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                let Some(item) = page.selected_item() else {
                    return;
                };
                let Some(session) = state.current_session() else {
                    return;
                };
                let id = item_id(&item);
                let kind = page.kind;
                let state_ok = state.clone();
                let page_ok = page.clone();
                state.spawn_job(
                    None,
                    |_| {},
                    t("common.processing"),
                    move || match kind {
                        LifecycleKind::Trash => session.restore_entry(&id).map(|_| ()),
                        LifecycleKind::Archive => session.set_archived(&id, false).map(|_| ()),
                    },
                    move |()| {
                        let toast = match kind {
                            LifecycleKind::Trash => t("recycle.restored"),
                            LifecycleKind::Archive => t("archive.unarchived"),
                        };
                        state_ok.toast.add_toast(libadwaita::Toast::new(&toast));
                        if let Some(session) = state_ok.current_session() {
                            page_ok.reload(&state_ok, session);
                        }
                    },
                );
            }
        ));
        self.secondary.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            self,
            move |_| {
                state.touch();
                match page.kind {
                    LifecycleKind::Trash => {
                        state.toast.add_toast(libadwaita::Toast::new(
                            &t("recycle.purge_blocked"),
                        ));
                    }
                    LifecycleKind::Archive => {
                        let Some(item) = page.selected_item() else {
                            return;
                        };
                        confirm_action(&state, &t("wallet.delete_q"), &t("wallet.delete_d"), &t("common.delete"), {
                            let state = state.clone();
                            let page = page.clone();
                            move || {
                                let Some(session) = state.current_session() else {
                                    return;
                                };
                                let id = item_id(&item);
                                let state_ok = state.clone();
                                let page_ok = page.clone();
                                state.spawn_job(
                                    None,
                                    |_| {},
                                    t("common.deleting"),
                                    move || session.delete_entry(&id),
                                    move |()| {
                                        state_ok.toast.add_toast(libadwaita::Toast::new(&t("common.deleted")));
                                        if let Some(session) = state_ok.current_session() {
                                            page_ok.reload(&state_ok, session);
                                        }
                                    },
                                );
                            }
                        });
                    }
                }
            }
        ));
    }

    fn reload(&self, state: &AppState, session: VaultSession) {
        let page = self.clone();
        match self.kind {
            LifecycleKind::Trash => state.spawn_job(
                None,
                |_| {},
                t("recycle.reading"),
                move || session.list_trash(),
                move |items| page.show_trash(&items),
            ),
            LifecycleKind::Archive => state.spawn_job(
                None,
                |_| {},
                t("archive.reading"),
                move || session.list_archived(),
                move |items| page.show_archive(&items),
            ),
        }
    }

    fn show_trash(&self, items: &[TrashItem]) {
        self.rows
            .borrow_mut()
            .splice(0.., items.iter().cloned().map(RowData::Trash));
        let count = fill_list(
            &self.split.list,
            &self.ids,
            items.iter().map(|item| {
                (
                    item.entry_id.clone(),
                    item.title.clone(),
                    format!("{} · {}", item.kind_label, short_time(&item.deleted_at)),
                )
            }),
        );
        self.after_list(count);
    }

    fn show_archive(&self, items: &[ArchivedItem]) {
        self.rows
            .borrow_mut()
            .splice(0.., items.iter().cloned().map(RowData::Archive));
        let count = fill_list(
            &self.split.list,
            &self.ids,
            items.iter().map(|item| {
                (
                    item.entry_id.clone(),
                    item.title.clone(),
                    format!("{} · {}", item.kind_label, short_time(&item.archived_at)),
                )
            }),
        );
        self.after_list(count);
    }

    fn after_list(&self, count: usize) {
        if count == 0 {
            self.split.list_stack.set_visible_child_name("empty");
            self.clear_sensitive();
        } else {
            self.split.list_stack.set_visible_child_name("list");
            if let Some(row) = self.split.list.row_at_index(0) {
                self.split.list.select_row(Some(&row));
            }
        }
    }

    fn show_item(&self, item: RowData) {
        match &item {
            RowData::Trash(item) => {
                self.title.set_label(&item.title);
                self.kind_label.set_label(&item.kind_label);
                self.time.set_label(&short_time(&item.deleted_at));
            }
            RowData::Archive(item) => {
                self.title.set_label(&item.title);
                self.kind_label.set_label(&item.kind_label);
                self.time.set_label(&short_time(dash(&item.archived_at)));
            }
        }
        self.set_detail_sensitive(true);
        self.split.detail_stack.set_visible_child_name("detail");
    }

    fn selected_item(&self) -> Option<RowData> {
        let row = self.split.list.selected_row()?;
        self.rows.borrow().get(row.index() as usize).cloned()
    }

    fn set_detail_sensitive(&self, sensitive: bool) {
        self.primary.set_sensitive(sensitive);
        self.secondary.set_sensitive(sensitive);
    }
}

fn item_id(item: &RowData) -> String {
    match item {
        RowData::Trash(item) => item.entry_id.clone(),
        RowData::Archive(item) => item.entry_id.clone(),
    }
}
