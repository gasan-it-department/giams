//! The local draft store.
//!
//! Per docs/ARCHITECTURE.md §2, a workstation may hold **drafts** but may never
//! create a ledger fact. That split is what makes offline work safe: a draft is
//! one person's work-in-progress with no balance implications, so it can be
//! written anywhere and reconciled trivially. A posting is permanent, and only
//! the office node makes those.
//!
//! Everything here is therefore about drafts and nothing else. There is no
//! table for journal entries, and deliberately so.
//!
//! **Encryption.** A municipal workstation is shared and physically
//! accessible, so a plain file of half-finished vouchers sitting in AppData is
//! an avoidable disclosure. The two fields that carry meaning — the summary
//! line and the form payload — are encrypted with AES-256-GCM under a 256-bit
//! key held in the Windows Credential Manager. The key never touches the draft
//! file, so copying the file elsewhere yields nothing readable.
//!
//! SQLCipher would encrypt the whole file and was the first choice, but its
//! build requires OpenSSL, which on Windows MSVC pulls in native Perl and NASM.
//! Three extra toolchains is a poor thing to hand the municipal IT team at
//! turnover for a strictly better-than-needed guarantee. Field-level AEAD
//! covers the actual risk with a pure-Rust dependency.
//!
//! What this deliberately leaves readable: draft ids, timestamps, and the kind
//! of document. Those are not sensitive — knowing a voucher draft was edited
//! on Tuesday discloses nothing about who or how much.

use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keyring::Entry;
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "ph.gov.gasan.giams";
const KEY_ACCOUNT: &str = "draft-database-key";
/// AES-GCM standard nonce length.
const NONCE_LEN: usize = 12;

/// Encrypts one field.
///
/// A fresh random nonce per call, prefixed to the ciphertext. Reusing a nonce
/// under the same key breaks GCM badly, so it is never derived from anything
/// about the record.
fn seal(plaintext: &str) -> Result<String, String> {
    let cipher = cipher()?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| "The draft could not be secured for storage.".to_string())?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(STANDARD.encode(combined))
}

fn open_sealed(stored: &str) -> Result<String, String> {
    let combined = STANDARD
        .decode(stored)
        .map_err(|_| "A stored draft could not be read.".to_string())?;

    if combined.len() <= NONCE_LEN {
        return Err("A stored draft could not be read.".to_string());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);

    let plaintext = cipher()?
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        // GCM authenticates as well as encrypts, so this also fires if the
        // stored bytes were altered — a tampered draft is refused, not
        // silently accepted as garbage.
        .map_err(|_| {
            "A draft saved on this computer could not be read. It may have been \
             created under a different Windows account."
                .to_string()
        })?;

    String::from_utf8(plaintext).map_err(|_| "A stored draft could not be read.".to_string())
}

fn cipher() -> Result<Aes256Gcm, String> {
    let raw = STANDARD
        .decode(database_key()?)
        .map_err(|_| "The key for your local drafts is not usable.".to_string())?;

    if raw.len() != 32 {
        return Err("The key for your local drafts is not usable.".to_string());
    }

    Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&raw)))
}

/// A piece of unfinished work, held until the office node can take it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    /// Assigned by the client as a UUIDv7 so it keeps its identity when it
    /// eventually reaches the node — re-sending is then an upsert on a known
    /// id, not a second document.
    pub id: String,
    /// What is being drafted: `voucher`, `obligation`, `journal_entry`.
    pub kind: String,
    /// A short line for the drafts list, so it can be shown without parsing
    /// the payload.
    pub summary: String,
    /// The form contents, exactly as the screen would have submitted them.
    pub payload: String,
    pub created_at: String,
    pub updated_at: String,
    /// How many times sending this has been attempted, and why it last failed.
    pub attempts: i64,
    pub last_error: Option<String>,
}

/// Opens the draft database, creating it on first use.
fn open(app_dir: &PathBuf) -> Result<Connection, String> {
    std::fs::create_dir_all(app_dir)
        .map_err(|e| format!("could not create the application folder: {e}"))?;

    let conn = Connection::open(app_dir.join("drafts.db"))
        .map_err(|e| format!("could not open the local draft file: {e}"))?;

    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS draft (
            id          TEXT PRIMARY KEY,
            kind        TEXT NOT NULL,
            summary     TEXT NOT NULL,
            payload     TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            attempts    INTEGER NOT NULL DEFAULT 0,
            last_error  TEXT
        );

        CREATE INDEX IF NOT EXISTS draft_by_kind ON draft (kind, updated_at DESC);
        "#,
    )
    .map_err(|e| format!("could not prepare the local draft file: {e}"))?;

    Ok(conn)
}

