//! TOTP entries (`EntryType::Totp`) plus codes for login-bound authenticator keys.

use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE32;
use hmac::{Hmac, Mac};
use mdbx_core::model::EntryType;
use mdbx_storage::connection::VaultConnection;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{json, Value};
use sha1::Sha1;

use crate::io::{list_by_type, load_entry, save_json_entry, soft_delete_entry};
use crate::payload::{
    is_archived, json_string, json_u32, nested_item_data, take_payload_json, title_from_bytes,
};
use crate::password::get_password_entry;
use crate::VaultError;

const DEFAULT_PERIOD: u32 = 30;
const DEFAULT_DIGITS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpSource {
    Standalone,
    Login,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpSummary {
    pub entry_id: String,
    pub source: TotpSource,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub period: u32,
    pub digits: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct TotpDetail {
    pub entry_id: String,
    pub source: TotpSource,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub period: u32,
    pub digits: u32,
    pub secret: SecretString,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct TotpDraft {
    pub entry_id: Option<String>,
    pub source: TotpSource,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub secret: SecretString,
    pub period: u32,
    pub digits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpCode {
    pub code: String,
    pub remaining: u32,
    pub period: u32,
}

struct TotpFields {
    title: String,
    issuer: String,
    account: String,
    period: u32,
    digits: u32,
    secret: String,
}

pub(crate) fn list_totp_entries(conn: &VaultConnection) -> Result<Vec<TotpSummary>, VaultError> {
    let mut summaries = Vec::new();
    let mut standalone = list_by_type(conn, EntryType::Totp)?;
    for entry in &mut standalone {
        if is_archived_entry(entry) {
            continue;
        }
        let fields = TotpFields::from_entry(entry, false)?;
        summaries.push(TotpSummary {
            entry_id: entry.entry_id.clone(),
            source: TotpSource::Standalone,
            title: fields.title,
            issuer: fields.issuer,
            account: fields.account,
            period: fields.period,
            digits: fields.digits,
            updated_at: entry.updated_at.clone(),
        });
    }

    let mut logins = list_by_type(conn, EntryType::Login)?;
    for entry in &mut logins {
        let value = take_payload_json(entry);
        if is_archived(&value) {
            continue;
        }
        let raw = json_string(&value, &["authenticator_key", "authenticatorKey", "otp"]);
        if raw.trim().is_empty() {
            continue;
        }
        let parsed = parse_totp_input(&raw);
        let title = title_from_bytes(entry.title_ct.as_deref());
        summaries.push(TotpSummary {
            entry_id: entry.entry_id.clone(),
            source: TotpSource::Login,
            title: title.clone(),
            issuer: if parsed.issuer.is_empty() {
                title
            } else {
                parsed.issuer
            },
            account: parsed.account,
            period: parsed.period,
            digits: parsed.digits,
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

pub(crate) fn get_totp_entry(
    conn: &VaultConnection,
    entry_id: &str,
    source: TotpSource,
) -> Result<TotpDetail, VaultError> {
    match source {
        TotpSource::Standalone => {
            let mut entry = load_entry(conn, entry_id, false)?;
            if entry.entry_type != EntryType::Totp {
                return Err(VaultError::Storage(format!(
                    "entry {entry_id} is not a totp"
                )));
            }
            let fields = TotpFields::from_entry(&mut entry, true)?;
            Ok(TotpDetail {
                entry_id: entry.entry_id,
                source,
                title: fields.title,
                issuer: fields.issuer,
                account: fields.account,
                period: fields.period,
                digits: fields.digits,
                secret: SecretString::from(fields.secret),
                updated_at: entry.updated_at,
            })
        }
        TotpSource::Login => {
            let login = get_password_entry(conn, entry_id)?;
            let parsed = parse_totp_input(login.totp_secret.expose_secret());
            Ok(TotpDetail {
                entry_id: login.entry_id,
                source,
                title: login.title.clone(),
                issuer: if parsed.issuer.is_empty() {
                    login.title
                } else {
                    parsed.issuer
                },
                account: parsed.account,
                period: parsed.period,
                digits: parsed.digits,
                secret: SecretString::from(parsed.secret),
                updated_at: login.updated_at,
            })
        }
    }
}

pub(crate) fn save_totp_entry(
    conn: &VaultConnection,
    draft: &TotpDraft,
) -> Result<TotpSummary, VaultError> {
    if draft.entry_id.is_none() || draft.source == TotpSource::Standalone {
        save_standalone(conn, draft)
    } else {
        save_login_bound(conn, draft)
    }
}

fn save_standalone(
    conn: &VaultConnection,
    draft: &TotpDraft,
) -> Result<TotpSummary, VaultError> {
    let title = if draft.title.trim().is_empty() {
        display_title(&draft.issuer, &draft.account)
    } else {
        draft.title.trim().to_string()
    };
    let period = normalize_period(draft.period);
    let digits = normalize_digits(draft.digits);
    let spec = parse_totp_input(draft.secret.expose_secret());
    let secret = if spec.secret.is_empty() {
        draft.secret.expose_secret().trim().to_string()
    } else {
        spec.secret
    };
    if secret.is_empty() {
        return Err(VaultError::Storage("请填写密钥".to_string()));
    }
    let issuer = nonempty(&draft.issuer).unwrap_or(spec.issuer);
    let account = nonempty(&draft.account).unwrap_or(spec.account);
    let item_data = json!({
        "secret": secret,
        "issuer": issuer,
        "accountName": account,
        "period": period,
        "digits": digits,
        "algorithm": "SHA1",
        "otpType": "TOTP",
        "counter": 0,
    });
    let payload = json!({
        "kind": "totp",
        "notes": "",
        "item_data": item_data.to_string(),
        "image_paths": "[]",
        "archived": false,
    });
    let saved = save_json_entry(
        conn,
        draft.entry_id.as_deref(),
        EntryType::Totp,
        &title,
        &payload,
    )?;
    Ok(TotpSummary {
        entry_id: saved.entry_id,
        source: TotpSource::Standalone,
        title,
        issuer,
        account,
        period,
        digits,
        updated_at: saved.updated_at,
    })
}

fn save_login_bound(
    conn: &VaultConnection,
    draft: &TotpDraft,
) -> Result<TotpSummary, VaultError> {
    let entry_id = draft
        .entry_id
        .as_deref()
        .ok_or_else(|| VaultError::Storage("登录项缺少编号".to_string()))?;
    let mut login = crate::password::get_password_entry(conn, entry_id)?;
    let period = normalize_period(draft.period);
    let digits = normalize_digits(draft.digits);
    let spec = parse_totp_input(draft.secret.expose_secret());
    let secret = if spec.secret.is_empty() {
        draft.secret.expose_secret().trim().to_string()
    } else {
        spec.secret
    };
    login.totp_secret = SecretString::from(secret.clone());
    let saved = crate::password::save_password_entry(
        conn,
        &crate::password::PasswordEntryDraft {
            entry_id: Some(login.entry_id.clone()),
            title: login.title.clone(),
            username: login.username.clone(),
            url: login.url.clone(),
            notes: login.notes.clone(),
            password: login.password.clone(),
            totp_secret: SecretString::from(secret),
            archived: login.archived,
        },
    )?;
    Ok(TotpSummary {
        entry_id: saved.entry_id,
        source: TotpSource::Login,
        title: saved.title,
        issuer: draft.issuer.clone(),
        account: draft.account.clone(),
        period,
        digits,
        updated_at: saved.updated_at,
    })
}

pub(crate) fn delete_totp_entry(
    conn: &VaultConnection,
    entry_id: &str,
    source: TotpSource,
) -> Result<(), VaultError> {
    match source {
        TotpSource::Standalone => soft_delete_entry(conn, entry_id),
        TotpSource::Login => {
            let login = crate::password::get_password_entry(conn, entry_id)?;
            crate::password::save_password_entry(
                conn,
                &crate::password::PasswordEntryDraft {
                    entry_id: Some(login.entry_id),
                    title: login.title,
                    username: login.username,
                    url: login.url,
                    notes: login.notes,
                    password: login.password,
                    totp_secret: SecretString::from(String::new()),
                    archived: login.archived,
                },
            )?;
            Ok(())
        }
    }
}

pub fn totp_now(secret: &str, period: u32, digits: u32) -> TotpCode {
    totp_at(secret, unix_now(), period, digits)
}

pub fn totp_at(secret: &str, unix: u64, period: u32, digits: u32) -> TotpCode {
    let period = normalize_period(period);
    let digits = normalize_digits(digits);
    let remaining = period - (unix % u64::from(period)) as u32;
    let Some(key) = decode_secret(secret) else {
        return TotpCode {
            code: "------".to_string(),
            remaining,
            period,
        };
    };
    let counter = unix / u64::from(period);
    let code = hotp(&key, counter, digits);
    TotpCode {
        code,
        remaining,
        period,
    }
}

fn is_archived_entry(entry: &mut mdbx_core::model::Entry) -> bool {
    let mut clone_payload = entry.payload_ct.clone();
    let value = serde_json::from_slice::<Value>(&clone_payload).unwrap_or(Value::Null);
    clone_payload.clear();
    is_archived(&value)
}

impl TotpFields {
    fn from_entry(entry: &mut mdbx_core::model::Entry, include_secret: bool) -> Result<Self, VaultError> {
        let value = take_payload_json(entry);
        let nested = nested_item_data(&value);
        let raw_secret = json_string(
            &nested,
            &["secret", "key", "authenticator_key", "authenticatorKey"],
        );
        let parsed = parse_totp_input(&raw_secret);
        let issuer = first_nonempty(&[
            json_string(&nested, &["issuer", "serviceName"]),
            parsed.issuer.clone(),
        ]);
        let account = first_nonempty(&[
            json_string(&nested, &["accountName", "account", "account_name"]),
            parsed.account.clone(),
        ]);
        let title = title_from_bytes(entry.title_ct.as_deref());
        Ok(Self {
            title,
            issuer,
            account,
            period: json_u32(&nested, &["period"], parsed.period),
            digits: json_u32(&nested, &["digits"], parsed.digits),
            secret: if include_secret {
                if parsed.secret.is_empty() {
                    raw_secret
                } else {
                    parsed.secret
                }
            } else {
                String::new()
            },
        })
    }
}

struct ParsedTotp {
    secret: String,
    issuer: String,
    account: String,
    period: u32,
    digits: u32,
}

fn parse_totp_input(raw: &str) -> ParsedTotp {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ParsedTotp {
            secret: String::new(),
            issuer: String::new(),
            account: String::new(),
            period: DEFAULT_PERIOD,
            digits: DEFAULT_DIGITS,
        };
    }
    if let Some(parsed) = parse_otpauth(trimmed) {
        return parsed;
    }
    ParsedTotp {
        secret: normalize_secret(trimmed),
        issuer: String::new(),
        account: String::new(),
        period: DEFAULT_PERIOD,
        digits: DEFAULT_DIGITS,
    }
}

fn parse_otpauth(raw: &str) -> Option<ParsedTotp> {
    let rest = raw.strip_prefix("otpauth://")?;
    let (kind, rest) = rest.split_once('/')?;
    if !kind.eq_ignore_ascii_case("totp") && !kind.eq_ignore_ascii_case("hotp") {
        return None;
    }
    let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
    let label = percent_decode(label);
    let mut issuer = String::new();
    let mut account = label.clone();
    if let Some((left, right)) = label.split_once(':') {
        issuer = left.to_string();
        account = right.to_string();
    }
    let mut secret = String::new();
    let mut period = DEFAULT_PERIOD;
    let mut digits = DEFAULT_DIGITS;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(value);
        match key {
            "secret" => secret = normalize_secret(&value),
            "issuer" if issuer.is_empty() || true => {
                if !value.is_empty() {
                    issuer = value;
                }
            }
            "period" => {
                if let Ok(parsed) = value.parse() {
                    period = parsed;
                }
            }
            "digits" => {
                if let Ok(parsed) = value.parse() {
                    digits = parsed;
                }
            }
            _ => {}
        }
    }
    Some(ParsedTotp {
        secret,
        issuer,
        account,
        period: normalize_period(period),
        digits: normalize_digits(digits),
    })
}

fn normalize_secret(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

fn decode_secret(secret: &str) -> Option<Vec<u8>> {
    let cleaned = normalize_secret(secret);
    if cleaned.is_empty() {
        return None;
    }
    let mut padded = cleaned;
    while padded.len() % 8 != 0 {
        padded.push('=');
    }
    BASE32.decode(padded.as_bytes()).ok()
}

fn hotp(key: &[u8], counter: u64, digits: u32) -> String {
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(key) else {
        return "------".to_string();
    };
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    let bin = ((u32::from(result[offset]) & 0x7f) << 24)
        | (u32::from(result[offset + 1]) << 16)
        | (u32::from(result[offset + 2]) << 8)
        | u32::from(result[offset + 3]);
    let wrap = 10_u32.pow(digits);
    format!("{:0width$}", bin % wrap, width = digits as usize)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn normalize_period(period: u32) -> u32 {
    if period == 0 {
        DEFAULT_PERIOD
    } else {
        period.clamp(10, 120)
    }
}

fn normalize_digits(digits: u32) -> u32 {
    if digits == 0 {
        DEFAULT_DIGITS
    } else {
        digits.clamp(6, 8)
    }
}

fn display_title(issuer: &str, account: &str) -> String {
    match (issuer.trim(), account.trim()) {
        ("", "") => "动态口令".to_string(),
        (issuer, "") => issuer.to_string(),
        ("", account) => account.to_string(),
        (issuer, account) => format!("{issuer} · {account}"),
    }
}

fn nonempty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn first_nonempty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_sha1_8_digits() {
        // ASCII key "12345678901234567890" as Base32.
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let code = totp_at(secret, 59, 30, 8);
        assert_eq!(code.code, "94287082");
        assert_eq!(code.period, 30);
        assert_eq!(code.remaining, 1);
    }

    #[test]
    fn otpauth_uri_roundtrip_fields() {
        let parsed = parse_otpauth(
            "otpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP&period=30&digits=6&issuer=GitHub",
        )
        .expect("uri");
        assert_eq!(parsed.issuer, "GitHub");
        assert_eq!(parsed.account, "ada");
        assert_eq!(parsed.secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(parsed.period, 30);
        assert_eq!(parsed.digits, 6);
    }

    #[test]
    fn invalid_secret_returns_placeholder() {
        let code = totp_now("***", 30, 6);
        assert_eq!(code.code, "------");
    }
}
