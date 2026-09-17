use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{TransferSummary, MONICA_JSON_FORMAT};
use secrecy::SecretString;

use crate::i18n::{t, tf};
use crate::dialogs::{ask_secret, button_row, choose_open, choose_save, note_label, pill, status_label};
use crate::prefs::scrolled_clamp;
use crate::state::AppState;

#[derive(Clone)]
pub struct TransferPage {
    pub root: gtk::Widget,
    status: gtk::Label,
}

impl TransferPage {
    pub fn build(state: &AppState) -> Self {
        let export_monica = pill(&t("transfer.export_monica"), true);
        let import_monica = pill(&t("transfer.import_monica"), false);
        let export_kdbx = pill(&t("transfer.export_kdbx"), true);
        let import_kdbx = pill(&t("transfer.import_kdbx"), false);
        let export_kdbx_bin = pill(&t("transfer.export_kdbx_bin"), true);
        let import_kdbx_bin = pill(&t("transfer.import_kdbx_bin"), false);
        let export_password_csv = pill(&t("transfer.export_password_csv"), true);
        let export_combined_csv = pill(&t("transfer.export_combined_csv"), true);
        let import_csv = pill(&t("transfer.import_csv"), false);
        let status = status_label(&t("transfer.idle"));

        let monica = libadwaita::PreferencesGroup::builder()
            .title("Monica JSON")
            .description(tf("transfer.monica_desc", &[MONICA_JSON_FORMAT]))
            .build();
        let kdbx = libadwaita::PreferencesGroup::builder()
            .title("KDBX JSON")
            .description(t("transfer.kdbx_desc"))
            .build();
        let kdbx_bin = libadwaita::PreferencesGroup::builder()
            .title(t("transfer.kdbx_bin"))
            .description(t("transfer.kdbx_bin_desc"))
            .build();
        let csv = libadwaita::PreferencesGroup::builder()
            .title("CSV")
            .description(t("transfer.csv_desc"))
            .build();

        let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label(t("nav.transfer"))
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&monica);
        form.append(&button_row(&[&export_monica, &import_monica]));
        form.append(&kdbx);
        form.append(&button_row(&[&export_kdbx, &import_kdbx]));
        form.append(&kdbx_bin);
        form.append(&button_row(&[&export_kdbx_bin, &import_kdbx_bin]));
        form.append(&csv);
        form.append(&button_row(&[
            &export_password_csv,
            &export_combined_csv,
            &import_csv,
        ]));
        form.append(&note_label(
            &t("transfer.note"),
        ));
        form.append(&status);

        let page = Self {
            root: scrolled_clamp(&form),
            status,
        };

