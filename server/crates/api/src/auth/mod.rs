//! Authentication and authorisation.
//!
//! See docs/ARCHITECTURE.md §6. The shape of it:
//!
//! * Argon2id passwords, tuned rather than defaulted ([`password`]).
//! * A short-lived signed access token plus an opaque rotating refresh token
//!   ([`token`]), with replay of a retired token treated as theft ([`session`]).
//! * Permissions checked in middleware **and** re-checked in the domain layer.
//!   The UI hiding a button is a convenience, never a control.

pub mod middleware;
pub mod password;
pub mod session;
pub mod token;

pub use middleware::CurrentUser;
pub use token::TokenKeys;
