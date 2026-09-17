use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::backup_vault_file;

use crate::i18n::{t, tf};
use crate::dialogs::{button_row, choose_open, choose_save, note_label, pill, status_label};
use crate::prefs::scrolled_clamp;
use crate::state::AppState;

#[derive(Clone)]
pub struct BackupSyncPage {
    pub root: gtk::Widget,
    status: gtk::Label,
}

impl BackupSyncPage {
    pub fn build(state: &AppState) -> Self {
        let backup = pill(&t("backup.to_file"), true);
        let export_bundle = pill(&t("backup.export_bundle"), true);
        let apply_bundle = pill(&t("backup.apply_bundle"), false);
        let status = status_label(&t("backup.idle"));

        let backup_group = libadwaita::PreferencesGroup::builder()
            .title(t("backup.group"))
            .description(t("backup.group_desc"))
            .build();
        let sync_group = libadwaita::PreferencesGroup::builder()
            .title(t("backup.sync_group"))
            .description(
                t("backup.sync_group_desc"),
            )
            .build();

        let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label(t("nav.backup"))
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&backup_group);
        form.append(&button_row(&[&backup]));
        form.append(&sync_group);
        form.append(&button_row(&[&export_bundle, &apply_bundle]));
        let online_group = libadwaita::PreferencesGroup::builder()
            .title(t("backup.online_blocked_title"))
            .description(t("backup.online_blocked"))
            .build();
        form.append(&online_group);
        form.append(&note_label(&t("backup.sync_status_note")));
        form.append(&status);

        let page = Self {
            root: scrolled_clamp(&form),
            status,
        };

        backup.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                choose_save(
                    &state,
                    &t("backup.save"),
                    &t("backup.filter"),
                    "*.mdbx",
                    "monica-backup.mdbx",
                    {
                        let state = state.clone();
                        let page = page.clone();
                        move |path| page.run_backup(&state, path)
                    },
                );
            }
        ));
        export_bundle.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                let Some(_) = state.current_session() else {
                    state.show_error(Some(&page.status), &t("common.unlock_first"));
                    return;
                };
                choose_save(
                    &state,
                    &t("backup.export_title"),
                    &t("backup.bundle_filter"),
                    "*.mdbx-sync",
                    "monica-sync.mdbx-sync",
                    {
                        let state = state.clone();
                        let page = page.clone();
                        move |path| {
                            let Some(session) = state.current_session() else {
                                return;
                            };
                            let status = page.status.clone();
                            let state_ok = state.clone();
                            state.spawn_job(
                                Some(status.clone()),
                                |_| {},
                                &t("backup.exporting"),
                                move || session.export_sync_bundle(&path),
                                move |info| {
                                    status.set_label(&tf(
                                        "backup.exported_status",
                                        &[
                                            &info.path.display().to_string(),
                                            &info.vault_id,
                                            &info.commits.to_string(),
                                            &info.exported_at,
                                        ],
                                    ));
                                    state_ok
                                        .toast
                                        .add_toast(libadwaita::Toast::new(&t("backup.exported")));
                                },
                            );
                        }
                    },
                );
            }
        ));
        apply_bundle.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                let Some(_) = state.current_session() else {
                    state.show_error(Some(&page.status), &t("common.unlock_first"));
                    return;
                };
                choose_open(
                    &state,
                    &t("backup.apply_title"),
                    &t("backup.bundle_filter"),
                    "*.mdbx-sync",
                    {
                        let state = state.clone();
                        let page = page.clone();
                        move |path| {
                            let Some(session) = state.current_session() else {
                                return;
                            };
                            let status = page.status.clone();
                            let state_ok = state.clone();
                            state.spawn_job(
                                Some(status.clone()),
                                |_| {},
                                &t("backup.applying"),
                                move || session.apply_sync_bundle(&path),
                                move |info| {
                                    status.set_label(&tf(
                                        "backup.applied_status",
                                        &[
                                            &info.path.display().to_string(),
                                            &info.vault_id,
                                            &info.applied.to_string(),
                                            &info.skipped.to_string(),
                                            &info.conflicts.to_string(),
                                            &info.missing_parents.to_string(),
                                        ],
                                    ));
                                    state_ok
                                        .toast
                                        .add_toast(libadwaita::Toast::new(&t("backup.applied")));
                                },
                            );
                        }
                    },
                );
            }
        ));

        page
    }

    pub fn on_session_changed(&self, _state: &AppState) {}

    pub fn clear_sensitive(&self) {}

    fn run_backup(&self, state: &AppState, path: std::path::PathBuf) {
        let status = self.status.clone();
        if let Some(session) = state.current_session() {
            state.spawn_job(
                Some(status.clone()),
                |_| {},
                &t("backup.backing_up"),
                move || session.backup_to(&path),
                {
                    let state = state.clone();
                    move |info: monica_vault::VaultBackupInfo| {
                        status.set_label(&tf(
                            "backup.done_status",
                            &[
                                &info.path.display().to_string(),
                                &info.vault_id,
                                &info.format_version,
                                &info.schema_version.to_string(),
                                &info.file_size_bytes.to_string(),
                            ],
                        ));
                        state.toast.add_toast(libadwaita::Toast::new(&t("backup.done")));
                        state.notify("backup-done", &t("app.name"), &t("backup.notify"));
                    }
                },
            );
            return;
        }
        let source = state.vault_path.borrow().clone();
        state.spawn_job(
            Some(status.clone()),
            |_| {},
            &t("backup.backing_up"),
            move || backup_vault_file(&source, &path),
            {
                let state = state.clone();
                move |info: monica_vault::VaultBackupInfo| {
                    status.set_label(&tf(
                        "backup.done_locked_status",
                        &[
                            &info.path.display().to_string(),
                            &info.vault_id,
                            &info.format_version,
                            &info.schema_version.to_string(),
                            &info.file_size_bytes.to_string(),
                        ],
                    ));
                    state.toast.add_toast(libadwaita::Toast::new(&t("backup.done")));
                    state.notify("backup-done", &t("app.name"), &t("backup.notify"));
                }
            },
        );
    }
}
