//! TOTP entries (`EntryType::Totp`) plus codes for login-bound authenticator keys.

use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE32;
use hmac::{Hmac, Mac};
use mdbx_core::model::EntryType;
use mdbx_storage::connection::VaultConnection;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

use crate::io::{list_by_type, load_entry, save_json_entry, soft_delete_entry};
use crate::payload::{
    is_archived, json_string, json_u32, json_u64, nested_item_data, take_payload_json,
    title_from_bytes,
};
use crate::password::get_password_entry;
use crate::VaultError;

const DEFAULT_PERIOD: u32 = 30;
const DEFAULT_DIGITS: u32 = 6;
const STEAM_ALPHABET: &[u8] = b"23456789BCDFGHJKMNPQRTVWXY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpSource {
    Standalone,
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl TotpAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().replace(['-', '_'], "").as_str() {
            "SHA256" | "SHA2" => Self::Sha256,
            "SHA512" => Self::Sha512,
            _ => Self::Sha1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtpType {
    Totp,
    Hotp,
    Steam,
}

impl OtpType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Totp => "TOTP",
            Self::Hotp => "HOTP",
            Self::Steam => "STEAM",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "HOTP" => Self::Hotp,
            "STEAM" => Self::Steam,
            _ => Self::Totp,
        }
    }
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
    pub algorithm: TotpAlgorithm,
    pub otp_type: OtpType,
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
    pub algorithm: TotpAlgorithm,
    pub otp_type: OtpType,
    pub counter: u64,
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
    pub algorithm: TotpAlgorithm,
    pub otp_type: OtpType,
    pub counter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpCode {
    pub code: String,
    pub remaining: u32,
    pub period: u32,
}

/// Parsed authenticator secret or otpauth URI (login + standalone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpSpec {
    pub secret: String,
    pub issuer: String,
    pub account: String,
    pub period: u32,
    pub digits: u32,
    pub algorithm: TotpAlgorithm,
    pub otp_type: OtpType,
    pub counter: u64,
}

