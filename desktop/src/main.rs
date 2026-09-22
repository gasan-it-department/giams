//! GIAMS desktop shell.
//!
//! Deliberately thin. All accounting logic is on the office node; this process
//! exists to give the Accounting Office a real installed application rather
//! than a browser tab, and to provide the two things a browser cannot:
//!
//!   1. **Credential storage.** The refresh token goes into the Windows
//!      Credential Manager (DPAPI), encrypted against the Windows account. In
//!      a browser it can only go to session storage, which is weaker.
//!   2. **A pinned, versioned install.** For an audited system, "which version
//!      produced this statement" is a question that has to have an answer.
//!
//! Later phases add offline drafting against a local encrypted database, LAN
//! discovery of the node, and printing.

// Do not open a console window alongside the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod discovery;
mod drafts;

use keyring::Entry;
use tauri::Manager;

use discovery::DiscoveredNode;
use drafts::Draft;

/// Namespace for this application's secrets in the platform credential store.
const SERVICE: &str = "ph.gov.gasan.giams";
/// One entry per Windows account, which is the isolation we want: two clerks
/// sharing a workstation under different logins never see each other's session.
const ACCOUNT: &str = "refresh-token";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| {
        // The message returned to the UI must stay non-technical; the detail
        // goes to the log.
        tracing::error!(error = ?e, "could not open the credential store");
        "This computer's secure storage is not available.".to_string()
    })
}

/// Reads the stored refresh token, if there is one.
///
/// A missing entry is a normal state — nobody has signed in on this machine
/// yet — so it returns `None` rather than an error.
#[tauri::command]
fn read_refresh_token() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        // Nobody has signed in on this machine yet.
        Err(keyring::Error::NoEntry) => Ok(None),
        // A genuine failure is NOT reported as "no session". Collapsing the
        // two would present a broken credential store as an ordinary
        // signed-out state, and the person would be sent to the sign-in page
        // every restart with nothing explaining why.
        Err(e) => {
            tracing::error!(error = ?e, "the stored session could not be read");
            Err("This computer's secure storage could not be read. You will need to sign in again.".to_string())
        }
    }
}

#[tauri::command]
fn store_refresh_token(token: String) -> Result<(), String> {
    entry()?.set_password(&token).map_err(|e| {
        tracing::error!(error = ?e, "could not save the session");
        "Your session could not be saved on this computer. You may need to sign in again after closing the application.".to_string()
    })
}

/// Removes the stored token on sign-out.
///
/// A missing entry is success: the desired end state is "no stored token", and
/// it is already true.
#[tauri::command]
fn clear_refresh_token() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => {
            tracing::error!(error = ?e, "could not clear the stored session");
            Err("Your session could not be cleared from this computer.".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real platform credential store.
    ///
    /// Deliberately not mocked: the thing worth verifying is that the refresh
    /// token survives a round trip through the Windows Credential Manager on
    /// the machine this actually ships to, and a mock would verify only that
    /// the mock works.
    #[test]
    fn a_token_survives_a_round_trip_through_the_credential_store() {
        // Leave no residue if an earlier run failed part way.
        let _ = clear_refresh_token();

        assert_eq!(
            read_refresh_token().expect("readable"),
            None,
            "a machine nobody has signed in on must report no stored session"
        );

        let token = "Y3N5jt3RlTESTTOKEN-not-a-real-session";
        store_refresh_token(token.to_string()).expect("storable");

        assert_eq!(read_refresh_token().expect("readable"), Some(token.to_string()));

        // Rotation overwrites rather than accumulating entries.
        let rotated = "5pQ85LxL9LTESTTOKEN-also-not-real";
        store_refresh_token(rotated.to_string()).expect("storable");
        assert_eq!(read_refresh_token().expect("readable"), Some(rotated.to_string()));

        clear_refresh_token().expect("clearable");
        assert_eq!(read_refresh_token().expect("readable"), None);

        // Clearing an already-clear store is success, not an error: the
        // desired end state is "no stored token" and it already holds.
        clear_refresh_token().expect("clearing twice is not an error");
    }
}

// ── Local drafts ─────────────────────────────────────────────────────────────
// The offline half of the system. A workstation holds unfinished work; only
// the office node creates ledger facts. See docs/ARCHITECTURE.md §2.

/// Where this installation keeps its local data.
fn app_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|_| "This computer's application folder could not be found.".to_string())
}

/// Searches the office network for accounting servers.
///
/// Returns every one found rather than choosing. On a shared municipal network
/// there may legitimately be more than one, and picking silently would
/// eventually connect somebody to another municipality's books.
#[tauri::command]
async fn discover_nodes() -> Result<Vec<DiscoveredNode>, String> {
    // Browsing blocks for a few seconds, so it goes to a worker rather than
    // holding the UI thread.
    tauri::async_runtime::spawn_blocking(discovery::find_nodes)
        .await
        .map_err(|_| "The search for the accounting server did not finish.".to_string())?
}

#[tauri::command]
fn draft_save(app: tauri::AppHandle, draft: Draft) -> Result<(), String> {
    drafts::save(&app_dir(&app)?, draft)
}

#[tauri::command]
fn draft_list(app: tauri::AppHandle, kind: Option<String>) -> Result<Vec<Draft>, String> {
    drafts::list(&app_dir(&app)?, kind)
}

#[tauri::command]
fn draft_remove(app: tauri::AppHandle, id: String) -> Result<bool, String> {
    drafts::remove(&app_dir(&app)?, &id)
}

#[tauri::command]
fn draft_record_failure(app: tauri::AppHandle, id: String, error: String) -> Result<(), String> {
    drafts::record_failure(&app_dir(&app)?, &id, &error)
}

#[tauri::command]
fn draft_count(app: tauri::AppHandle) -> Result<i64, String> {
    drafts::count(&app_dir(&app)?)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            read_refresh_token,
            store_refresh_token,
            clear_refresh_token,
            draft_save,
            draft_list,
            draft_remove,
            draft_record_failure,
            draft_count,
            discover_nodes,
        ])
        .run(tauri::generate_context!())
        .expect("the desktop shell failed to start");
}
