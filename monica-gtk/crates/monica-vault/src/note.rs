//! Secure notes (`EntryType::Note`) using Android/Avalonia payload shape.

use mdbx_core::model::EntryType;
use mdbx_storage::connection::VaultConnection;
use serde_json::json;

use crate::io::{list_by_type, load_entry, save_json_entry, soft_delete_entry};
use crate::payload::{
    is_archived, json_bool, json_string, nested_item_data, take_payload_json, title_from_bytes,
};
use crate::VaultError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSummary {
    pub entry_id: String,
    pub title: String,
    pub preview: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDetail {
    pub entry_id: String,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub markdown: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft {
    pub entry_id: Option<String>,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub markdown: bool,
}

pub(crate) fn list_notes(conn: &VaultConnection) -> Result<Vec<NoteSummary>, VaultError> {
    let mut entries = list_by_type(conn, EntryType::Note)?;
    let mut summaries = Vec::new();
    for entry in &mut entries {
        let value = take_payload_json(entry);
        if is_archived(&value) {
            continue;
        }
        let nested = nested_item_data(&value);
        let content = first_content(&value, &nested);
        let title = title_from_bytes(entry.title_ct.as_deref());
        summaries.push(NoteSummary {
            entry_id: entry.entry_id.clone(),
            title,
            preview: preview_text(&content),
            updated_at: entry.updated_at.clone(),
        });
    }
    summaries.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(summaries)
}

pub(crate) fn get_note(conn: &VaultConnection, entry_id: &str) -> Result<NoteDetail, VaultError> {
    let mut entry = load_entry(conn, entry_id, false)?;
    if entry.entry_type != EntryType::Note {
        return Err(VaultError::Storage(format!("entry {entry_id} is not a note")));
    }
    let value = take_payload_json(&mut entry);
    if is_archived(&value) {
        return Err(VaultError::EntryNotFound(entry_id.to_string()));
    }
    let nested = nested_item_data(&value);
    let content = first_content(&value, &nested);
    let tags = json_string(&nested, &["tags"]);
    let tags = if tags.starts_with('[') {
        serde_json::from_str::<Vec<String>>(&tags)
            .unwrap_or_default()
            .join(", ")
    } else {
        tags
    };
    let tags = if tags.is_empty() {
        json_string(&value, &["tags"])
    } else {
        tags
    };
    Ok(NoteDetail {
        entry_id: entry.entry_id,
        title: title_from_bytes(entry.title_ct.as_deref()),
        content,
        tags,
        markdown: json_bool(&nested, &["isMarkdown", "markdown"]),
        updated_at: entry.updated_at,
    })
}

pub(crate) fn save_note(
    conn: &VaultConnection,
    draft: &NoteDraft,
) -> Result<NoteSummary, VaultError> {
    let content = draft.content.trim_end().to_string();
    let title = if draft.title.trim().is_empty() {
        resolve_title(&content)
    } else {
        draft.title.trim().to_string()
    };
    let tags = normalize_tags(&draft.tags);
    let item_data = json!({
        "content": content,
        "tags": tags,
        "isMarkdown": draft.markdown,
    });
    let payload = json!({
        "kind": "note",
        "notes": content,
        "item_data": item_data.to_string(),
        "image_paths": "[]",
        "archived": false,
    });
    let saved = save_json_entry(
        conn,
        draft.entry_id.as_deref(),
        EntryType::Note,
        &title,
        &payload,
    )?;
    Ok(NoteSummary {
        entry_id: saved.entry_id,
        title,
        preview: preview_text(&content),
        updated_at: saved.updated_at,
    })
}

pub(crate) fn delete_note(conn: &VaultConnection, entry_id: &str) -> Result<(), VaultError> {
    soft_delete_entry(conn, entry_id)
}

fn first_content(value: &serde_json::Value, nested: &serde_json::Value) -> String {
    let nested_content = json_string(nested, &["content"]);
    if !nested_content.is_empty() {
        return nested_content;
    }
    json_string(value, &["notes", "content", "note"])
}

fn preview_text(content: &str) -> String {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let trimmed = line.trim_start_matches(|ch: char| {
        matches!(ch, '#' | '-' | '*' | '+' | '>' | ' ' | '\t')
    });
    if trimmed.chars().count() > 48 {
        format!("{}…", trimmed.chars().take(48).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

fn resolve_title(content: &str) -> String {
    let preview = preview_text(content);
    if preview.is_empty() {
        "未命名笔记".to_string()
    } else {
        preview
    }
}

fn normalize_tags(raw: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for part in raw.split([',', '\n']) {
        let tag = part.trim();
        if tag.is_empty() {
            continue;
        }
        if !tags
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(tag))
        {
            tags.push(tag.to_string());
        }
    }
    tags
}
