//! A refusal the office can act on.
//!
//! The repository layer returns `anyhow::Result`, which suits the things that
//! genuinely are unexpected — a dropped connection, a query that no longer
//! matches its table. It does not suit the far commoner case of the database
//! layer declining to do something for an ordinary reason the person at the
//! screen can fix.
//!
//! Before this existed, those two were the same thing. A missing accounting
//! period came back as `anyhow::Error`, the API could not tell it apart from a
//! crash, and a clerk who had simply picked a date in a month nobody had opened
//! yet was shown *"Something went wrong and this could not be completed"* with
//! a reference number — as though the system were broken. It was not broken; it
//! was working exactly as designed and saying so badly.
//!
//! A `Refusal` carries what the error taxonomy needs: a stable code, a sentence
//! for a person, and what to do next. The API downcasts to it before anything
//! else, so any repository function can decline in the system's own voice
//! simply by returning one.

/// A rule the database layer declined to break, phrased for the office.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct Refusal {
    /// Stable across releases, so the client can branch on it without
    /// matching on prose that may be reworded.
    pub code: &'static str,
    /// A complete sentence, in the words the office uses. Never a table name,
    /// a column, or a Rust type.
    pub message: String,
    /// What to do next. A message that only announces a problem turns into a
    /// phone call.
    pub hint: Option<String>,
}

impl Refusal {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Refusal { code, message: message.into(), hint: None }
    }

    pub fn with_hint(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Refusal { code, message: message.into(), hint: Some(hint.into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point is that the API can pick it out of an `anyhow::Error`.
    /// If this ever stops working, every refusal silently becomes a 500 again.
    #[test]
    fn a_refusal_survives_being_boxed_into_anyhow() {
        let err: anyhow::Error =
            Refusal::with_hint("PERIOD_NOT_OPEN", "That month is not open.", "Open it first.")
                .into();

        let recovered = err.downcast::<Refusal>().expect("a refusal must be recoverable");

        assert_eq!(recovered.code, "PERIOD_NOT_OPEN");
        assert_eq!(recovered.message, "That month is not open.");
        assert_eq!(recovered.hint.as_deref(), Some("Open it first."));
    }

    #[test]
    fn it_reads_as_its_message() {
        let refusal = Refusal::new("X", "The books are closed for August 2026.");
        assert_eq!(refusal.to_string(), "The books are closed for August 2026.");
    }
}
