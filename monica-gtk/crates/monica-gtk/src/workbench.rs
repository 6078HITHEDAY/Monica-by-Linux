use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{inspect_workbench, WorkbenchSnapshot};

use crate::i18n::{t, tf};
use crate::dialogs::{button_row, note_label, pill, status_label};
use crate::prefs::scrolled_clamp;
use crate::state::AppState;
use crate::widgets::{field, value_label};

#[derive(Clone)]
pub struct WorkbenchPage {
    pub root: gtk::Widget,
    path: gtk::Label,
    vault_id: gtk::Label,
    format: gtk::Label,
    schema: gtk::Label,
    tiga: gtk::Label,
    session: gtk::Label,
    migration: gtk::Label,
    counts: gtk::Label,
    size: gtk::Label,
    hint: gtk::Label,
    status: gtk::Label,
}

impl WorkbenchPage {
    pub fn build(state: &AppState) -> Self {
        let refresh = pill(&t("workbench.refresh"), true);
        let path = value_label("—");
        let vault_id = value_label("—");
        let format = value_label("—");
        let schema = value_label("—");
        let tiga = value_label("—");
        let session = value_label("—");
        let migration = value_label("—");
        let counts = value_label("—");
        let size = value_label("—");
        let hint = note_label(&t("workbench.hint"));
        let status = status_label(&t("workbench.idle"));

        let form = gtk::Box::new(gtk::Orientation::Vertical, 14);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label(t("nav.workbench"))
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&button_row(&[&refresh]));
        form.append(&field(&t("common.path"), &path));
        form.append(&field(&t("workbench.vault_id"), &vault_id));
        form.append(&field(&t("workbench.format"), &format));
        form.append(&field(&t("workbench.schema"), &schema));
        form.append(&field(&t("workbench.tiga"), &tiga));
        form.append(&field(&t("workbench.session"), &session));
        form.append(&field(&t("workbench.migration"), &migration));
        form.append(&field(&t("workbench.counts"), &counts));
        form.append(&field(&t("workbench.size"), &size));
        form.append(&hint);
        form.append(&status);

        let page = Self {
            root: scrolled_clamp(&form),
            path,
            vault_id,
            format,
            schema,
            tiga,
            session,
            migration,
            counts,
            size,
            hint,
            status,
        };

        refresh.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                page.reload(&state);
            }
        ));
        page
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.reload(state);
    }

    pub fn clear_sensitive(&self) {}

    fn reload(&self, state: &AppState) {
        let path = state.vault_path.borrow().clone();
        if let Some(session) = state.current_session() {
            let page = self.clone();
            state.spawn_job(
                Some(self.status.clone()),
                |_| {},
                &t("workbench.reading"),
                move || session.workbench(),
                move |snapshot| page.show_snapshot(&snapshot),
            );
            return;
        }
        let page = self.clone();
        state.spawn_job(
            Some(self.status.clone()),
            |_| {},
            &t("workbench.inspecting"),
            move || inspect_workbench(&path),
            move |snapshot| page.show_snapshot(&snapshot),
        );
    }

    fn show_snapshot(&self, snapshot: &WorkbenchSnapshot) {
        self.path.set_label(&snapshot.path.display().to_string());
        self.vault_id.set_label(&snapshot.vault_id);
        self.format.set_label(&snapshot.format_version);
        self.schema.set_label(&tf(
            "workbench.schema_target",
            &[
                &snapshot.schema_version.to_string(),
                &snapshot.target_schema_version.to_string(),
            ],
        ));
        self.tiga.set_label(&snapshot.tiga_mode);
        let session = if snapshot.unlocked {
            t("workbench.unlocked")
        } else {
            t("workbench.locked")
        };
        self.session.set_label(&session);
        let upgrade = if snapshot.requires_upgrade {
            t("workbench.upgrade_yes")
        } else {
            t("workbench.upgrade_no")
        };
        let ext = if snapshot.unknown_critical_extensions {
            t("workbench.ext_yes")
        } else {
            t("workbench.ext_no")
        };
        self.migration.set_label(&tf(
            "workbench.migration_line",
            &[
                &upgrade,
                &snapshot.min_reader_version.to_string(),
                &snapshot.min_writer_version.to_string(),
                &ext,
            ],
        ));
        self.counts.set_label(&tf(
            "workbench.counts_line",
            &[
                &snapshot.logins.to_string(),
                &snapshot.notes.to_string(),
                &snapshot.cards.to_string(),
                &snapshot.totp.to_string(),
                &snapshot.documents.to_string(),
                &snapshot.other_entries.to_string(),
                &snapshot.deleted_entries.to_string(),
                &snapshot.projects.to_string(),
                &snapshot.commits.to_string(),
            ],
        ));
        self.size
            .set_label(&tf("workbench.bytes", &[&snapshot.file_size_bytes.to_string()]));
        if let Some(hint) = snapshot.upgrade_hint() {
            self.hint.set_label(&hint);
        } else {
            self.hint.set_label(&t("workbench.ok_hint"));
        }
        self.status.set_label(&t("workbench.refreshed"));
    }
}
