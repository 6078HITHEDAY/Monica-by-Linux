//! Import / export of formats this crate can actually produce and consume.
//!
//! - **Monica JSON** (`monica-gtk-export-v1`): logins, notes, wallet, standalone TOTP.
//!   Plaintext secrets. Round-trips through [`VaultSession`] CRUD into the default project.
//! - **KDBX JSON**: `Vec<KdbxEntry>` as used by `mdbx-cli import-kdbx-json` /
//!   `export-kdbx-json`. Import uses upstream [`KdbxImporter::import_entries_atomic`]
//!   (one project per KDBX entry). Export walks login entries, **not**
//!   `KdbxExporter::export_all`, because GTK stores many logins in one default project.
//! - **Binary `.kdbx`**: upstream `KdbxBinaryAdapter` (`keepass` crate) with a
//!   `SecretString` file password. Vault I/O stays off the GTK thread.
//! - **CSV**: Avalonia password headers plus a GTK combined `kind` sheet; see `csv.rs`.
//!
//! Not implemented here: Bitwarden JSON, online sync transports.

use std::fs;
use std::path::{Path, PathBuf};

use mdbx_storage::connection::VaultConnection;
use mdbx_storage::import::{KdbxBinaryAdapter, KdbxBinaryLimits, KdbxEntry, KdbxImporter};
use mdbx_storage::repo::{CommitContext, OperationExecution};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::note::NoteDraft;
use crate::password::{list_password_entries, PasswordEntryDraft};
use crate::totp::{list_totp_entries, TotpDraft, TotpSource};
use crate::wallet::{WalletDraft, WalletKind};
use crate::{secret_password, storage_error, VaultError, DEVICE_ID};

pub const MONICA_JSON_FORMAT: &str = "monica-gtk-export-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSummary {
    pub path: PathBuf,
    pub format: String,
    pub logins: u32,
    pub notes: u32,
    pub wallet: u32,
    pub totp: u32,
    pub skipped: u32,
    pub warnings: Vec<String>,
}

