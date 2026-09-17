# Monica by Linux — Desktop Vault

Monica by Linux is a local-first desktop vault whose feature language follows the Monica Android product while adapting interaction patterns to desktop platforms.

**Scope of this glossary.** These terms are the shared product contract, so they are the part
that survives the front-end rewrite. The GTK4 tree (`monica-gtk/`) keeps this same vault
language; only the interaction layer is being rebuilt. Wording that names a specific toolkit
("FluentAvalonia layout", "WinUI task layout", "Avalonia view") is deliberately absent here —
such phrasing describes the **frozen** Avalonia tree and must not be read as product vocabulary.

See `README.md` §「分支布局」 for the branch layout and the current status of each line. This
file is deliberately kept **identical on both branches** — it is toolkit-agnostic by design, so
if you change it on one branch, mirror the change on the other.

## Language

**Vault**:
The encrypted local collection containing passwords, notes, authenticators, cards, documents, attachments, and history.
_Avoid_: Database, store

**Vault Access**:
The initialization, creation, and unlock flow that establishes an authenticated vault session.
_Avoid_: Login, authentication page

**Password Vault**:
The feature for organizing and using website and application credentials.
_Avoid_: Password page, accounts

**Secure Note**:
A vault item containing private plain-text or Markdown content.
_Avoid_: Memo

**Authenticator**:
A vault item that generates a time-based one-time password.
_Avoid_: TOTP account, token

**Wallet Item**:
A bank card or identity document stored in the vault.
_Avoid_: Card record

## Relationships

- **Vault Access** establishes the session required by all other vault features.
- A **Vault** contains zero or more **Password Vault** entries, **Secure Notes**, **Authenticators**, and **Wallet Items**.

## Example Dialogue

> **Dev:** "Should the Password Vault load while Vault Access is still verifying the master password?"
> **Domain expert:** "No. Vault Access must establish the vault session before any encrypted feature data is loaded."

## Flagged Ambiguities

- "Login" previously described local vault unlocking; use **Vault Access** because no remote account is involved.
