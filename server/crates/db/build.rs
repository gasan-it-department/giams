//! Makes `db/migrations` a build input.
//!
//! `sqlx::migrate!` embeds the migration files into the binary at compile
//! time. Without this, Cargo does not know the macro depends on that
//! directory, so adding a migration and rebuilding produces a binary that
//! still carries the old set — and the failure appears far from the cause, as
//! a missing table or function at runtime.
//!
//! Emitting the directory as a rerun trigger makes a new migration invalidate
//! this crate the way changing its source would.

fn main() {
    println!("cargo:rerun-if-changed=../../../db/migrations");
}
