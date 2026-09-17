//! Shared payload helpers. Entries store decrypted JSON in `payload_ct`.

use mdbx_core::model::{Entry, EntryType};
use serde_json::Value;
use zeroize::Zeroize;

use crate::VaultError;

pub(crate) const UNNAMED_TITLE: &str = "未命名";

pub(crate) fn title_from_bytes(bytes: Option<&[u8]>) -> String {
    bytes
        .map(|raw| String::from_utf8_lossy(raw).into_owned())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| UNNAMED_TITLE.to_string())
}

pub(crate) fn take_payload_json(entry: &mut Entry) -> Value {
    let mut payload_bytes = std::mem::take(&mut entry.payload_ct);
    let value = serde_json::from_slice::<Value>(&payload_bytes).unwrap_or(Value::Object(
        serde_json::Map::new(),
    ));
    payload_bytes.zeroize();
    value
}

pub(crate) fn json_string(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|item| item.as_str()) {
            return text.to_string();
        }
    }
    String::new()
}

pub(crate) fn json_bool(value: &Value, keys: &[&str]) -> bool {
    for key in keys {
        let Some(item) = value.get(*key) else {
            continue;
        };
        match item {
            Value::Bool(flag) => return *flag,
            Value::String(text) => {
                if text.eq_ignore_ascii_case("true") || text == "1" {
                    return true;
                }
            }
            Value::Number(number) => {
                if number.as_i64() == Some(1) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub(crate) fn json_u32(value: &Value, keys: &[&str], default: u32) -> u32 {
    for key in keys {
        let Some(item) = value.get(*key) else {
            continue;
        };
        if let Some(number) = item.as_u64() {
            return number.min(u64::from(u32::MAX)) as u32;
        }
        if let Some(text) = item.as_str() {
            if let Ok(parsed) = text.parse::<u32>() {
                return parsed;
            }
        }
    }
    default
}

pub(crate) fn is_archived(value: &Value) -> bool {
    json_bool(value, &["archived", "is_archived"])
}

pub(crate) fn nested_item_data(value: &Value) -> Value {
    let raw = json_string(value, &["item_data", "itemData"]);
    if raw.is_empty() {
        return value.clone();
    }
    serde_json::from_str(&raw).unwrap_or_else(|_| value.clone())
}

pub(crate) fn encode_payload_bytes(payload: &Value) -> Result<Vec<u8>, VaultError> {
    serde_json::to_vec(payload)
        .map_err(|error| VaultError::Storage(format!("encode payload: {error}")))
}

pub(crate) fn kind_label(entry_type: &EntryType) -> &'static str {
    match entry_type {
        EntryType::Login => "登录",
        EntryType::Note => "笔记",
        EntryType::Card => "银行卡",
        EntryType::Totp => "动态口令",
        EntryType::DocumentRef => "证件",
        EntryType::Identity => "身份",
        _ => "条目",
    }
}

pub(crate) fn require_title(title: &str) -> Result<&str, VaultError> {
    let title = title.trim();
    if title.is_empty() {
        Err(VaultError::Storage("条目标题不能为空".to_string()))
    } else {
        Ok(title)
    }
}
