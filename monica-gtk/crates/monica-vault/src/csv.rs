//! CSV import / export aligned with Avalonia/Monica where the GTK vault
//! actually has fields.
//!
//! - **Password CSV**: Avalonia `ImportExportService` headers
//!   (`title,website,username,password,notes,authenticatorKey,…`). Extra
//!   Android-only columns are written empty and ignored on import.
//! - **Combined CSV** (`kind` column): GTK-native round trip for logins,
//!   notes, wallet, and standalone TOTP.
//!
//! Avalonia secure-item CSV (`Type,Title,Data,…`) is accepted on import for
//! `NOTE` / `TOTP` when `Data` is plain text (not the encoded ItemData blob).
//! Encoded wallet payloads are skipped with a warning.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use mdbx_storage::connection::VaultConnection;
use zeroize::Zeroize;

use crate::exchange::{refuse_existing, TransferSummary};
use crate::note::NoteDraft;
use crate::password::PasswordEntryDraft;
use crate::totp::{TotpDraft, TotpSource};
use crate::wallet::{WalletDraft, WalletKind};
use crate::{secret_password, VaultError};

pub const PASSWORD_CSV_FORMAT: &str = "monica-password-csv";
pub const COMBINED_CSV_FORMAT: &str = "monica-gtk-csv-v1";
const MAX_CSV_BYTES: usize = 64 * 1024 * 1024;

const PASSWORD_HEADERS: [&str; 15] = [
    "title",
    "website",
    "username",
    "password",
    "notes",
    "authenticatorKey",
    "appName",
    "appPackageName",
    "email",
    "phone",
    "loginType",
    "ssoProvider",
    "passkeyBindings",
    "wifiMetadata",
    "sshKeyData",
];

const COMBINED_HEADERS: [&str; 19] = [
    "kind",
    "title",
    "username",
    "url",
    "password",
    "notes",
    "totp_secret",
    "content",
    "tags",
    "markdown",
    "holder",
    "number",
    "extra",
    "expiry",
    "cvv",
    "issuer",
    "account",
    "secret",
    "period",
];

