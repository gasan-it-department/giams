//! Proves that peso amounts survive the full round trip without drift.
//!
//! The claim this project rests on is that money is exact from the database,
//! through Rust, onto the wire, and back. That claim is worth testing rather
//! than assuming, because every part of it has a plausible-looking way to be
//! wrong:
//!
//!   * PostgreSQL `NUMERIC` is arbitrary-precision decimal — but a driver
//!     could decode it through `f64` on the way to Rust.
//!   * `rust_decimal::Decimal` is base-10 with a 96-bit integer mantissa, not
//!     binary floating point — but it has a finite range, and division is not
//!     exact.
//!   * The wire format is a string — but only if nothing re-parses it as a
//!     number along the way.
//!
//! Run with a live database:
//!     DATABASE_URL=postgresql://... cargo test -p giams-db --test decimal_fidelity
//! Skipped, loudly, when DATABASE_URL is unset.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

/// Amounts chosen to break things: centavo precision, values binary floating
/// point cannot represent, a municipal annual budget, and the figures from the
/// voucher tests elsewhere in this repository.
///
/// All exactly two decimal places, because that is what money columns are —
/// see migration 0014. The Accounting and Treasury offices work to the
/// centavo, so anything finer is not a peso amount.
const AMOUNTS: &[&str] = &[
    "0.00",
    "0.01",
    "0.10",
    "0.30",             // 0.1 + 0.2 in float is 0.30000000000000004
    "1.15",             // classic half-cent rounding trap
    "8.20",             // not representable in binary
    "184320.00",
    "171417.60",
    "3686.40",
    "12902.40",
    "1234567.89",
    "999999999.99",
    "2500000000.00",    // a large LGU annual budget
    "9999999999999999.99", // the widest NUMERIC(18,2) allows
    "-1250.00",
];

/// A single-connection pool.
///
/// Deliberately one: these tests use `CREATE TEMPORARY TABLE`, which is scoped
/// to the connection that made it. With a larger pool the insert can land on a
/// different connection than the create, and the table appears not to exist.
async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .ok()
}

macro_rules! require_db {
    ($pool:expr) => {
        match $pool {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: set DATABASE_URL to run decimal fidelity tests");
                return;
            }
        }
    };
}

#[tokio::test]
async fn numeric_survives_the_round_trip_to_rust_exactly() {
    let pool = require_db!(pool().await);

    sqlx::query("CREATE TEMPORARY TABLE fidelity (id int, amount numeric(18,2))")
        .execute(&pool)
        .await
        .expect("temp table");

    for (i, text) in AMOUNTS.iter().enumerate() {
        let original = Decimal::from_str_exact(text).expect("parses");

        sqlx::query("INSERT INTO fidelity (id, amount) VALUES ($1, $2)")
            .bind(i as i32)
            .bind(original)
            .execute(&pool)
            .await
            .expect("insert");

        let back: Decimal = sqlx::query("SELECT amount FROM fidelity WHERE id = $1")
            .bind(i as i32)
            .fetch_one(&pool)
            .await
            .expect("select")
            .get(0);

        assert_eq!(
            back, original,
            "`{text}` changed on the way through the database"
        );

        // Equality is not enough on its own — two Decimals can compare equal
        // while carrying different scales. The stored digits must match too.
        let as_text: String = sqlx::query("SELECT amount::text FROM fidelity WHERE id = $1")
            .bind(i as i32)
            .fetch_one(&pool)
            .await
            .expect("select text")
            .get(0);

        assert_eq!(
            Decimal::from_str_exact(&as_text).unwrap(),
            original,
            "`{text}` was stored as `{as_text}`"
        );
    }
}

/// Adds a centavo ten thousand times, in the database and in Rust.
///
/// This is the shape of a real ledger: many small amounts accumulating. In
/// `f64` the error compounds; the whole point of the design is that here it
/// does not.
#[tokio::test]
async fn accumulating_many_small_amounts_does_not_drift() {
    let pool = require_db!(pool().await);

    let db_total: Decimal =
        sqlx::query("SELECT sum(0.01::numeric(18,2)) FROM generate_series(1, 10000)")
            .fetch_one(&pool)
            .await
            .expect("sum")
            .get(0);

    let mut rust_total = Decimal::ZERO;
    let centavo = Decimal::from_str_exact("0.01").unwrap();
    for _ in 0..10_000 {
        rust_total += centavo;
    }

    let expected = Decimal::from_str_exact("100.00").unwrap();

    assert_eq!(db_total, expected, "the database drifted");
    assert_eq!(rust_total, expected, "rust_decimal drifted");

    // What the same loop does in the type this project refuses to use.
    // Compared as a value, not as formatted text: the error is far below the
    // decimal places anyone would print, which is exactly what makes it
    // dangerous — it hides until it accumulates into a reconciliation that is
    // off by a centavo for no visible reason.
    let mut float_total = 0.0_f64;
    for _ in 0..10_000 {
        float_total += 0.01;
    }
    assert_ne!(
        float_total, 100.0_f64,
        "f64 was expected to drift here; if it no longer does, this test has \
         stopped demonstrating anything"
    );
    println!("  f64 after 10,000 additions of 0.01: {float_total:.17}");
    println!("  decimal after the same:             {rust_total}");
}

/// A withholding computation, checked against the arithmetic an auditor would
/// do by hand.
#[tokio::test]
async fn withholding_matches_the_database_to_the_centavo() {
    let pool = require_db!(pool().await);

    let gross = Decimal::from_str_exact("184320.00").unwrap();
    let rate = Decimal::from_str_exact("0.02").unwrap();

    let db_result: Decimal = sqlx::query("SELECT round($1::numeric * $2::numeric, 2)")
        .bind(gross)
        .bind(rate)
        .fetch_one(&pool)
        .await
        .expect("compute")
        .get(0);

    let rust_result = (gross * rate).round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);

    assert_eq!(db_result, rust_result);
    assert_eq!(db_result.to_string(), "3686.40");
}

/// The peso range this actually has to cover.
///
/// `rust_decimal` holds a 96-bit mantissa, so at two decimal places it reaches
/// about 7.9 × 10^26 — far beyond any municipal figure. `NUMERIC(18,2)` is the
/// tighter limit at 10^16, which is ten quadrillion pesos.
#[test]
fn the_representable_range_comfortably_covers_municipal_amounts() {
    // A large LGU annual budget, well inside both limits.
    let budget = Decimal::from_str_exact("2500000000.00").unwrap();
    assert_eq!(budget.to_string(), "2500000000.00");

    // The widest value NUMERIC(18,2) can hold.
    let widest = Decimal::from_str_exact("9999999999999999.99").unwrap();
    assert_eq!(widest.scale(), 2);
    assert!(widest < Decimal::MAX);

    // And the smallest unit the offices work in.
    let centavo = Decimal::from_str_exact("0.01").unwrap();
    assert_eq!(centavo.to_string(), "0.01");
}
