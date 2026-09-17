use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{backup_vault_file, SYNC_STATUS_NOTE};

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
        let backup = pill("备份到文件", true);
        let export_bundle = pill("导出同步包", true);
        let apply_bundle = pill("应用同步包", false);
        let status = status_label("尚未备份或同步。");

        let backup_group = libadwaita::PreferencesGroup::builder()
            .title("备份")
            .description("便携 .mdbx 副本：SQLite 在线备份，DELETE 日志，不含 WAL。密文，不解锁，不升级格式。目标文件必须不存在。")
            .build();
        let sync_group = libadwaita::PreferencesGroup::builder()
            .title("离线同步包")
            .description("上游 mdbx-sync 完整包（魔数 MDBXSYNC，v3，SHA-256）。只能应用到同一 vault_id。")
            .build();

        let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label("备份同步")
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&backup_group);
        form.append(&button_row(&[&backup]));
        form.append(&sync_group);
        form.append(&button_row(&[&export_bundle, &apply_bundle]));
        form.append(&note_label(SYNC_STATUS_NOTE));
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
                    "保存便携备份",
                    "Monica 备份 (*.mdbx)",
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
                    state.show_error(Some(&page.status), "请先解锁");
                    return;
                };
                choose_save(
                    &state,
                    "导出 MDBXSYNC 包",
                    "同步包 (*.mdbx-sync)",
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
                                "正在导出同步包…",
                                move || session.export_sync_bundle(&path),
                                move |info| {
                                    status.set_label(&format!(
                                        "已导出同步包\n路径：{}\nvault_id：{}\n提交：{}  时间：{}",
                                        info.path.display(),
                                        info.vault_id,
                                        info.commits,
                                        info.exported_at
                                    ));
                                    state_ok
                                        .toast
                                        .add_toast(libadwaita::Toast::new("已导出同步包"));
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
                    state.show_error(Some(&page.status), "请先解锁");
                    return;
                };
                choose_open(
                    &state,
                    "选择 MDBXSYNC 包",
                    "同步包 (*.mdbx-sync)",
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
                                "正在应用同步包…",
                                move || session.apply_sync_bundle(&path),
                                move |info| {
                                    status.set_label(&format!(
                                        "已应用同步包\n路径：{}\nvault_id：{}\n写入 {} · 跳过 {} · 冲突 {} · 缺父 {}",
                                        info.path.display(),
                                        info.vault_id,
                                        info.applied,
                                        info.skipped,
                                        info.conflicts,
                                        info.missing_parents
                                    ));
                                    state_ok
                                        .toast
                                        .add_toast(libadwaita::Toast::new("已应用同步包"));
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
                "正在备份保险库…",
                move || session.backup_to(&path),
                {
                    let state = state.clone();
                    move |info: monica_vault::VaultBackupInfo| {
                        status.set_label(&format!(
                            "已备份\n路径：{}\nvault_id：{}\n格式：{}  schema：{}  大小：{} 字节",
                            info.path.display(),
                            info.vault_id,
                            info.format_version,
                            info.schema_version,
                            info.file_size_bytes
                        ));
                        state.toast.add_toast(libadwaita::Toast::new("已备份"));
                    }
                },
            );
            return;
        }
        let source = state.vault_path.borrow().clone();
        state.spawn_job(
            Some(status.clone()),
            |_| {},
            "正在备份保险库…",
            move || backup_vault_file(&source, &path),
            {
                let state = state.clone();
                move |info: monica_vault::VaultBackupInfo| {
                    status.set_label(&format!(
                        "已备份（未解锁）\n路径：{}\nvault_id：{}\n格式：{}  schema：{}  大小：{} 字节",
                        info.path.display(),
                        info.vault_id,
                        info.format_version,
                        info.schema_version,
                        info.file_size_bytes
                    ));
                    state.toast.add_toast(libadwaita::Toast::new("已备份"));
                }
            },
        );
    }
}