pub(crate) fn export_password_csv(
    conn: &VaultConnection,
    destination: &Path,
) -> Result<TransferSummary, VaultError> {
    refuse_existing(destination)?;
    let records = crate::exchange::collect_login_records(conn)?;
    let mut rows = Vec::with_capacity(records.len() + 1);
    rows.push(PASSWORD_HEADERS.iter().map(|h| (*h).to_string()).collect());
    for record in &records {
        rows.push(vec![
            record.title.clone(),
            record.url.clone(),
            record.username.clone(),
            record.password.clone(),
            record.notes.clone(),
            record.totp_secret.clone(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            "Password".to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]);
    }
    write_csv(destination, &rows)?;
    Ok(TransferSummary {
        path: destination.to_path_buf(),
        format: PASSWORD_CSV_FORMAT.to_string(),
        logins: records.len() as u32,
        notes: 0,
        wallet: 0,
        totp: 0,
        skipped: 0,
        warnings: Vec::new(),
    })
}

pub(crate) fn export_combined_csv(
    conn: &VaultConnection,
    destination: &Path,
) -> Result<TransferSummary, VaultError> {
    refuse_existing(destination)?;
    let logins = crate::exchange::collect_login_records(conn)?;
    let notes = crate::exchange::collect_note_records(conn)?;
    let wallet = crate::exchange::collect_wallet_records(conn)?;
    let totp = crate::exchange::collect_totp_records(conn)?;
    let mut rows = Vec::new();
    rows.push(COMBINED_HEADERS.iter().map(|h| (*h).to_string()).collect());
    for record in &logins {
        rows.push(combined_row(&[
            ("kind", "login"),
            ("title", &record.title),
            ("username", &record.username),
            ("url", &record.url),
            ("password", &record.password),
            ("notes", &record.notes),
            ("totp_secret", &record.totp_secret),
        ]));
    }
    for record in &notes {
        let markdown = if record.markdown { "true" } else { "false" };
        rows.push(combined_row(&[
            ("kind", "note"),
            ("title", &record.title),
            ("content", &record.content),
            ("tags", &record.tags),
            ("markdown", markdown),
        ]));
    }
    for record in &wallet {
        rows.push(combined_row(&[
            ("kind", "wallet"),
            ("title", &record.title),
            ("notes", &record.notes),
            ("holder", &record.holder),
            ("number", &record.number),
            ("extra", &record.extra),
            ("expiry", &record.expiry),
            ("cvv", &record.cvv),
            ("tags", &record.kind),
        ]));
    }
    for record in &totp {
        let period = record.period.to_string();
        let digits = record.digits.to_string();
        rows.push(combined_row(&[
            ("kind", "totp"),
            ("title", &record.title),
            ("issuer", &record.issuer),
            ("account", &record.account),
            ("secret", &record.secret),
            ("period", &period),
            ("digits", &digits),
            ("algorithm", &record.algorithm),
            ("otp_type", &record.otp_type),
        ]));
    }
    write_csv(destination, &rows)?;
    Ok(TransferSummary {
        path: destination.to_path_buf(),
        format: COMBINED_CSV_FORMAT.to_string(),
        logins: logins.len() as u32,
        notes: notes.len() as u32,
        wallet: wallet.len() as u32,
        totp: totp.len() as u32,
        skipped: 0,
        warnings: Vec::new(),
    })
}

pub(crate) fn import_csv(
    conn: &VaultConnection,
    source: &Path,
) -> Result<TransferSummary, VaultError> {
    let mut bytes = fs::read(source).map_err(|error| VaultError::Storage(error.to_string()))?;
    if bytes.len() > MAX_CSV_BYTES {
        return Err(VaultError::Storage("CSV 超过 64 MiB 限制".into()));
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        bytes.drain(..3);
    }
    let text = String::from_utf8(bytes)
        .map_err(|error| VaultError::Storage(format!("CSV 不是 UTF-8：{error}")))?;
    let (headers, records) = parse_csv(&text)?;
    if headers.is_empty() {
        return Err(VaultError::Storage("CSV 没有表头".into()));
    }
    let index = header_index(&headers);
    if has_any(&index, &["kind"]) {
        import_combined(conn, source, &index, &records)
    } else if has_any(
        &index,
        &[
            "username",
            "login_username",
            "password",
            "login_password",
            "website",
            "authenticatorKey",
        ],
    ) {
        import_password_rows(conn, source, &index, &records)
    } else if has_any(&index, &["type", "itemType", "item_type"]) {
        import_secure_item_rows(conn, source, &index, &records)
    } else {
        Err(VaultError::Storage(
            "无法识别 CSV 表头（需要 kind、Avalonia 密码列，或 Type）".into(),
        ))
    }
}

fn import_password_rows(
    conn: &VaultConnection,
    source: &Path,
    index: &HashMap<String, usize>,
    records: &[Vec<String>],
) -> Result<TransferSummary, VaultError> {
    let mut summary = empty_summary(source, PASSWORD_CSV_FORMAT);
    for row in records {
        let website = field(index, row, &["website", "url", "uri", "login_uri", "login_uri_1"]);
        let username = field(index, row, &["username", "login_username", "user", "email"]);
        let mut title = field(index, row, &["title", "name", "folder/name", "login_title"]);
        if title.trim().is_empty() {
            title = infer_title(&website, &username);
        }
        let password = field(index, row, &["password", "login_password"]);
        let notes = field(index, row, &["notes", "note"]);
        let totp = field(
            index,
            row,
            &["authenticatorKey", "totp", "login_totp", "otp", "otp_secret"],
        );
        if title.trim().is_empty() && username.trim().is_empty() && password.trim().is_empty() {
            continue;
        }
        match crate::password::save_password_entry(
            conn,
            &PasswordEntryDraft {
                entry_id: None,
                title,
                username,
                url: website,
                notes,
                password: secret_password(password),
                totp_secret: secret_password(totp),
                project_id: None,
                archived: false,
            },
        ) {
            Ok(_) => summary.logins += 1,
            Err(error) => {
                summary.skipped += 1;
                summary.warnings.push(error.to_string());
            }
        }
    }
    Ok(summary)
}

fn import_combined(
    conn: &VaultConnection,
    source: &Path,
    index: &HashMap<String, usize>,
    records: &[Vec<String>],
) -> Result<TransferSummary, VaultError> {
    let mut summary = empty_summary(source, COMBINED_CSV_FORMAT);
    for row in records {
        let kind = field(index, row, &["kind", "type"]).to_ascii_lowercase();
        let result = match kind.as_str() {
            "login" | "password" => save_login_from_combined(conn, index, row).map(|_| {
                summary.logins += 1;
            }),
            "note" => crate::note::save_note(
                conn,
                &NoteDraft {
                    entry_id: None,
                    title: field(index, row, &["title", "name"]),
                    content: field(index, row, &["content", "data", "notes"]),
                    tags: field(index, row, &["tags"]),
                    markdown: parse_bool(&field(index, row, &["markdown"])),
                },
            )
            .map(|_| {
                summary.notes += 1;
            }),
            "wallet" | "card" | "document" => crate::wallet::save_wallet(
                conn,
                &WalletDraft {
                    entry_id: None,
                    kind: if kind == "document"
                        || field(index, row, &["tags", "wallet_kind"]) == "document"
                    {
                        WalletKind::Document
                    } else {
                        WalletKind::Card
                    },
                    title: field(index, row, &["title", "name"]),
                    holder: field(index, row, &["holder"]),
                    number: secret_password(field(index, row, &["number"])),
                    extra: field(index, row, &["extra"]),
                    expiry: field(index, row, &["expiry"]),
                    cvv: secret_password(field(index, row, &["cvv"])),
                    notes: field(index, row, &["notes", "note"]),
                },
            )
            .map(|_| {
                summary.wallet += 1;
            }),
            "totp" | "otp" => crate::totp::save_totp_entry(
                conn,
                &TotpDraft {
                    entry_id: None,
                    source: TotpSource::Standalone,
                    title: field(index, row, &["title", "name"]),
                    issuer: field(index, row, &["issuer"]),
                    account: field(index, row, &["account"]),
                    secret: secret_password(field(index, row, &["secret", "totp_secret", "otp"])),
                    period: field(index, row, &["period"])
                        .parse::<u32>()
                        .unwrap_or(30)
                        .max(1),
                    digits: field(index, row, &["digits"])
                        .parse::<u32>()
                        .unwrap_or(6),
                    algorithm: crate::totp::TotpAlgorithm::parse(&field(
                        index,
                        row,
                        &["algorithm"],
                    )),
                    otp_type: crate::totp::OtpType::parse(&field(index, row, &["otp_type", "type"])),
                    counter: field(index, row, &["counter"])
                        .parse::<u64>()
                        .unwrap_or(0),
                },
            )
            .map(|_| {
                summary.totp += 1;
            }),
            "" => continue,
            other => {
                summary.skipped += 1;
                summary.warnings.push(format!("未知 kind：{other}"));
                continue;
            }
        };
        if let Err(error) = result {
            summary.skipped += 1;
            summary.warnings.push(error.to_string());
        }
    }
    Ok(summary)
}

fn save_login_from_combined(
    conn: &VaultConnection,
    index: &HashMap<String, usize>,
    row: &[String],
) -> Result<(), VaultError> {
    crate::password::save_password_entry(
        conn,
        &PasswordEntryDraft {
            entry_id: None,
            title: field(index, row, &["title", "name"]),
            username: field(index, row, &["username", "user"]),
            url: field(index, row, &["url", "website"]),
            notes: field(index, row, &["notes", "note"]),
            password: secret_password(field(index, row, &["password"])),
            totp_secret: secret_password(field(index, row, &["totp_secret", "authenticatorKey"])),
            project_id: None,
            archived: false,
        },
    )?;
    Ok(())
}

fn import_secure_item_rows(
    conn: &VaultConnection,
    source: &Path,
    index: &HashMap<String, usize>,
    records: &[Vec<String>],
) -> Result<TransferSummary, VaultError> {
    let mut summary = empty_summary(source, "avalonia-secure-item-csv");
    for row in records {
        let item_type = field(index, row, &["type", "itemType", "item_type"]).to_ascii_uppercase();
        let title = field(index, row, &["title", "name"]);
        let data = field(index, row, &["data", "itemData", "item_data"]);
        let notes = field(index, row, &["notes", "note"]);
        let result = match item_type.as_str() {
            "NOTE" => crate::note::save_note(
                conn,
                &NoteDraft {
                    entry_id: None,
                    title,
                    content: if looks_encoded(&data) {
                        notes.clone()
                    } else if data.trim().is_empty() {
                        notes.clone()
                    } else {
                        data.clone()
                    },
                    tags: String::new(),
                    markdown: false,
                },
            )
            .map(|_| {
                summary.notes += 1;
            }),
            "TOTP" => {
                let secret = if looks_plain_totp(&data) {
                    data.clone()
                } else {
                    String::new()
                };
                if secret.trim().is_empty() {
                    summary.skipped += 1;
                    summary
                        .warnings
                        .push("跳过编码后的 Avalonia TOTP Data".into());
                    continue;
                }
                crate::totp::save_totp_entry(
                    conn,
                    &TotpDraft {
                        entry_id: None,
                        source: TotpSource::Standalone,
                        title,
                        issuer: String::new(),
                        account: String::new(),
                        secret: secret_password(secret),
                        period: 30,
                        digits: 6,
                        algorithm: crate::totp::TotpAlgorithm::Sha1,
                        otp_type: crate::totp::OtpType::Totp,
                        counter: 0,
                    },
                )
                .map(|_| {
                    summary.totp += 1;
                })
            }
            "BANK_CARD" | "DOCUMENT" | "BILLING_ADDRESS" | "PAYMENT_ACCOUNT" => {
                summary.skipped += 1;
                summary
                    .warnings
                    .push(format!("跳过 Avalonia 编码钱包行：{item_type}"));
                continue;
            }
            "" => continue,
            other => {
                summary.skipped += 1;
                summary.warnings.push(format!("未知 Type：{other}"));
                continue;
            }
        };
        if let Err(error) = result {
            summary.skipped += 1;
            summary.warnings.push(error.to_string());
        }
    }
    Ok(summary)
}

fn combined_row(pairs: &[(&str, &str)]) -> Vec<String> {
    let mut map = HashMap::new();
    for (key, value) in pairs {
        map.insert(*key, (*value).to_string());
    }
    COMBINED_HEADERS
        .iter()
        .map(|header| map.remove(*header).unwrap_or_default())
        .collect()
}

fn empty_summary(source: &Path, format: &str) -> TransferSummary {
    TransferSummary {
        path: source.to_path_buf(),
        format: format.to_string(),
        logins: 0,
        notes: 0,
        wallet: 0,
        totp: 0,
        skipped: 0,
        warnings: Vec::new(),
    }
}

fn header_index(headers: &[String]) -> HashMap<String, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.trim().to_ascii_lowercase(), i))
        .collect()
}