/// The database key, created once and kept in the Windows Credential Manager.
///
/// Generated rather than derived from anything: there is no password to derive
/// from, and the protection wanted here is against someone reading the file,
/// which DPAPI on the credential entry already provides.
fn database_key() -> Result<String, String> {
    let entry = Entry::new(SERVICE, KEY_ACCOUNT)
        .map_err(|_| "This computer's secure storage is not available.".to_string())?;

    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            let mut bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut bytes);
            let key = STANDARD.encode(bytes);

            entry.set_password(&key).map_err(|e| {
                tracing::error!(error = ?e, "could not store the draft database key");
                "The key for your local drafts could not be saved.".to_string()
            })?;

            Ok(key)
        }
        Err(e) => {
            tracing::error!(error = ?e, "could not read the draft database key");
            Err("The key for your local drafts could not be read.".to_string())
        }
    }
}

pub fn save(app_dir: &PathBuf, draft: Draft) -> Result<(), String> {
    let conn = open(app_dir)?;

    // Upsert on the client-assigned id: editing a draft replaces it rather
    // than accumulating a new row per keystroke-save.
    conn.execute(
        r#"
        INSERT INTO draft (id, kind, summary, payload, created_at, updated_at, attempts, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT (id) DO UPDATE SET
             summary    = excluded.summary,
             payload    = excluded.payload,
             updated_at = excluded.updated_at
        "#,
        params![
            draft.id,
            draft.kind,
            seal(&draft.summary)?,
            seal(&draft.payload)?,
            draft.created_at,
            draft.updated_at,
            draft.attempts,
            draft.last_error
        ],
    )
    .map_err(|e| format!("could not save the draft: {e}"))?;

    Ok(())
}

pub fn list(app_dir: &PathBuf, kind: Option<String>) -> Result<Vec<Draft>, String> {
    let conn = open(app_dir)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, kind, summary, payload, created_at, updated_at, attempts, last_error
               FROM draft
              WHERE (?1 IS NULL OR kind = ?1)
              ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("could not read the drafts: {e}"))?;

    let rows = stmt
        .query_map(params![kind], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|e| format!("could not read the drafts: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("could not read the drafts: {e}"))?;

    rows.into_iter()
        .map(|(id, kind, summary, payload, created_at, updated_at, attempts, last_error)| {
            Ok(Draft {
                id,
                kind,
                summary: open_sealed(&summary)?,
                payload: open_sealed(&payload)?,
                created_at,
                updated_at,
                attempts,
                last_error,
            })
        })
        .collect()
}

/// Removes a draft once the node has accepted it.
///
/// Called only after the node confirms — a draft deleted on send and then lost
/// to a dropped connection is a clerk's work destroyed.
pub fn remove(app_dir: &PathBuf, id: &str) -> Result<bool, String> {
    let conn = open(app_dir)?;

    let removed = conn
        .execute("DELETE FROM draft WHERE id = ?1", params![id])
        .map_err(|e| format!("could not remove the draft: {e}"))?;

    Ok(removed > 0)
}

/// Records a failed send attempt, so a draft that keeps being rejected is
/// visible rather than retrying silently forever.
pub fn record_failure(app_dir: &PathBuf, id: &str, error: &str) -> Result<(), String> {
    let conn = open(app_dir)?;

    conn.execute(
        "UPDATE draft SET attempts = attempts + 1, last_error = ?2 WHERE id = ?1",
        params![id, error],
    )
    .map_err(|e| format!("could not update the draft: {e}"))?;

    Ok(())
}

pub fn count(app_dir: &PathBuf) -> Result<i64, String> {
    let conn = open(app_dir)?;

    conn.query_row("SELECT count(*) FROM draft", [], |row| row.get(0))
        .map_err(|e| format!("could not count the drafts: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("giams-draft-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn a_draft(id: &str) -> Draft {
        Draft {
            id: id.into(),
            kind: "voucher".into(),
            summary: "Marinduque Hardware Supply — 184,320.00".into(),
            payload: r#"{"gross_amount":"184320.00"}"#.into(),
            created_at: "2026-02-15T09:00:00Z".into(),
            updated_at: "2026-02-15T09:00:00Z".into(),
            attempts: 0,
            last_error: None,
        }
    }

    #[test]
    fn a_draft_survives_being_written_and_read_back() {
        let dir = temp_dir("roundtrip");

        save(&dir, a_draft("0195-a")).expect("save");
        let all = list(&dir, None).expect("list");

        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "0195-a");
        assert_eq!(all[0].payload, r#"{"gross_amount":"184320.00"}"#);
        // The amount is stored as it arrived — a string, untouched.
        assert!(all[0].payload.contains("\"184320.00\""));
    }

    #[test]
    fn saving_the_same_draft_twice_replaces_it() {
        let dir = temp_dir("upsert");

        save(&dir, a_draft("0195-b")).expect("save");

        let mut edited = a_draft("0195-b");
        edited.summary = "Marinduque Hardware Supply — 200,000.00".into();
        edited.updated_at = "2026-02-15T10:30:00Z".into();
        save(&dir, edited).expect("save again");

        let all = list(&dir, None).expect("list");
        assert_eq!(all.len(), 1, "editing a draft must not create a second one");
        assert!(all[0].summary.ends_with("200,000.00"));
    }

    #[test]
    fn a_draft_is_only_removed_once_it_has_been_accepted() {
        let dir = temp_dir("remove");

        save(&dir, a_draft("0195-c")).expect("save");
        assert_eq!(count(&dir).expect("count"), 1);

        // A failed send leaves the draft in place, with the reason recorded.
        record_failure(&dir, "0195-c", "The accounting office server cannot be reached.")
            .expect("record failure");

        let all = list(&dir, None).expect("list");
        assert_eq!(all.len(), 1, "a failed send must never destroy the work");
        assert_eq!(all[0].attempts, 1);
        assert!(all[0].last_error.as_deref().unwrap().contains("cannot be reached"));

        assert!(remove(&dir, "0195-c").expect("remove"));
        assert_eq!(count(&dir).expect("count"), 0);
    }

    #[test]
    fn drafts_can_be_listed_by_kind() {
        let dir = temp_dir("bykind");

        save(&dir, a_draft("0195-d")).expect("save");

        let mut ors = a_draft("0195-e");
        ors.kind = "obligation".into();
        save(&dir, ors).expect("save");

        assert_eq!(list(&dir, Some("voucher".into())).expect("list").len(), 1);
        assert_eq!(list(&dir, Some("obligation".into())).expect("list").len(), 1);
        assert_eq!(list(&dir, None).expect("list").len(), 2);
    }

    /// Nothing meaningful about a draft may be readable on disk.
    ///
    /// A municipal workstation is shared and physically accessible. This is the
    /// property the whole encryption design exists for, so it is asserted
    /// against the actual bytes rather than trusted.
    #[test]
    fn draft_contents_are_not_readable_in_the_file() {
        let dir = temp_dir("encrypted");

        save(&dir, a_draft("0195-f")).expect("save");

        // WAL mode keeps recent writes beside the main file; check both.
        let mut bytes = std::fs::read(dir.join("drafts.db")).expect("read the file");
        if let Ok(wal) = std::fs::read(dir.join("drafts.db-wal")) {
            bytes.extend_from_slice(&wal);
        }

        let haystack = String::from_utf8_lossy(&bytes);

        assert!(
            !haystack.contains("Marinduque Hardware Supply"),
            "the payee name is readable in the draft file"
        );
        assert!(
            !haystack.contains("184320.00"),
            "the amount is readable in the draft file"
        );
        assert!(
            !haystack.contains("gross_amount"),
            "the form payload is readable in the draft file"
        );
    }

    /// A tampered record is refused rather than silently accepted.
    ///
    /// AES-GCM authenticates as well as encrypts, so this comes for free — but
    /// only if the failure is actually propagated, which is what is checked.
    #[test]
    fn an_altered_draft_is_refused() {
        let dir = temp_dir("tampered");

        save(&dir, a_draft("0195-g")).expect("save");

        // Overwrite the stored ciphertext with a valid-looking but wrong value.
        let conn = open(&dir).expect("open");
        conn.execute(
            "UPDATE draft SET payload = ?1 WHERE id = '0195-g'",
            params![STANDARD.encode([0u8; 40])],
        )
        .expect("tamper");
        drop(conn);

        assert!(
            list(&dir, None).is_err(),
            "an altered draft must not be returned as if it were genuine"
        );
    }
}