struct TotpFields {
    title: String,
    issuer: String,
    account: String,
    period: u32,
    digits: u32,
    algorithm: TotpAlgorithm,
    otp_type: OtpType,
    counter: u64,
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
            algorithm: fields.algorithm,
            otp_type: fields.otp_type,
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
        let parsed = parse_totp_spec(&raw);
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
            algorithm: parsed.algorithm,
            otp_type: parsed.otp_type,
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
                algorithm: fields.algorithm,
                otp_type: fields.otp_type,
                counter: fields.counter,
                secret: SecretString::from(fields.secret),
                updated_at: entry.updated_at,
            })
        }
        TotpSource::Login => {
            let login = get_password_entry(conn, entry_id)?;
            let parsed = parse_totp_spec(login.totp_secret.expose_secret());
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
                algorithm: parsed.algorithm,
                otp_type: parsed.otp_type,
                counter: parsed.counter,
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
    let spec = resolve_draft(draft);
    if spec.secret.is_empty() {
        return Err(VaultError::Storage("请填写密钥".to_string()));
    }
    let issuer = nonempty(&draft.issuer).unwrap_or(spec.issuer.clone());
    let account = nonempty(&draft.account).unwrap_or(spec.account.clone());
    let item_data = json!({
        "secret": spec.secret,
        "issuer": issuer,
        "accountName": account,
        "period": spec.period,
        "digits": spec.digits,
        "algorithm": spec.algorithm.as_str(),
        "otpType": spec.otp_type.as_str(),
        "counter": spec.counter,
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
        period: spec.period,
        digits: spec.digits,
        algorithm: spec.algorithm,
        otp_type: spec.otp_type,
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
    let login = crate::password::get_password_entry(conn, entry_id)?;
    let spec = resolve_draft(draft);
    let encoded = encode_authenticator_key(&spec);
    let saved = crate::password::save_password_entry(
        conn,
        &crate::password::PasswordEntryDraft {
            entry_id: Some(login.entry_id.clone()),
            title: login.title.clone(),
            username: login.username.clone(),
            url: login.url.clone(),
            notes: login.notes.clone(),
            password: login.password.clone(),
            totp_secret: SecretString::from(encoded),
            project_id: Some(login.project_id),
            archived: login.archived,
        },
    )?;
    Ok(TotpSummary {
        entry_id: saved.entry_id,
        source: TotpSource::Login,
        title: saved.title,
        issuer: draft.issuer.clone(),
        account: draft.account.clone(),
        period: spec.period,
        digits: spec.digits,
        algorithm: spec.algorithm,
        otp_type: spec.otp_type,
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
                    project_id: Some(login.project_id),
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
    generate_code(
        secret,
        unix,
        period,
        digits,
        TotpAlgorithm::Sha1,
        OtpType::Totp,
        0,
    )
}

pub fn totp_from_input(raw: &str) -> TotpCode {
    let spec = parse_totp_spec(raw);
    generate_code(
        &spec.secret,
        unix_now(),
        spec.period,
        spec.digits,
        spec.algorithm,
        spec.otp_type,
        spec.counter,
    )
}

pub fn totp_now_ex(
    secret: &str,
    period: u32,
    digits: u32,
    algorithm: TotpAlgorithm,
    otp_type: OtpType,
    counter: u64,
) -> TotpCode {
    generate_code(
        secret,
        unix_now(),
        period,
        digits,
        algorithm,
        otp_type,
        counter,
    )
}

pub fn parse_totp_spec(raw: &str) -> TotpSpec {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return TotpSpec {
            secret: String::new(),
            issuer: String::new(),
            account: String::new(),
            period: DEFAULT_PERIOD,
            digits: DEFAULT_DIGITS,
            algorithm: TotpAlgorithm::Sha1,
            otp_type: OtpType::Totp,
            counter: 0,
        };
    }
    if let Some(parsed) = parse_otpauth(trimmed) {
        return parsed;
    }
    TotpSpec {
        secret: normalize_secret(trimmed),
        issuer: String::new(),
        account: String::new(),
        period: DEFAULT_PERIOD,
        digits: DEFAULT_DIGITS,
        algorithm: TotpAlgorithm::Sha1,
        otp_type: OtpType::Totp,
        counter: 0,
    }
}

pub fn encode_authenticator_key(spec: &TotpSpec) -> String {
    if spec.secret.trim().is_empty() {
        return String::new();
    }
    let kind = match spec.otp_type {
        OtpType::Hotp => "hotp",
        OtpType::Steam => "totp",
        OtpType::Totp => "totp",
    };
    let label = otpauth_label(&spec.issuer, &spec.account);
    let mut query = format!(
        "secret={}&algorithm={}&digits={}",
        percent_encode(&spec.secret),
        spec.algorithm.as_str(),
        spec.digits
    );
    if !spec.issuer.is_empty() {
        query.push_str("&issuer=");
        query.push_str(&percent_encode(&spec.issuer));
    }
    match spec.otp_type {
        OtpType::Hotp => {
            query.push_str("&counter=");
            query.push_str(&spec.counter.to_string());
        }
        OtpType::Steam => {
            query.push_str("&period=30&encoder=steam");
        }
        OtpType::Totp => {
            query.push_str("&period=");
            query.push_str(&spec.period.to_string());
        }
    }
    format!("otpauth://{kind}/{label}?{query}")
}

fn resolve_draft(draft: &TotpDraft) -> TotpSpec {
    let parsed = parse_totp_spec(draft.secret.expose_secret());
    let secret = if parsed.secret.is_empty() {
        normalize_secret(draft.secret.expose_secret())
    } else {
        parsed.secret
    };
    let otp_type = if draft.otp_type != OtpType::Totp {
        draft.otp_type
    } else {
        parsed.otp_type
    };
    let algorithm = if draft.algorithm != TotpAlgorithm::Sha1 {
        draft.algorithm
    } else if parsed.algorithm != TotpAlgorithm::Sha1 {
        parsed.algorithm
    } else {
        draft.algorithm
    };
    let period = normalize_period(if draft.period == 0 {
        parsed.period
    } else {
        draft.period
    });
    let digits = normalize_digits(
        if draft.digits == 0 {
            parsed.digits
        } else {
            draft.digits
        },
        otp_type,
    );
    let counter = if draft.counter == 0 {
        parsed.counter
    } else {
        draft.counter
    };
    TotpSpec {
        secret,
        issuer: nonempty(&draft.issuer).unwrap_or(parsed.issuer),
        account: nonempty(&draft.account).unwrap_or(parsed.account),
        period,
        digits,
        algorithm,
        otp_type,
        counter,
    }
}

fn generate_code(
    secret: &str,
    unix: u64,
    period: u32,
    digits: u32,
    algorithm: TotpAlgorithm,
    otp_type: OtpType,
    counter: u64,
) -> TotpCode {
    let period = normalize_period(period);
    let digits = normalize_digits(digits, otp_type);
    let remaining = match otp_type {
        OtpType::Hotp => 0,
        _ => period - (unix % u64::from(period)) as u32,
    };
    let Some(key) = decode_secret(secret) else {
        return TotpCode {
            code: placeholder(digits, otp_type),
            remaining,
            period,
        };
    };
    let hmac_counter = match otp_type {
        OtpType::Hotp => counter,
        _ => unix / u64::from(period),
    };
    let Some(hmac) = hmac_bytes(algorithm, &key, hmac_counter) else {
        return TotpCode {
            code: placeholder(digits, otp_type),
            remaining,
            period,
        };
    };
    let code = match otp_type {
        OtpType::Steam => steam_code(&hmac),
        _ => truncate_digits(&hmac, digits),
    };
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
    fn from_entry(
        entry: &mut mdbx_core::model::Entry,
        include_secret: bool,
    ) -> Result<Self, VaultError> {
        let value = take_payload_json(entry);
        let nested = nested_item_data(&value);
        let raw_secret = json_string(
            &nested,
            &["secret", "key", "authenticator_key", "authenticatorKey"],
        );
        let parsed = parse_totp_spec(&raw_secret);
        let issuer = first_nonempty(&[
            json_string(&nested, &["issuer", "serviceName"]),
            parsed.issuer.clone(),
        ]);
        let account = first_nonempty(&[
            json_string(&nested, &["accountName", "account", "account_name"]),
            parsed.account.clone(),
        ]);
        let title = title_from_bytes(entry.title_ct.as_deref());
        let otp_type = OtpType::parse(&first_nonempty(&[
            json_string(&nested, &["otpType", "otp_type", "type"]),
            parsed.otp_type.as_str().to_string(),
        ]));
        let algorithm = TotpAlgorithm::parse(&first_nonempty(&[
            json_string(&nested, &["algorithm"]),
            parsed.algorithm.as_str().to_string(),
        ]));
        Ok(Self {
            title,
            issuer,
            account,
            period: json_u32(&nested, &["period"], parsed.period),
            digits: json_u32(&nested, &["digits"], parsed.digits),
            algorithm,
            otp_type,
            counter: json_u64(&nested, &["counter"], parsed.counter),
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

fn parse_otpauth(raw: &str) -> Option<TotpSpec> {
    let rest = raw.strip_prefix("otpauth://")?;
    let (kind, rest) = rest.split_once('/')?;
    let mut otp_type = if kind.eq_ignore_ascii_case("hotp") {
        OtpType::Hotp
    } else if kind.eq_ignore_ascii_case("totp") || kind.eq_ignore_ascii_case("steam") {
        OtpType::Totp
    } else {
        return None;
    };
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
    let mut algorithm = TotpAlgorithm::Sha1;
    let mut counter = 0_u64;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(value);
        match key {
            "secret" => secret = normalize_secret(&value),
            "issuer" => {
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
            "algorithm" => algorithm = TotpAlgorithm::parse(&value),
            "counter" => {
                if let Ok(parsed) = value.parse() {
                    counter = parsed;
                }
            }
            "encoder" if value.eq_ignore_ascii_case("steam") => otp_type = OtpType::Steam,
            _ => {}
        }
    }
    if otp_type == OtpType::Steam {
        digits = 5;
    }
    Some(TotpSpec {
        secret,
        issuer,
        account,
        period: normalize_period(period),
        digits: normalize_digits(digits, otp_type),
        algorithm,
        otp_type,
        counter,
    })
}

fn hmac_bytes(algorithm: TotpAlgorithm, key: &[u8], counter: u64) -> Option<Vec<u8>> {
    match algorithm {
        TotpAlgorithm::Sha1 => {
            let mut mac = Hmac::<Sha1>::new_from_slice(key).ok()?;
            mac.update(&counter.to_be_bytes());
            Some(mac.finalize().into_bytes().to_vec())
        }
        TotpAlgorithm::Sha256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(key).ok()?;
            mac.update(&counter.to_be_bytes());
            Some(mac.finalize().into_bytes().to_vec())
        }
        TotpAlgorithm::Sha512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(key).ok()?;
            mac.update(&counter.to_be_bytes());
            Some(mac.finalize().into_bytes().to_vec())
        }
    }
}

fn truncate_digits(hmac: &[u8], digits: u32) -> String {
    if hmac.len() < 4 {
        return placeholder(digits, OtpType::Totp);
    }
    let offset = (hmac[hmac.len() - 1] & 0x0f) as usize;
    if offset + 3 >= hmac.len() {
        return placeholder(digits, OtpType::Totp);
    }
    let bin = ((u32::from(hmac[offset]) & 0x7f) << 24)
        | (u32::from(hmac[offset + 1]) << 16)
        | (u32::from(hmac[offset + 2]) << 8)
        | u32::from(hmac[offset + 3]);
    let wrap = 10_u32.pow(digits);
    format!("{:0width$}", bin % wrap, width = digits as usize)
}

fn steam_code(hmac: &[u8]) -> String {
    if hmac.len() < 4 {
        return "-----".to_string();
    }
    let offset = (hmac[hmac.len() - 1] & 0x0f) as usize;
    if offset + 3 >= hmac.len() {
        return "-----".to_string();
    }
    let mut full = ((u32::from(hmac[offset]) & 0x7f) << 24)
        | (u32::from(hmac[offset + 1]) << 16)
        | (u32::from(hmac[offset + 2]) << 8)
        | u32::from(hmac[offset + 3]);
    let mut out = String::with_capacity(5);
    for _ in 0..5 {
        let idx = (full % STEAM_ALPHABET.len() as u32) as usize;
        out.push(STEAM_ALPHABET[idx] as char);
        full /= STEAM_ALPHABET.len() as u32;
    }
    out
}

fn placeholder(digits: u32, otp_type: OtpType) -> String {
    if otp_type == OtpType::Steam {
        "-----".to_string()
    } else {
        "-".repeat(digits.clamp(4, 10) as usize)
    }
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

fn normalize_digits(digits: u32, otp_type: OtpType) -> u32 {
    if otp_type == OtpType::Steam {
        return 5;
    }
    if digits == 0 {
        DEFAULT_DIGITS
    } else {
        digits.clamp(4, 10)
    }
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

fn otpauth_label(issuer: &str, account: &str) -> String {
    match (issuer.trim(), account.trim()) {
        ("", "") => "Monica".to_string(),
        (issuer, "") => percent_encode(issuer),
        ("", account) => percent_encode(account),
        (issuer, account) => format!("{}:{}", percent_encode(issuer), percent_encode(account)),
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let code = totp_at(secret, 59, 30, 8);
        assert_eq!(code.code, "94287082");
        assert_eq!(code.period, 30);
        assert_eq!(code.remaining, 1);
    }

    #[test]
    fn rfc6238_sha256_8_digits() {
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";
        let code = generate_code(secret, 59, 30, 8, TotpAlgorithm::Sha256, OtpType::Totp, 0);
        assert_eq!(code.code, "46119246");
    }

    #[test]
    fn rfc6238_sha512_8_digits() {
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA";
        let code = generate_code(secret, 59, 30, 8, TotpAlgorithm::Sha512, OtpType::Totp, 0);
        assert_eq!(code.code, "90693936");
    }

    #[test]
    fn otpauth_uri_roundtrip_fields() {
        let parsed = parse_otpauth(
            "otpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP&period=30&digits=6&issuer=GitHub&algorithm=SHA256",
        )
        .expect("uri");
        assert_eq!(parsed.issuer, "GitHub");
        assert_eq!(parsed.account, "ada");
        assert_eq!(parsed.secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(parsed.period, 30);
        assert_eq!(parsed.digits, 6);
        assert_eq!(parsed.algorithm, TotpAlgorithm::Sha256);
        assert_eq!(parsed.otp_type, OtpType::Totp);
        let encoded = encode_authenticator_key(&parsed);
        let again = parse_totp_spec(&encoded);
        assert_eq!(again.algorithm, TotpAlgorithm::Sha256);
        assert_eq!(again.secret, parsed.secret);
    }

    #[test]
    fn otpauth_hotp_and_steam() {
        let hotp = parse_totp_spec(
            "otpauth://hotp/GitHub:ada?secret=JBSWY3DPEHPK3PXP&counter=7&algorithm=SHA1&digits=6",
        );
        assert_eq!(hotp.otp_type, OtpType::Hotp);
        assert_eq!(hotp.counter, 7);
        let steam = parse_totp_spec(
            "otpauth://totp/Steam:user?secret=JBSWY3DPEHPK3PXP&encoder=steam",
        );
        assert_eq!(steam.otp_type, OtpType::Steam);
        assert_eq!(steam.digits, 5);
    }

    #[test]
    fn invalid_secret_returns_placeholder() {
        let code = totp_now("***", 30, 6);
        assert_eq!(code.code, "------");
    }
}