fn has_any(index: &HashMap<String, usize>, names: &[&str]) -> bool {
    names.iter().any(|name| index.contains_key(&name.to_ascii_lowercase()))
}

fn field(index: &HashMap<String, usize>, row: &[String], names: &[&str]) -> String {
    for name in names {
        if let Some(idx) = index.get(&name.to_ascii_lowercase()) {
            if let Some(value) = row.get(*idx) {
                return value.trim().to_string();
            }
        }
    }
    String::new()
}

fn infer_title(website: &str, username: &str) -> String {
    let website = website.trim();
    let rest = website
        .strip_prefix("https://")
        .or_else(|| website.strip_prefix("http://"))
        .unwrap_or(website);
    let host = rest.split('/').next().unwrap_or(rest);
    if !host.is_empty() && host != website && host.contains('.') {
        return host.to_string();
    }
    if !website.is_empty() {
        return website.to_string();
    }
    if username.trim().is_empty() {
        "Imported password".to_string()
    } else {
        username.to_string()
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

fn looks_encoded(data: &str) -> bool {
    let trimmed = data.trim();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn looks_plain_totp(data: &str) -> bool {
    let trimmed = data.trim();
    if trimmed.is_empty() || looks_encoded(trimmed) {
        return false;
    }
    trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'=' || b == b' ')
        && trimmed.len() >= 8
}

fn write_csv(path: &Path, rows: &[Vec<String>]) -> Result<(), VaultError> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&row.iter().map(|cell| escape_csv(cell)).collect::<Vec<_>>().join(","));
        text.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| VaultError::Storage(error.to_string()))?;
    }
    fs::write(path, text.as_bytes()).map_err(|error| VaultError::Storage(error.to_string()))?;
    text.zeroize();
    Ok(())
}

fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        let mut out = String::from("\"");
        for ch in value.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
        out
    } else {
        value.to_string()
    }
}

fn parse_csv(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>), VaultError> {
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' => in_quotes = true,
            ',' => {
                current.push(std::mem::take(&mut field));
            }
            '\n' => {
                current.push(std::mem::take(&mut field));
                if current.iter().any(|cell| !cell.trim().is_empty()) {
                    rows.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            '\r' => {}
            _ => field.push(ch),
        }
    }
    if in_quotes {
        return Err(VaultError::Storage("CSV 引号未闭合".into()));
    }
    if !field.is_empty() || !current.is_empty() {
        current.push(field);
        if current.iter().any(|cell| !cell.trim().is_empty()) {
            rows.push(current);
        }
    }
    if rows.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let headers = rows.remove(0);
    Ok((headers, rows))
}

#[cfg(test)]
mod tests {
    use super::{escape_csv, parse_csv};

    #[test]
    fn round_trips_quoted_commas() {
        let row = vec![
            "title".to_string(),
            "hello, world".to_string(),
            "say \"hi\"".to_string(),
        ];
        let line = row.iter().map(|c| escape_csv(c)).collect::<Vec<_>>().join(",");
        let text = format!("a,b,c\n{line}\n");
        let (headers, records) = parse_csv(&text).expect("parse");
        assert_eq!(headers, ["a", "b", "c"]);
        assert_eq!(records[0][1], "hello, world");
        assert_eq!(records[0][2], "say \"hi\"");
    }

    #[test]
    fn skips_blank_lines() {
        let (headers, records) = parse_csv("title,username\n\nAcme,ada\n").expect("parse");
        assert_eq!(headers, ["title", "username"]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0][0], "Acme");
    }
}
