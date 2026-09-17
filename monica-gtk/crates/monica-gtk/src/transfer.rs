use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{TransferSummary, MONICA_JSON_FORMAT};

use crate::dialogs::{button_row, choose_open, choose_save, note_label, pill, status_label};
use crate::prefs::scrolled_clamp;
use crate::state::AppState;

#[derive(Clone)]
pub struct TransferPage {
    pub root: gtk::Widget,
    status: gtk::Label,
}

impl TransferPage {
    pub fn build(state: &AppState) -> Self {
        let export_monica = pill("导出 Monica JSON", true);
        let import_monica = pill("导入 Monica JSON", false);
        let export_kdbx = pill("导出 KDBX JSON", true);
        let import_kdbx = pill("导入 KDBX JSON", false);
        let status = status_label("尚未导入或导出。");

        let monica = libadwaita::PreferencesGroup::builder()
            .title("Monica JSON")
            .description(format!(
                "格式 `{MONICA_JSON_FORMAT}`：登录 / 笔记 / 钱包 / 独立口令。明文密钥，请妥善保管。"
            ))
            .build();
        let kdbx = libadwaita::PreferencesGroup::builder()
            .title("KDBX JSON")
            .description("与 mdbx-cli 的 import-kdbx-json / 逻辑条目 JSON 相同。导入按上游规则为每条建 project。GTK 多登录共用一个默认 project，故导出按登录条目写出，而不是按 project 折叠。")
            .build();

        let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label("导入导出")
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&monica);
        form.append(&button_row(&[&export_monica, &import_monica]));
        form.append(&kdbx);
        form.append(&button_row(&[&export_kdbx, &import_kdbx]));
        form.append(&note_label(
            "未做：二进制 .kdbx、Bitwarden JSON、CSV。冻结线 Avalonia 有这些解析器，本 crate 没有对应 Rust 实现。",
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
            move |_| page.need_session_save(&state, "导出 Monica JSON", "*.json", "monica-export.json", |session, path| session.export_monica_json(&path), "已导出 Monica JSON")
        ));
        import_monica.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_open(&state, "导入 Monica JSON", "*.json", |session, path| session.import_monica_json(&path), "已导入 Monica JSON")
        ));
        export_kdbx.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_save(&state, "导出 KDBX JSON", "*.json", "monica-kdbx.json", |session, path| session.export_kdbx_json(&path), "已导出 KDBX JSON")
        ));
        import_kdbx.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| page.need_session_open(&state, "导入 KDBX JSON", "*.json", |session, path| session.import_kdbx_json(&path), "已导入 KDBX JSON")
        ));

        page
    }

    pub fn on_session_changed(&self, _state: &AppState) {}

    pub fn clear_sensitive(&self) {}

    fn need_session_save<F>(
        &self,
        state: &AppState,
        title: &str,
        pattern: &str,
        initial_name: &str,
        work: F,
        toast: &'static str,
    ) where
        F: FnOnce(monica_vault::VaultSession, std::path::PathBuf) -> Result<TransferSummary, monica_vault::VaultError>
            + Send
            + 'static,
    {
        if state.current_session().is_none() {
            state.show_error(Some(&self.status), "请先解锁");
            return;
        }
        let page = self.clone();
        let dialog_state = state.clone();
        let job_state = state.clone();
        choose_save(
            &dialog_state,
            title,
            "JSON (*.json)",
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
                    "正在导出…",
                    move || work(session, path),
                    move |summary| {
                        status.set_label(&format_summary(&summary));
                        done_state.toast.add_toast(libadwaita::Toast::new(toast));
                    },
                );
            },
        );
    }

    fn need_session_open<F>(
        &self,
        state: &AppState,
        title: &str,
        pattern: &str,
        work: F,
        toast: &'static str,
    ) where
        F: FnOnce(monica_vault::VaultSession, std::path::PathBuf) -> Result<TransferSummary, monica_vault::VaultError>
            + Send
            + 'static,
    {
        if state.current_session().is_none() {
            state.show_error(Some(&self.status), "请先解锁");
            return;
        }
        let page = self.clone();
        let dialog_state = state.clone();
        let job_state = state.clone();
        choose_open(
            &dialog_state,
            title,
            "JSON (*.json)",
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
                    "正在导入…",
                    move || work(session, path),
                    move |summary| {
                        status.set_label(&format_summary(&summary));
                        done_state.toast.add_toast(libadwaita::Toast::new(toast));
                    },
                );
            },
        );
    }
}

fn format_summary(summary: &TransferSummary) -> String {
    let mut text = format!(
        "{}\n路径：{}\n{}",
        summary.format,
        summary.path.display(),
        summary.short_status()
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
