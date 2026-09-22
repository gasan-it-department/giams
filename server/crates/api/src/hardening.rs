//! Defences that apply to every request, regardless of route.
//!
//! Each of these exists because the alternative is a specific, ordinary
//! failure — not because a checklist asked for it.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request};
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppError;

/// The largest request body accepted, in bytes.
///
/// A journal entry with two hundred lines is a few tens of kilobytes. Two
/// megabytes is generous for everything this system accepts and still small
/// enough that a single request cannot exhaust the office server's memory.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Adds the response headers a browser needs to defend the page.
///
/// The client is served from the same origin as the API, so these can be
/// strict without breaking anything.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // No inline script, no external anything. The client is a compiled bundle
    // served from this origin; if a stored value ever ends up rendered as
    // markup, this is what stops it executing.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; \
             font-src 'self'; \
             connect-src 'self'; \
             object-src 'none'; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );

    // A voucher screen inside someone else's iframe is a clickjacking setup.
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));

    // Stops a browser deciding a JSON response is really HTML and running it.
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    // A voucher number in a referrer header is a voucher number in somebody
    // else's access log.
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );

    // Nothing here needs a camera, a microphone, or the office's location.
    headers.insert(
        axum::http::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );

    response
}

/// Refuses repeated failed sign-ins from the same address.
///
/// The account itself already locks after repeated failures, but an attacker
/// spraying one password across many usernames never trips that — every
/// individual account sees a single failure. This counts the *source* instead,
/// which is the shape that attack actually has.
///
/// Deliberately in memory rather than in the database. It protects a login
/// endpoint on an office LAN, and a restart clearing it is not a weakness
/// worth a write to disk on every failed attempt.
#[derive(Debug)]
pub struct LoginLimiter {
    attempts: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

/// How many failures from one address before it is asked to wait.
const MAX_ATTEMPTS: usize = 10;

/// The window those failures are counted over.
const WINDOW: Duration = Duration::from_secs(300);

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginLimiter {
    pub fn new() -> Self {
        LoginLimiter { attempts: Mutex::new(HashMap::new()) }
    }

    /// Whether this address may try again.
    pub fn permitted(&self, address: IpAddr) -> bool {
        let mut attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            // A poisoned lock means another thread panicked while holding it.
            // Refusing every sign-in from then on would be a self-inflicted
            // outage, so the limiter fails open and the account-level lockout
            // carries the defence.
            Err(poisoned) => poisoned.into_inner(),
        };

        let now = Instant::now();
        let recent = attempts.entry(address).or_default();
        recent.retain(|at| now.duration_since(*at) < WINDOW);

        recent.len() < MAX_ATTEMPTS
    }

    /// Records a failure.
    pub fn failed(&self, address: IpAddr) {
        let mut attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let now = Instant::now();
        let recent = attempts.entry(address).or_default();
        recent.retain(|at| now.duration_since(*at) < WINDOW);
        recent.push(now);

        // Bounded, so a spray from many forged addresses cannot grow this map
        // until the server runs out of memory — which would turn a defence
        // into the outage it was meant to prevent.
        if attempts.len() > 10_000 {
            attempts.retain(|_, times| {
                times.retain(|at| now.duration_since(*at) < WINDOW);
                !times.is_empty()
            });
        }
    }

    /// Forgets an address, on a sign-in that worked.
    pub fn succeeded(&self, address: IpAddr) {
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.remove(&address);
        }
    }
}

/// The error a rate-limited caller is given.
///
/// Says how long to wait and does not say whether the username exists — a
/// message that distinguishes those two is a username oracle.
pub fn too_many_attempts() -> AppError {
    AppError::Rule {
        code: "TOO_MANY_ATTEMPTS",
        message: "There have been too many failed sign-in attempts from this computer. \
                  Please wait five minutes and try again."
            .into(),
        hint: Some(
            "If you have forgotten your password, ask the system administrator to reset it."
                .into(),
        ),
    }
}

/// The caller's address, for rate limiting.
///
/// Trusts the socket, never a header. `X-Forwarded-For` is set by the client
/// unless a proxy overwrites it, so a limiter that believed it could be
/// bypassed by anyone who can type a header.
pub fn caller_address(request: &Request) -> Option<IpAddr> {
    request
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> IpAddr {
        "127.0.0.1".parse().unwrap()
    }

    #[test]
    fn an_address_is_allowed_until_it_has_failed_too_often() {
        let limiter = LoginLimiter::new();

        for _ in 0..MAX_ATTEMPTS {
            assert!(limiter.permitted(local()));
            limiter.failed(local());
        }

        assert!(!limiter.permitted(local()), "the limiter never engaged");
    }

    #[test]
    fn signing_in_successfully_clears_the_count() {
        let limiter = LoginLimiter::new();

        for _ in 0..MAX_ATTEMPTS {
            limiter.failed(local());
        }
        assert!(!limiter.permitted(local()));

        limiter.succeeded(local());
        assert!(limiter.permitted(local()), "a correct password did not clear the block");
    }

    #[test]
    fn one_address_being_blocked_does_not_block_another() {
        let limiter = LoginLimiter::new();
        let other: IpAddr = "192.168.1.50".parse().unwrap();

        for _ in 0..MAX_ATTEMPTS {
            limiter.failed(local());
        }

        assert!(!limiter.permitted(local()));
        assert!(limiter.permitted(other), "one workstation locked out the whole office");
    }

    #[test]
    fn the_message_does_not_reveal_whether_an_account_exists() {
        let AppError::Rule { message, .. } = too_many_attempts() else {
            panic!("unexpected error shape");
        };

        for leak in ["username", "account exists", "no such user", "password is"] {
            assert!(!message.to_lowercase().contains(leak), "the message leaks: {message}");
        }
    }
}
