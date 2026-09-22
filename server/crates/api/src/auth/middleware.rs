//! Request authentication.
//!
//! [`CurrentUser`] is an axum extractor. Naming it in a handler's arguments is
//! what makes that route require a signed-in user — there is no way to write a
//! handler that reads the caller's identity without also enforcing it.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use giams_db::Scope;
use uuid::Uuid;

use crate::{AppError, AppState};

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub session_id: Uuid,
    pub username: String,
    permissions: Vec<String>,
}

impl CurrentUser {
    pub fn scope(&self) -> Scope {
        Scope::user(self.tenant_id, self.user_id)
    }

    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// Fails the request unless the user holds `permission`.
    ///
    /// The error deliberately does not name the permission. To the clerk it is
    /// noise, and to anyone else it is a map of the system.
    pub fn require(&self, permission: &'static str) -> Result<(), AppError> {
        if self.has(permission) {
            Ok(())
        } else {
            tracing::info!(
                user = %self.username,
                permission,
                "request refused for lack of permission"
            );
            Err(AppError::Forbidden { permission })
        }
    }

    /// The permission codes held, for the client to decide what to show.
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthenticated)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthenticated)?;

        let claims = state
            .keys
            .verify_access_token(token)
            .ok_or(AppError::Unauthenticated)?;

        // A token minted for another municipality must never be honoured here,
        // even if it was signed with the same key.
        if claims.tid != state.node.tenant_id {
            tracing::warn!(
                token_tenant = %claims.tid,
                node_tenant = %state.node.tenant_id,
                "rejected a token issued for a different municipality"
            );
            return Err(AppError::Unauthenticated);
        }

        Ok(CurrentUser {
            user_id: claims.sub,
            tenant_id: claims.tid,
            session_id: claims.sid,
            username: claims.username,
            permissions: claims.perms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_with(perms: &[&str]) -> CurrentUser {
        CurrentUser {
            user_id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            session_id: Uuid::nil(),
            username: "clerk".into(),
            permissions: perms.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn a_held_permission_is_allowed() {
        assert!(user_with(&["jev.post"]).require("jev.post").is_ok());
    }

    #[test]
    fn a_missing_permission_is_refused() {
        assert!(user_with(&["jev.draft"]).require("jev.post").is_err());
    }

    #[test]
    fn permissions_are_matched_exactly_not_by_prefix() {
        // "jev.post" must not be satisfied by holding "jev.posting_review",
        // and a user with "jev." must not gain everything under it.
        let u = user_with(&["jev.posting_review", "jev."]);
        assert!(u.require("jev.post").is_err());
    }
}
