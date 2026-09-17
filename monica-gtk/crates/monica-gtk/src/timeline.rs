use std::cell::Cell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{TimelineItem, VaultSession};

use crate::i18n::t;
use crate::state::AppState;
use crate::widgets::{dash, short_time, value_label};

#[derive(Clone)]
pub struct TimelinePage {
    pub root: gtk::Widget,
    list: gtk::ListBox,
    empty: libadwaita::StatusPage,
    stack: gtk::Stack,
    title: gtk::Label,
    summary: gtk::Label,
    meta: gtk::Label,
    ids: Rc<std::cell::RefCell<Vec<TimelineItem>>>,
    unlocked: Rc<Cell<bool>>,
}

impl TimelinePage {
    pub fn build(state: &AppState) -> Self {
        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk::SelectionMode::Single);
        let empty = libadwaita::StatusPage::builder()
            .icon_name("document-open-recent-symbolic")
            .title(t("timeline.empty"))
            .description(t("timeline.empty_desc"))
            .build();
        let stack = gtk::Stack::new();
        stack.add_named(&empty, Some("empty"));
        stack.add_named(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(&list)
                .build(),
            Some("list"),
        );
        stack.set_visible_child_name("empty");

        let list_toolbar = libadwaita::ToolbarView::new();
        list_toolbar.add_top_bar(&libadwaita::HeaderBar::new());
        list_toolbar.set_content(Some(&stack));
        let list_page = libadwaita::NavigationPage::builder()
            .title(t("nav.timeline"))
            .child(&list_toolbar)
            .build();

        let title = value_label(&t("timeline.pick"));
        title.add_css_class("title-2");
        let summary = value_label("—");
        let meta = value_label("—");
        let detail = gtk::Box::new(gtk::Orientation::Vertical, 12);
        detail.set_margin_start(18);
        detail.set_margin_end(18);
        detail.set_margin_top(18);
        detail.append(&title);
        detail.append(&summary);
        detail.append(&meta);
        let detail_toolbar = libadwaita::ToolbarView::new();
        detail_toolbar.add_top_bar(&libadwaita::HeaderBar::new());
        detail_toolbar.set_content(Some(&detail));
        let detail_page = libadwaita::NavigationPage::builder()
            .title(t("common.detail"))
            .child(&detail_toolbar)
            .build();

        let split = libadwaita::NavigationSplitView::new();
        split.set_min_sidebar_width(260.0);
        split.set_sidebar(Some(&list_page));
        split.set_content(Some(&detail_page));

        let page = Self {
            root: split.upcast(),
            list,
            empty,
            stack,
            title,
            summary,
            meta,
            ids: Rc::new(std::cell::RefCell::new(Vec::new())),
            unlocked: Rc::new(Cell::new(false)),
        };
        page.list.connect_row_selected(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_, row| {
                state.touch();
                let Some(row) = row else {
                    return;
                };
                let Some(item) = page.ids.borrow().get(row.index() as usize).cloned() else {
                    return;
                };
                page.title.set_label(&item.kind);
                page.summary.set_label(dash(&item.summary));
                let message = item.message.as_deref().unwrap_or("");
                page.meta.set_label(&format!(
                    "{} · {}\n{}",
                    short_time(&item.created_at),
                    item.scope,
                    dash(message)
                ));
            }
        ));
        page
    }

    pub fn on_session_changed(&self, state: &AppState) {
        match state.current_session() {
            Some(session) => {
                self.unlocked.set(true);
                self.reload(state, session);
            }
            None => {
                self.unlocked.set(false);
                self.ids.borrow_mut().clear();
                while let Some(row) = self.list.row_at_index(0) {
                    self.list.remove(&row);
                }
                self.empty.set_title(&t("common.unlock_vault_first"));
                self.empty
                    .set_description(Some(t("timeline.locked_desc").as_str()));
                self.stack.set_visible_child_name("empty");
                self.title.set_label(&t("timeline.pick"));
                self.summary.set_label("—");
                self.meta.set_label("—");
            }
        }
    }

    pub fn clear_sensitive(&self) {}

    fn reload(&self, state: &AppState, session: VaultSession) {
        let page = self.clone();
        state.spawn_job(
            None,
            |_| {},
            t("timeline.reading"),
            move || session.list_timeline(),
            move |items| {
                page.ids.borrow_mut().clear();
                while let Some(row) = page.list.row_at_index(0) {
                    page.list.remove(&row);
                }
                if items.is_empty() {
                    page.empty.set_title(&t("timeline.empty"));
                    page.empty.set_description(Some(t("timeline.none").as_str()));
                    page.stack.set_visible_child_name("empty");
                    return;
                }
                page.stack.set_visible_child_name("list");
                for item in &items {
                    let row = libadwaita::ActionRow::builder()
                        .title(&item.kind)
                        .subtitle(&format!(
                            "{} · {}",
                            short_time(&item.created_at),
                            dash(&item.summary)
                        ))
                        .activatable(true)
                        .build();
                    page.list.append(&row);
                }
                *page.ids.borrow_mut() = items;
                if let Some(row) = page.list.row_at_index(0) {
                    page.list.select_row(Some(&row));
                }
            },
        );
    }
}
