//! Request extractors that fail in the system's own words.
//!
//! Axum's built-in `Json` rejects a malformed body with plain text like
//! *"Failed to deserialize the JSON body into the target type: missing field
//! `fund_id` at line 1 column 75"*. That is a message for whoever wrote the
//! client, delivered to whoever is using it — and it escapes the entire error
//! taxonomy the rest of the system goes through.
//!
//! This wrapper produces the same shape as every other failure: a code, a
//! sentence a clerk can act on, and a reference to quote. The field name is
//! kept where the body named one, because "the entry is missing which fund it
//! belongs to" is genuinely useful, while the line and column are not.

use axum::extract::rejection::JsonRejection;
use axum::extract::FromRequest;
use axum::response::{IntoResponse, Response};

use crate::AppError;

/// Drop-in replacement for `axum::Json` as an extractor.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<S, T> FromRequest<S> for Json<T>
where
    axum::Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Json(value)),
            Err(rejection) => Err(translate(rejection).into_response()),
        }
    }
}

impl<T: serde::Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

fn translate(rejection: JsonRejection) -> AppError {
    // Logged in full, where it is useful; never sent, where it is not.
    tracing::debug!(rejection = %rejection, "a request body could not be read");

    match rejection {
        JsonRejection::JsonDataError(e) => {
            let detail = e.body_text();

            if let Some(field) = missing_field(&detail) {
                return AppError::validation(
                    field.clone(),
                    format!("This is required and was not filled in ({field})."),
                );
            }

            AppError::Rule {
                code: "REQUEST_NOT_UNDERSTOOD",
                message: "Some of the information sent was not in the form this screen expected."
                    .into(),
                hint: Some("Reload the page and try again.".into()),
            }
        }

        JsonRejection::JsonSyntaxError(_) => AppError::Rule {
            code: "REQUEST_DAMAGED",
            message: "The information sent was incomplete or damaged in transit.".into(),
            hint: Some("Try again. If it keeps happening, check the network connection.".into()),
        },

        JsonRejection::MissingJsonContentType(_) => AppError::Rule {
            code: "REQUEST_NOT_UNDERSTOOD",
            message: "This request was not sent in the form the server expected.".into(),
            hint: Some("Reload the page and try again.".into()),
        },

        _ => AppError::Rule {
            code: "REQUEST_NOT_UNDERSTOOD",
            message: "The information sent could not be read.".into(),
            hint: Some("Reload the page and try again.".into()),
        },
    }
}

/// Pulls the field name out of serde's message, if it named one.
///
/// Parsing another library's prose is fragile by nature, so it is used only to
/// *improve* a message that already has a usable fallback. When the format
/// changes, the result is a slightly vaguer sentence rather than a broken one.
fn missing_field(detail: &str) -> Option<String> {
    let rest = detail.split("missing field `").nth(1)?;
    let name = rest.split('`').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_field_is_named() {
        let detail = "Failed to deserialize the JSON body into the target type: \
                      missing field `fund_id` at line 1 column 75";
        assert_eq!(missing_field(detail).as_deref(), Some("fund_id"));
    }

    #[test]
    fn a_message_without_a_field_falls_back_rather_than_guessing() {
        assert_eq!(missing_field("invalid type: string, expected u32"), None);
    }
}