impl TransferSummary {
    pub fn short_status(&self) -> String {
        format!(
            "{}：登录 {} · 笔记 {} · 钱包 {} · 口令 {} · 跳过 {}",
            self.format, self.logins, self.notes, self.wallet, self.totp, self.skipped
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MonicaDocument {
    format: String,
    exported_at: String,
    vault_id: String,
    #[serde(default)]
    logins: Vec<LoginRecord>,
    #[serde(default)]
    notes: Vec<NoteRecord>,
    #[serde(default)]
    wallet: Vec<WalletRecord>,
    #[serde(default)]
    totp: Vec<TotpRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct LoginRecord {
    pub(crate) title: String,
    pub(crate) username: String,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) notes: String,
    pub(crate) password: String,
    #[serde(default)]
    pub(crate) totp_secret: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NoteRecord {
    pub(crate) title: String,
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) tags: String,
    #[serde(default)]
    pub(crate) markdown: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WalletRecord {
    pub(crate) kind: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) holder: String,
    pub(crate) number: String,
    #[serde(default)]
    pub(crate) extra: String,
    #[serde(default)]
    pub(crate) expiry: String,
    #[serde(default)]
    pub(crate) cvv: String,
    #[serde(default)]
    pub(crate) notes: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TotpRecord {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) issuer: String,
    #[serde(default)]
    pub(crate) account: String,
    pub(crate) secret: String,
    #[serde(default = "default_period")]
    pub(crate) period: u32,
    #[serde(default = "default_digits")]
    pub(crate) digits: u32,
}

fn default_period() -> u32 {
    30
}

fn default_digits() -> u32 {
    6
}

pub(crate) fn export_monica_json(
    conn: &VaultConnection,
    vault_id: &str,
    destination: &Path,
) -> Result<TransferSummary, VaultError> {
    refuse_existing(destination)?;
    let logins = collect_login_records(conn)?;
    let notes = collect_note_records(conn)?;
    let wallet = collect_wallet_records(conn)?;
    let totp = collect_totp_records(conn)?;
    let document = MonicaDocument {
        format: MONICA_JSON_FORMAT.to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        vault_id: vault_id.to_string(),
        logins,
        notes,
        wallet,
        totp,
    };
    let summary = TransferSummary {
        path: destination.to_path_buf(),
        format: MONICA_JSON_FORMAT.to_string(),
        logins: document.logins.len() as u32,
        notes: document.notes.len() as u32,
        wallet: document.wallet.len() as u32,
        totp: document.totp.len() as u32,
        skipped: 0,
        warnings: Vec::new(),
    };
    write_json(destination, &document)?;
    Ok(summary)
}

pub(crate) fn import_monica_json(
    conn: &VaultConnection,
    source: &Path,
) -> Result<TransferSummary, VaultError> {
    let bytes = fs::read(source).map_err(|error| VaultError::Storage(error.to_string()))?;
    let document: MonicaDocument = serde_json::from_slice(&bytes).map_err(|error| {
        VaultError::Storage(format!("无法解析 Monica JSON：{error}"))
    })?;
    if document.format != MONICA_JSON_FORMAT {
        return Err(VaultError::Storage(format!(
            "不支持的导出格式：{}",
            document.format
        )));
    }
    let mut summary = TransferSummary {
        path: source.to_path_buf(),
        format: MONICA_JSON_FORMAT.to_string(),
        logins: 0,
        notes: 0,
        wallet: 0,
        totp: 0,
        skipped: 0,
        warnings: Vec::new(),
    };
    for record in document.logins {
        match crate::password::save_password_entry(
            conn,
            &PasswordEntryDraft {
                entry_id: None,
                title: record.title,
                username: record.username,
                url: record.url,
                notes: record.notes,
                password: secret_password(record.password),
                totp_secret: secret_password(record.totp_secret),
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
    for record in document.notes {
        match crate::note::save_note(
            conn,
            &NoteDraft {
                entry_id: None,
                title: record.title,
                content: record.content,
                tags: record.tags,
                markdown: record.markdown,
            },
        ) {
            Ok(_) => summary.notes += 1,
            Err(error) => {
                summary.skipped += 1;
                summary.warnings.push(error.to_string());
            }
        }
    }
    for record in document.wallet {
        let kind = match record.kind.as_str() {
            "document" | "document-ref" => WalletKind::Document,
            _ => WalletKind::Card,
        };
        match crate::wallet::save_wallet(
            conn,
            &WalletDraft {
                entry_id: None,
                kind,
                title: record.title,
                holder: record.holder,
                number: secret_password(record.number),
                extra: record.extra,
                expiry: record.expiry,
                cvv: secret_password(record.cvv),
                notes: record.notes,
            },
        ) {
            Ok(_) => summary.wallet += 1,
            Err(error) => {
                summary.skipped += 1;
                summary.warnings.push(error.to_string());
            }
        }
    }
    for record in document.totp {
        match crate::totp::save_totp_entry(
            conn,
            &TotpDraft {
                entry_id: None,
                source: TotpSource::Standalone,
                title: record.title,
                issuer: record.issuer,
                account: record.account,
                secret: secret_password(record.secret),
                period: record.period.max(1),
                digits: record.digits.clamp(6, 8),
            },
        ) {
            Ok(_) => summary.totp += 1,
            Err(error) => {
                summary.skipped += 1;
                summary.warnings.push(error.to_string());
            }
        }
    }
    Ok(summary)
}

pub(crate) fn export_kdbx_json(
    conn: &VaultConnection,
    destination: &Path,
) -> Result<TransferSummary, VaultError> {
    refuse_existing(destination)?;
    let records = collect_login_records(conn)?;
    let mut entries = Vec::with_capacity(records.len());
    for record in &records {
        entries.push(login_to_kdbx(record));
    }
    write_json(destination, &entries)?;
    Ok(TransferSummary {
        path: destination.to_path_buf(),
        format: "kdbx-json".to_string(),
        logins: entries.len() as u32,
        notes: 0,
        wallet: 0,
        totp: 0,
        skipped: 0,
        warnings: Vec::new(),
    })
}

pub(crate) fn import_kdbx_json(
    conn: &VaultConnection,
    source: &Path,
) -> Result<TransferSummary, VaultError> {
    let bytes = fs::read(source).map_err(|error| VaultError::Storage(error.to_string()))?;
    let entries: Vec<KdbxEntry> = serde_json::from_slice(&bytes).map_err(|error| {
        VaultError::Storage(format!("无法解析 KDBX JSON：{error}"))
    })?;
    if entries.is_empty() {
        return Err(VaultError::Storage("KDBX JSON 没有条目".to_string()));
    }
    import_kdbx_entries(conn, source, "kdbx-json", &entries)
}

pub(crate) fn export_kdbx_binary(
    conn: &VaultConnection,
    destination: &Path,
    password: &SecretString,
) -> Result<TransferSummary, VaultError> {
    refuse_existing(destination)?;
    if password.expose_secret().is_empty() {
        return Err(VaultError::Storage("KDBX 文件密码不能为空".into()));
    }
    let records = collect_login_records(conn)?;
    let entries: Vec<KdbxEntry> = records.iter().map(login_to_kdbx).collect();
    let mut bytes = KdbxBinaryAdapter::encode(
        &entries,
        password.expose_secret(),
        KdbxBinaryLimits::default(),
    )
    .map_err(storage_error)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| VaultError::Storage(error.to_string()))?;
    }
    fs::write(destination, &bytes).map_err(|error| VaultError::Storage(error.to_string()))?;
    bytes.zeroize();
    Ok(TransferSummary {
        path: destination.to_path_buf(),
        format: "kdbx".to_string(),
        logins: entries.len() as u32,
        notes: 0,
        wallet: 0,
        totp: 0,
        skipped: 0,
        warnings: Vec::new(),
    })
}

pub(crate) fn import_kdbx_binary(
    conn: &VaultConnection,
    source: &Path,
    password: &SecretString,
) -> Result<TransferSummary, VaultError> {
    if password.expose_secret().is_empty() {
        return Err(VaultError::Storage("KDBX 文件密码不能为空".into()));
    }
    let file = fs::File::open(source).map_err(|error| VaultError::Storage(error.to_string()))?;
    let mut reader = file;
    let document = KdbxBinaryAdapter::decode(
        &mut reader,
        password.expose_secret(),
        KdbxBinaryLimits::default(),
    )
    .map_err(storage_error)?;
    if document.entries.is_empty() {
        return Err(VaultError::Storage("KDBX 没有条目".into()));
    }
    import_kdbx_entries(conn, source, "kdbx", &document.entries)
}

fn import_kdbx_entries(
    conn: &VaultConnection,
    source: &Path,
    format: &str,
    entries: &[KdbxEntry],
) -> Result<TransferSummary, VaultError> {
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let execution = KdbxImporter::import_entries_atomic(conn, &ctx, fresh_id("kdbx-import"), entries)
        .map_err(storage_error)?;
    let (imported, warnings, skipped) = match execution {
        OperationExecution::Applied { value, .. } => (
            value.entries_created,
            value.warnings,
            value.entries_skipped,
        ),
        OperationExecution::AlreadyCommitted { commit_id } => {
            return Ok(TransferSummary {
                path: source.to_path_buf(),
                format: format.to_string(),
                logins: 0,
                notes: 0,
                wallet: 0,
                totp: 0,
                skipped: 0,
                warnings: vec![format!("该导入已提交：{commit_id}")],
            });
        }
    };
    Ok(TransferSummary {
        path: source.to_path_buf(),
        format: format.to_string(),
        logins: imported,
        notes: 0,
        wallet: 0,
        totp: 0,
        skipped,
        warnings,
    })
}

pub(crate) fn collect_login_records(conn: &VaultConnection) -> Result<Vec<LoginRecord>, VaultError> {
    let listed = list_password_entries(conn)?;
    let mut records = Vec::with_capacity(listed.len());
    for summary in listed {
        let detail = crate::password::get_password_entry(conn, &summary.entry_id)?;
        records.push(LoginRecord {
            title: detail.title,
            username: detail.username,
            url: detail.url,
            notes: detail.notes,
            password: detail.password.expose_secret().to_string(),
            totp_secret: detail.totp_secret.expose_secret().to_string(),
        });
    }
    Ok(records)
}

pub(crate) fn collect_note_records(conn: &VaultConnection) -> Result<Vec<NoteRecord>, VaultError> {
    let listed = crate::note::list_notes(conn)?;
    let mut records = Vec::with_capacity(listed.len());
    for summary in listed {
        let detail = crate::note::get_note(conn, &summary.entry_id)?;
        records.push(NoteRecord {
            title: detail.title,
            content: detail.content,
            tags: detail.tags,
            markdown: detail.markdown,
        });
    }
    Ok(records)
}

pub(crate) fn collect_wallet_records(conn: &VaultConnection) -> Result<Vec<WalletRecord>, VaultError> {
    let listed = crate::wallet::list_wallet(conn)?;
    let mut records = Vec::with_capacity(listed.len());
    for summary in listed {
        let detail = crate::wallet::get_wallet(conn, &summary.entry_id)?;
        records.push(WalletRecord {
            kind: match detail.kind {
                WalletKind::Card => "card".to_string(),
                WalletKind::Document => "document".to_string(),
            },
            title: detail.title,
            holder: detail.holder,
            number: detail.number.expose_secret().to_string(),
            extra: detail.extra,
            expiry: detail.expiry,
            cvv: detail.cvv.expose_secret().to_string(),
            notes: detail.notes,
        });
    }
    Ok(records)
}

pub(crate) fn collect_totp_records(conn: &VaultConnection) -> Result<Vec<TotpRecord>, VaultError> {
    let listed = list_totp_entries(conn)?;
    let mut records = Vec::new();
    for summary in listed {
        if summary.source != TotpSource::Standalone {
            continue;
        }
        let detail = crate::totp::get_totp_entry(conn, &summary.entry_id, TotpSource::Standalone)?;
        records.push(TotpRecord {
            title: detail.title,
            issuer: detail.issuer,
            account: detail.account,
            secret: detail.secret.expose_secret().to_string(),
            period: detail.period,
            digits: detail.digits,
        });
    }
    Ok(records)
}

fn login_to_kdbx(record: &LoginRecord) -> KdbxEntry {
    let now = chrono::Utc::now().to_rfc3339();
    KdbxEntry {
        uuid: fresh_uuid(),
        title: record.title.clone(),
        username: record.username.clone(),
        password: record.password.clone(),
        url: record.url.clone(),
        notes: record.notes.clone(),
        totp_seed: if record.totp_secret.trim().is_empty() {
            None
        } else {
            Some(record.totp_secret.clone())
        },
        custom_fields: Vec::new(),
        attachments: Vec::new(),
        group_path: Vec::new(),
        icon_id: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), VaultError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| VaultError::Storage(format!("JSON 编码失败：{error}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| VaultError::Storage(error.to_string()))?;
    }
    fs::write(path, &bytes).map_err(|error| VaultError::Storage(error.to_string()))?;
    bytes.zeroize();
    Ok(())
}

pub(crate) fn refuse_existing(path: &Path) -> Result<(), VaultError> {
    if path.exists() {
        Err(VaultError::AlreadyExists(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn fresh_uuid() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::getrandom(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn fresh_id(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::getrandom(&mut bytes);
    format!(
        "{prefix}-{}",
        bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    )
}
