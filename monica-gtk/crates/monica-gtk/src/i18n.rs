//! GNU gettext catalogs for GTK UI strings.
//!
//! `msgid` is a stable key (issue #7 / #8 键集). Catalogs live in
//! `monica-gtk/po/{zh_CN,en}.po`. No `gettext-sys` — catalogs are parsed
//! from the `.po` files embedded at compile time so Flatpak stays offline.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const ZH_PO: &str = include_str!("../../../po/zh_CN.po");
const EN_PO: &str = include_str!("../../../po/en.po");

static CATALOGS: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();
static LOCALE: Mutex<String> = Mutex::new(String::new());

const DEFAULT_LOCALE: &str = "zh_CN";

fn catalogs() -> &'static HashMap<String, HashMap<String, String>> {
    CATALOGS.get_or_init(|| {
        let mut map = HashMap::new();
        map.insert("zh_CN".into(), parse_po(ZH_PO));
        map.insert("en".into(), parse_po(EN_PO));
        map
    })
}

/// Parse a minimal gettext `.po`: `msgid "key"` / `msgstr "value"` with
/// C escapes. Headers (`msgid ""`) are skipped.
pub fn parse_po(source: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut msgid: Option<String> = None;
    for raw in source.lines() {
        let line = raw.trim();
        if line.starts_with("msgid ") {
            msgid = Some(unquote(line[6..].trim()));
        } else if line.starts_with("msgstr ") {
            let value = unquote(line[7..].trim());
            if let Some(key) = msgid.take() {
                if !key.is_empty() {
                    map.insert(key, value);
                }
            }
        }
    }
    map
}

fn unquote(token: &str) -> String {
    let trimmed = token.trim();
    let inner = trimmed
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(trimmed);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn normalize_locale(spec: &str) -> String {
    let lowered = spec.trim().replace('-', "_");
    let primary = lowered
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if primary.is_empty() || primary == "system" || primary == "c" || primary == "posix" {
        return detect_system_locale();
    }
    if primary == "zh" || primary.starts_with("zh_") {
        "zh_CN".into()
    } else if primary == "en" || primary.starts_with("en_") {
        "en".into()
    } else {
        DEFAULT_LOCALE.into()
    }
}

fn detect_system_locale() -> String {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                let primary = value.split(['.', '@']).next().unwrap_or(&value);
                let lower = primary.to_ascii_lowercase().replace('-', "_");
                if lower == "c" || lower == "posix" {
                    continue;
                }
                if lower.starts_with("en") {
                    return "en".into();
                }
                if lower.starts_with("zh") {
                    return "zh_CN".into();
                }
            }
        }
    }
    DEFAULT_LOCALE.into()
}

/// Apply `settings.json` locale (`system` / `zh_CN` / `en`) or `MONICA_GTK_LANG`.
pub fn apply_preference(settings_locale: &str) {
    if let Ok(env) = std::env::var("MONICA_GTK_LANG") {
        if !env.trim().is_empty() {
            set_locale(&env);
            return;
        }
    }
    if settings_locale.trim().is_empty() || settings_locale == "system" {
        set_locale(&detect_system_locale());
    } else {
        set_locale(settings_locale);
    }
}

pub fn init() {
    apply_preference("system");
}

pub fn set_locale(spec: &str) {
    if let Ok(mut guard) = LOCALE.lock() {
        *guard = normalize_locale(spec);
    }
}

pub fn current_locale() -> String {
    LOCALE
        .lock()
        .ok()
        .map(|guard| {
            if guard.is_empty() {
                detect_system_locale()
            } else {
                guard.clone()
            }
        })
        .unwrap_or_else(detect_system_locale)
}

pub fn t(key: &str) -> String {
    lookup(key).unwrap_or_else(|| key.to_string())
}

pub fn tf(key: &str, args: &[&str]) -> String {
    let mut text = t(key);
    for (index, arg) in args.iter().enumerate() {
        text = text.replace(&format!("{{{index}}}"), arg);
    }
    text
}

fn lookup(key: &str) -> Option<String> {
    let catalogs = catalogs();
    let locale = current_locale();
    if let Some(value) = catalogs.get(&locale).and_then(|map| map.get(key)) {
        if !value.is_empty() {
            return Some(value.clone());
        }
    }
    catalogs
        .get(DEFAULT_LOCALE)
        .and_then(|map| map.get(key))
        .cloned()
}

#[cfg(test)]
pub fn catalog_keys(locale: &str) -> Vec<String> {
    let mut keys: Vec<String> = catalogs()
        .get(locale)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_share_the_same_keyset() {
        let zh = catalog_keys("zh_CN");
        let en = catalog_keys("en");
        assert!(!zh.is_empty());
        assert_eq!(zh, en);
        assert!(zh.contains(&"unlock.confirm_password".to_string()));
        assert!(zh.contains(&"backup.online_blocked".to_string()));
        assert!(!zh.contains(&"nav.unlock".to_string()));
    }

    #[test]
    fn lookup_respects_locale_and_placeholders() {
        set_locale("zh_CN");
        assert_eq!(t("nav.passwords"), "密码库");
        assert_eq!(t("unlock.onboard_heading"), "还没有本地密码库");
        assert_eq!(tf("common.failed", &["boom"]), "失败：boom");
        set_locale("en");
        assert_eq!(t("nav.passwords"), "Password Vault");
        assert_eq!(t("unlock.onboard_heading"), "No local password library yet");
        assert_eq!(tf("common.failed", &["boom"]), "Failed: boom");
        set_locale("zh_CN");
    }

    #[test]
    fn missing_key_falls_back_to_msgid() {
        set_locale("en");
        assert_eq!(t("does.not.exist"), "does.not.exist");
    }

    #[test]
    fn parse_po_skips_header_and_unescapes() {
        let parsed = parse_po(
            r#"
msgid ""
msgstr "Language: en\n"

msgid "sample.key"
msgstr "line\nnext"
"#,
        );
        assert_eq!(parsed.get("sample.key").unwrap(), "line\nnext");
        assert!(!parsed.contains_key(""));
    }

    #[test]
    fn zh_catalog_does_not_say_baoxianku() {
        assert!(!ZH_PO.contains("保险库"));
    }
}