        export_monica.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_save(&state, &t("transfer.export_monica"), &t("transfer.json_filter"), "*.json", "monica-export.json", |session, path| session.export_monica_json(&path), t("transfer.exported_monica"))
        ));
        import_monica.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_open(&state, &t("transfer.import_monica"), &t("transfer.json_filter"), "*.json", |session, path| session.import_monica_json(&path), t("transfer.imported_monica"))
        ));
        export_kdbx.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_save(&state, &t("transfer.export_kdbx"), &t("transfer.json_filter"), "*.json", "monica-kdbx.json", |session, path| session.export_kdbx_json(&path), t("transfer.exported_kdbx"))
        ));
        import_kdbx.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_open(&state, &t("transfer.import_kdbx"), &t("transfer.json_filter"), "*.json", |session, path| session.import_kdbx_json(&path), t("transfer.imported_kdbx"))
        ));
        export_kdbx_bin.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_save_secret(
                &state,
                &t("transfer.export_kdbx_bin"),
                &t("transfer.kdbx_filter"),
                "*.kdbx",
                "monica-export.kdbx",
                t("transfer.file_password"),
                t("transfer.file_password_export"),
                |session, path, password| session.export_kdbx_binary(&path, &password),
                t("transfer.exported_kdbx_bin"),
            )
        ));
        import_kdbx_bin.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_open_secret(
                &state,
                &t("transfer.import_kdbx_bin"),
                &t("transfer.kdbx_filter"),
                "*.kdbx",
                t("transfer.file_password"),
                t("transfer.file_password_import"),
                |session, path, password| session.import_kdbx_binary(&path, &password),
                t("transfer.imported_kdbx_bin"),
            )
        ));
        export_password_csv.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_save(&state, &t("transfer.export_password_csv"), &t("transfer.csv_filter"), "*.csv", "monica-passwords.csv", |session, path| session.export_password_csv(&path), t("transfer.exported_password_csv"))
        ));
        export_combined_csv.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_save(&state, &t("transfer.export_combined_csv"), &t("transfer.csv_filter"), "*.csv", "monica-export.csv", |session, path| session.export_combined_csv(&path), t("transfer.exported_combined_csv"))
        ));
        import_csv.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_open(&state, &t("transfer.import_csv"), &t("transfer.csv_filter"), "*.csv", |session, path| session.import_csv(&path), t("transfer.imported_csv"))
        ));

        page
    }

    pub fn on_session_changed(&self, _state: &AppState) {}

    pub fn clear_sensitive(&self) {}

    fn need_session_save<F>(
        &self,
        state: &AppState,
        title: &str,
        filter_name: &str,
        pattern: &str,
        initial_name: &str,
        work: F,
        toast: String,
    ) where
        F: FnOnce(monica_vault::VaultSession, std::path::PathBuf) -> Result<TransferSummary, monica_vault::VaultError>
            + Send
            + 'static,
    {
        if state.current_session().is_none() {
            state.show_error(Some(&self.status), &t("common.unlock_first"));
            return;
        }
        let page = self.clone();
        let dialog_state = state.clone();
        let job_state = state.clone();
        choose_save(
            &dialog_state,
            title,
            filter_name,
            pattern,
            initial_name,
            move |path| {
                let Some(session) = job_state.current_session() else {
                    return;
                };
                let status = page.status.clone();
                let done_state = job_state.clone();
                job_state.spawn_job(
                    Some(status.clone()),
                    |_| {},
                    &t("transfer.exporting"),
                    move || work(session, path),
                    move |summary| {
                        status.set_label(&format_summary(&summary));
                        done_state.toast.add_toast(libadwaita::Toast::new(&toast));
                    },
                );
            },
        );
    }

    fn need_session_open<F>(
        &self,
        state: &AppState,
        title: &str,
        filter_name: &str,
        pattern: &str,
        work: F,
        toast: String,
    ) where
        F: FnOnce(monica_vault::VaultSession, std::path::PathBuf) -> Result<TransferSummary, monica_vault::VaultError>
            + Send
            + 'static,
    {
        if state.current_session().is_none() {
            state.show_error(Some(&self.status), &t("common.unlock_first"));
            return;
        }
        let page = self.clone();
        let dialog_state = state.clone();
        let job_state = state.clone();
        choose_open(
            &dialog_state,
            title,
            filter_name,
            pattern,
            move |path| {
                let Some(session) = job_state.current_session() else {
                    return;
                };
                let status = page.status.clone();
                let done_state = job_state.clone();
                job_state.spawn_job(
                    Some(status.clone()),
                    |_| {},
                    &t("transfer.importing"),
                    move || work(session, path),
                    move |summary| {
                        status.set_label(&format_summary(&summary));
                        done_state.toast.add_toast(libadwaita::Toast::new(&toast));
                    },
                );
            },
        );
    }

    fn need_session_save_secret<F>(
        &self,
        state: &AppState,
        title: &str,
        filter_name: &str,
        pattern: &str,
        initial_name: &str,
        secret_title: String,
        secret_body: String,
        work: F,
        toast: String,
    ) where
        F: FnOnce(
                monica_vault::VaultSession,
                std::path::PathBuf,
                SecretString,
            ) -> Result<TransferSummary, monica_vault::VaultError>
            + Send
            + 'static,
    {
        if state.current_session().is_none() {
            state.show_error(Some(&self.status), &t("common.unlock_first"));
            return;
        }
        let page = self.clone();
        let dialog_state = state.clone();
        let job_state = state.clone();
        choose_save(
            &dialog_state.clone(),
            title,
            filter_name,
            pattern,
            initial_name,
            move |path| {
                let prompt_state = dialog_state.clone();
                let job_state = job_state.clone();
                let page = page.clone();
                ask_secret(&prompt_state, &secret_title, &secret_body, move |password| {
                    let Some(session) = job_state.current_session() else {
                        return;
                    };
                    let status = page.status.clone();
                    let done_state = job_state.clone();
                    job_state.spawn_job(
                        Some(status.clone()),
                        |_| {},
                        &t("transfer.exporting"),
                        move || work(session, path, password),
                        move |summary| {
                            status.set_label(&format_summary(&summary));
                            done_state.toast.add_toast(libadwaita::Toast::new(&toast));
                        },
                    );
                });
            },
        );
    }

    fn need_session_open_secret<F>(
        &self,
        state: &AppState,
        title: &str,
        filter_name: &str,
        pattern: &str,
        secret_title: String,
        secret_body: String,
        work: F,
        toast: String,
    ) where
        F: FnOnce(
                monica_vault::VaultSession,
                std::path::PathBuf,
                SecretString,
            ) -> Result<TransferSummary, monica_vault::VaultError>
            + Send
            + 'static,
    {
        if state.current_session().is_none() {
            state.show_error(Some(&self.status), &t("common.unlock_first"));
            return;
        }
        let page = self.clone();
        let dialog_state = state.clone();
        let job_state = state.clone();
        choose_open(
            &dialog_state.clone(),
            title,
            filter_name,
            pattern,
            move |path| {
                let prompt_state = dialog_state.clone();
                let job_state = job_state.clone();
                let page = page.clone();
                ask_secret(&prompt_state, &secret_title, &secret_body, move |password| {
                    let Some(session) = job_state.current_session() else {
                        return;
                    };
                    let status = page.status.clone();
                    let done_state = job_state.clone();
                    job_state.spawn_job(
                        Some(status.clone()),
                        |_| {},
                        &t("transfer.importing"),
                        move || work(session, path, password),
                        move |summary| {
                            status.set_label(&format_summary(&summary));
                            done_state.toast.add_toast(libadwaita::Toast::new(&toast));
                        },
                    );
                });
            },
        );
    }
}

fn format_summary(summary: &TransferSummary) -> String {
    let path = summary.path.display().to_string();
    let mut text = tf(
        "transfer.summary",
        &[&summary.format, &path, &summary.short_status()],
    );
    for warning in summary.warnings.iter().take(6) {
        text.push_str("\n· ");
        text.push_str(warning);
    }
    if summary.warnings.len() > 6 {
        text.push_str("\n…");
    }
    text
}
