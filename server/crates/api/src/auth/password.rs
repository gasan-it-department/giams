//! Password hashing.
//!
//! Argon2id, with parameters chosen from the OWASP Password Storage Cheat
//! Sheet rather than the library defaults. The parameters are recorded inside
//! the PHC string, so raising them later re-hashes users gradually on their
//! next sign-in instead of invalidating every existing password.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

/// 19 MiB of memory, 2 passes, 1 lane.
///
/// The memory cost is what makes GPU cracking expensive, and it is chosen to
/// stay comfortable on the modest hardware a municipal office actually runs —
/// a login should cost well under a tenth of a second on the office server.
fn argon2() -> Argon2<'static> {
    let params = Params::new(19_456, 2, 1, None).expect("argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("could not hash the password: {e}"))?;
    Ok(hash.to_string())
}

/// Checks a password against a stored PHC string.
///
/// Returns `false` rather than an error for a malformed stored hash: an
/// unreadable hash must never be treated as a match, and the distinction is
/// not something the caller should be able to act on differently.
pub fn verify_password(plain: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        tracing::error!("a stored password hash could not be parsed");
        return false;
    };

    argon2().verify_password(plain.as_bytes(), &parsed).is_ok()
}

/// Runs a hash computation and discards it.
///
/// Called when a sign-in names a user that does not exist. Without it, a
/// missing username returns noticeably faster than a wrong password, and that
/// timing difference lets an attacker enumerate who works in the Accounting
/// Office before trying a single password.
pub fn waste_time_like_a_real_verification() {
    const DUMMY: &str = "$argon2id$v=19$m=19456,t=2,p=1\
                         $c29tZXNhbHRzb21lc2FsdA$sVSc6E1V3+wJ0kO9dQyQ0nOgSjMqPGvJTOZQIHNCFVo";
    let _ = verify_password("not the real password", DUMMY);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_verifies_against_its_own_hash() {
        let hash = hash_password("Tan0ng-Gasan-2026!").unwrap();
        assert!(verify_password("Tan0ng-Gasan-2026!", &hash));
        assert!(!verify_password("tan0ng-gasan-2026!", &hash));
        assert!(!verify_password("", &hash));
    }

    #[test]
    fn the_same_password_hashes_differently_every_time() {
        // Distinct salts. Two users with the same password must not be
        // identifiable as such from the stored rows.
        let a = hash_password("shared-password").unwrap();
        let b = hash_password("shared-password").unwrap();
        assert_ne!(a, b);
        assert!(verify_password("shared-password", &a));
        assert!(verify_password("shared-password", &b));
    }

    #[test]
    fn the_recorded_parameters_are_the_ones_we_intend() {
        let hash = hash_password("x").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{hash}");
    }

    #[test]
    fn a_corrupt_stored_hash_never_verifies() {
        assert!(!verify_password("anything", "not-a-phc-string"));
        assert!(!verify_password("anything", ""));
    }
}
