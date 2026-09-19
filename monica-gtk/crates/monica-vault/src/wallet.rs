//! Wallet items: bank cards (`EntryType::Card`) and documents (`DocumentRef`).

use mdbx_core::model::EntryType;
use mdbx_storage::connection::VaultConnection;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::io::{list_by_type, load_entry, save_json_entry, soft_delete_entry};
use crate::payload::{
    is_archived, json_string, nested_item_data, take_payload_json, title_from_bytes,
};
use crate::VaultError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletKind {
    Card,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletCardType {
    Debit,
    Credit,
    Prepaid,
}

impl WalletCardType {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Debit => "DEBIT",
            Self::Credit => "CREDIT",
            Self::Prepaid => "PREPAID",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "CREDIT" => Self::Credit,
            "PREPAID" => Self::Prepaid,
            _ => Self::Debit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletDocumentType {
    IdCard,
    Passport,
    DriverLicense,
    SocialSecurity,
    Other,
}

impl WalletDocumentType {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::IdCard => "ID_CARD",
            Self::Passport => "PASSPORT",
            Self::DriverLicense => "DRIVER_LICENSE",
            Self::SocialSecurity => "SOCIAL_SECURITY",
            Self::Other => "OTHER",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "PASSPORT" => Self::Passport,
            "DRIVER_LICENSE" | "DRIVERS_LICENSE" => Self::DriverLicense,
            "SOCIAL_SECURITY" | "SSN" => Self::SocialSecurity,
            "OTHER" => Self::Other,
            _ => Self::IdCard,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletSummary {
    pub entry_id: String,
    pub kind: WalletKind,
    pub title: String,
    pub subtitle: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct WalletDetail {
    pub entry_id: String,
    pub kind: WalletKind,
    pub title: String,
    pub holder: String,
    pub number: SecretString,
    pub extra: String,
    pub expiry: String,
    pub issued: String,
    pub nationality: String,
    pub card_type: WalletCardType,
    pub document_type: WalletDocumentType,
    pub cvv: SecretString,
    pub notes: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct WalletDraft {
    pub entry_id: Option<String>,
    pub kind: WalletKind,
    pub title: String,
    pub holder: String,
    pub number: SecretString,
    pub extra: String,
    pub expiry: String,
    pub issued: String,
    pub nationality: String,
    pub card_type: WalletCardType,
    pub document_type: WalletDocumentType,
    pub cvv: SecretString,
    pub notes: String,
}

pub(crate) fn list_wallet(conn: &VaultConnection) -> Result<Vec<WalletSummary>, VaultError> {
    let mut summaries = Vec::new();
    collect_kind(conn, EntryType::Card, WalletKind::Card, &mut summaries)?;
    collect_kind(
        conn,
        EntryType::DocumentRef,
        WalletKind::Document,
        &mut summaries,
    )?;
    summaries.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(summaries)
}

fn collect_kind(
    conn: &VaultConnection,
    entry_type: EntryType,
    kind: WalletKind,
    summaries: &mut Vec<WalletSummary>,
) -> Result<(), VaultError> {
    let mut entries = list_by_type(conn, entry_type)?;
    for entry in &mut entries {
        let value = take_payload_json(entry);
        if is_archived(&value) {
            continue;
        }
        let nested = nested_item_data(&value);
        let title = title_from_bytes(entry.title_ct.as_deref());
        let subtitle = match kind {
            WalletKind::Card => {
                let number = json_string(&nested, &["cardNumber", "card_number"]);
                mask_digits(&number)
            }
            WalletKind::Document => {
                let number = json_string(&nested, &["documentNumber", "document_number"]);
                let holder = json_string(&nested, &["fullName", "full_name"]);
                if holder.is_empty() {
                    mask_digits(&number)
                } else {
                    holder
                }
            }
        };
        summaries.push(WalletSummary {
            entry_id: entry.entry_id.clone(),
            kind,
            title,
            subtitle,
            updated_at: entry.updated_at.clone(),
        });
    }
    Ok(())
}

pub(crate) fn get_wallet(
    conn: &VaultConnection,
    entry_id: &str,
) -> Result<WalletDetail, VaultError> {
    let mut entry = load_entry(conn, entry_id, false)?;
    let kind = match entry.entry_type {
        EntryType::Card => WalletKind::Card,
        EntryType::DocumentRef => WalletKind::Document,
        _ => {
            return Err(VaultError::Storage(format!(
                "entry {entry_id} is not a wallet item"
            )))
        }
    };
    let value = take_payload_json(&mut entry);
    if is_archived(&value) {
        return Err(VaultError::EntryNotFound(entry_id.to_string()));
    }
    let nested = nested_item_data(&value);
    let notes = json_string(&value, &["notes"]);
    Ok(match kind {
        WalletKind::Card => WalletDetail {
            entry_id: entry.entry_id,
            kind,
            title: title_from_bytes(entry.title_ct.as_deref()),
            holder: json_string(&nested, &["cardholderName", "cardholder_name"]),
            number: SecretString::from(json_string(&nested, &["cardNumber", "card_number"])),
            extra: json_string(&nested, &["bankName", "bank_name", "brand"]),
            expiry: card_expiry(&nested),
            issued: String::new(),
            nationality: String::new(),
            card_type: WalletCardType::parse(&json_string(&nested, &["cardType", "card_type"])),
            document_type: WalletDocumentType::IdCard,
            cvv: SecretString::from(json_string(&nested, &["cvv"])),
            notes,
            updated_at: entry.updated_at,
        },
        WalletKind::Document => WalletDetail {
            entry_id: entry.entry_id,
            kind,
            title: title_from_bytes(entry.title_ct.as_deref()),
            holder: json_string(&nested, &["fullName", "full_name"]),
            number: SecretString::from(json_string(
                &nested,
                &["documentNumber", "document_number"],
            )),
            extra: json_string(&nested, &["issuedBy", "issued_by"]),
            expiry: json_string(&nested, &["expiryDate", "expiry_date"]),
            issued: json_string(&nested, &["issuedDate", "issued_date"]),
            nationality: json_string(&nested, &["nationality"]),
            card_type: WalletCardType::Debit,
            document_type: WalletDocumentType::parse(&json_string(
                &nested,
                &["documentType", "document_type"],
            )),
            cvv: SecretString::from(String::new()),
            notes,
            updated_at: entry.updated_at,
        },
    })
}

pub(crate) fn save_wallet(
    conn: &VaultConnection,
    draft: &WalletDraft,
) -> Result<WalletSummary, VaultError> {
    let title = if draft.title.trim().is_empty() {
        match draft.kind {
            WalletKind::Card => "银行卡",
            WalletKind::Document => "证件",
        }
        .to_string()
    } else {
        draft.title.trim().to_string()
    };
    let (entry_type, kind_name, item_data, subtitle) = match draft.kind {
        WalletKind::Card => {
            let (month, year) = split_expiry(&draft.expiry);
            let item_data = json!({
                "cardNumber": draft.number.expose_secret(),
                "cardholderName": draft.holder,
                "expiryMonth": month,
                "expiryYear": year,
                "cvv": draft.cvv.expose_secret(),
                "bankName": draft.extra,
                "cardType": draft.card_type.as_storage(),
                "billingAddress": "",
                "imagePaths": [],
                "brand": "",
                "nickname": title,
            });
            (
                EntryType::Card,
                "bank_card",
                item_data,
                mask_digits(draft.number.expose_secret()),
            )
        }
        WalletKind::Document => {
            let item_data = json!({
                "documentNumber": draft.number.expose_secret(),
                "fullName": draft.holder,
                "issuedDate": draft.issued,
                "expiryDate": draft.expiry,
                "issuedBy": draft.extra,
                "nationality": draft.nationality,
                "documentType": draft.document_type.as_storage(),
                "imagePaths": [],
                "additionalInfo": draft.notes,
            });
            (
                EntryType::DocumentRef,
                "document",
                item_data,
                if draft.holder.trim().is_empty() {
                    mask_digits(draft.number.expose_secret())
                } else {
                    draft.holder.clone()
                },
            )
        }
    };
    let payload = json!({
        "kind": kind_name,
        "notes": draft.notes,
        "item_data": item_data.to_string(),
        "image_paths": "[]",
        "archived": false,
    });
    let saved = save_json_entry(
        conn,
        draft.entry_id.as_deref(),
        entry_type,
        &title,
        &payload,
    )?;
    Ok(WalletSummary {
        entry_id: saved.entry_id,
        kind: draft.kind,
        title,
        subtitle,
        updated_at: saved.updated_at,
    })
}

pub(crate) fn delete_wallet(conn: &VaultConnection, entry_id: &str) -> Result<(), VaultError> {
    soft_delete_entry(conn, entry_id)
}

fn card_expiry(nested: &serde_json::Value) -> String {
    let month = json_string(nested, &["expiryMonth", "expiry_month"]);
    let year = json_string(nested, &["expiryYear", "expiry_year"]);
    match (month.as_str(), year.as_str()) {
        ("", "") => json_string(nested, &["expiryDate", "expiry"]),
        (month, "") => month.to_string(),
        ("", year) => year.to_string(),
        (month, year) => format!("{month}/{year}"),
    }
}

fn split_expiry(expiry: &str) -> (String, String) {
    let cleaned = expiry.replace(' ', "");
    if let Some((month, year)) = cleaned.split_once(['/', '-']) {
        (month.to_string(), year.to_string())
    } else {
        (cleaned, String::new())
    }
}

pub fn mask_digits(raw: &str) -> String {
    let digits: String = raw.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.len() < 4 {
        if raw.trim().is_empty() {
            "••••".to_string()
        } else {
            "••••".to_string()
        }
    } else {
        format!("•••• {}", &digits[digits.len() - 4..])
    }
}
