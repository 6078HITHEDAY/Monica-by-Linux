use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use monica_vault::{inspect_workbench, WorkbenchSnapshot};

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
        let refresh = pill("刷新", true);
        let path = value_label("—");
        let vault_id = value_label("—");
        let format = value_label("—");
        let schema = value_label("—");
        let tiga = value_label("—");
        let session = value_label("—");
        let migration = value_label("—");
        let counts = value_label("—");
        let size = value_label("—");
        let hint = note_label("不会在此就地把 MDBX-1 升级为 MDBX-2。需要升级时请先备份。");
        let status = status_label("尚未检查。");

        let form = gtk::Box::new(gtk::Orientation::Vertical, 14);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label("工作台")
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&button_row(&[&refresh]));
        form.append(&field("路径", &path));
        form.append(&field("vault_id", &vault_id));
        form.append(&field("格式", &format));
        form.append(&field("schema", &schema));
        form.append(&field("Tiga", &tiga));
        form.append(&field("会话", &session));
        form.append(&field("迁移", &migration));
        form.append(&field("计数", &counts));
        form.append(&field("大小", &size));
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
                "正在读取工作台…",
                move || session.workbench(),
                move |snapshot| page.show_snapshot(&snapshot),
            );
            return;
        }
        let page = self.clone();
        state.spawn_job(
            Some(self.status.clone()),
            |_| {},
            "正在只读检查…",
            move || inspect_workbench(&path),
            move |snapshot| page.show_snapshot(&snapshot),
        );
    }

    fn show_snapshot(&self, snapshot: &WorkbenchSnapshot) {
        self.path.set_label(&snapshot.path.display().to_string());
        self.vault_id.set_label(&snapshot.vault_id);
        self.format.set_label(&snapshot.format_version);
        self.schema.set_label(&format!(
            "{} → 目标 {}",
            snapshot.schema_version, snapshot.target_schema_version
        ));
        self.tiga.set_label(&snapshot.tiga_mode);
        self.session
            .set_label(if snapshot.unlocked { "已解锁" } else { "未解锁（只读）" });
        self.migration.set_label(&format!(
            "升级：{}  读：{}  写：{}  未知扩展：{}",
            if snapshot.requires_upgrade {
                "需要"
            } else {
                "否"
            },
            snapshot.min_reader_version,
            snapshot.min_writer_version,
            if snapshot.unknown_critical_extensions {
                "有"
            } else {
                "无"
            }
        ));
        self.counts.set_label(&format!(
            "登录 {} · 笔记 {} · 卡 {} · 口令 {} · 证件 {} · 其他 {} · 回收站 {} · 项目 {} · 提交 {}",
            snapshot.logins,
            snapshot.notes,
            snapshot.cards,
            snapshot.totp,
            snapshot.documents,
            snapshot.other_entries,
            snapshot.deleted_entries,
            snapshot.projects,
            snapshot.commits
        ));
        self.size
            .set_label(&format!("{} 字节", snapshot.file_size_bytes));
        if let Some(hint) = snapshot.upgrade_hint() {
            self.hint.set_label(&hint);
        } else {
            self.hint
                .set_label("当前格式可解锁。不会在此就地升级 MDBX-1。");
        }
        self.status.set_label("已刷新。");
    }
}
