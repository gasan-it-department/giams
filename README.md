# GIAMS — Gasan Integrated Accounting Management System

A Digital Governance Initiative of the Municipal Accounting Office,
Municipality of Gasan, Marinduque.

Built to the **Government Accounting Manual (GAM) for Local Government Units**,
prescribed by **COA Circular No. 2020-004** (31 January 2020) in three volumes —
Accounting Policies, Accounting Books/Registries/Records/Forms/Reports, and
The Chart of Accounts — and to **PPSAS**.

> **Note on earlier citations.** This project previously cited COA Circular
> 2015-009 as the GAM and COA Circular 2013-002 as the chart of accounts. Both
> were wrong for a municipality: 2015-009 prescribed the *Revised Chart of
> Accounts for LGUs* and is **superseded** by 2020-004, and 2013-002 is the
> chart for **national government agencies**, not LGUs. The account codes in
> use were taken from the Accounting Office's own trial balance, so the data
> was never affected — only its stated authority. Corrected in migration
> `0029_correct_authority.sql`, which also records the full basis in the
> `regulatory_basis` table so the office can print it.

---

## What this is

Two modules, one system — see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md):

- **Module A — General Ledger & Transaction Processing.** Chart of Accounts,
  appropriations → allotments → obligations (ORS) → disbursement vouchers →
  journal entry vouchers, posted through a balanced double-entry general ledger.
- **Module B — Reconciliation & Financial Reporting.** Bank reconciliation,
  automated matching of disbursement and collection records, trial balance, the
  six GPFS statements per fund, and real-time budget/fund dashboards.

## Shape of the system

```
Tauri 2 desktop client  ──HTTPS──▶  Axum API (Windows Service)  ──▶  PostgreSQL 17
   (per workstation)                  (office server)                (office server)
```

The React SPA is built once and shipped two ways: bundled into the desktop `.msi`,
and served by the API binary for browser-based read-only dashboards.

## Layout

| Path | Contents |
|---|---|
| `db/migrations/` | Versioned SQL migrations (sqlx). The schema and its invariants. |
| `db/seed/` | Revised Chart of Accounts, funds, expense classes, deduction rates. |
| `server/crates/domain/` | Pure accounting logic. No I/O. Where the arithmetic lives. |
| `server/crates/db/` | sqlx repositories. |
| `server/crates/api/` | Axum routers, auth, RBAC. |
| `server/crates/reports/` | Typst → PDF and XLSX generation. |
| `server/crates/server/` | The `giams-server` binary. |
| `client/` | React 19 + TypeScript SPA. |
| `desktop/` | Tauri 2 shell. |
| `ops/` | Service install, TLS certs, backup tasks, installer. |
| `docs/` | Architecture, COA domain reference, ADRs. |

## Non-negotiables

These are enforced in the database, not by convention. Read
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §5 before touching the schema.

1. **Money is `NUMERIC(18,4)` / `rust_decimal::Decimal`.** Never a float. The client
   receives amounts as strings and computes nothing.
2. **Journal entries must balance.** A deferred constraint trigger checks
   `SUM(debit) = SUM(credit)` at COMMIT.
3. **Posted entries are append-only.** Corrections are reversing entries.
4. **The audit log is hash-chained.** `verify_audit_chain()` locates any tampering.
5. **Voucher numbers come from `document_series`, not a SEQUENCE.** Gaps are audit
   findings.
6. **Deduction rates are effectivity-dated rows**, never constants in code.

## Development

Prerequisites: Rust 1.85+ with the **MSVC C++ build tools**, Node 22+,
PostgreSQL 17, WebView2 (bundled with Windows 11).

```bash
cargo install tauri-cli --version "^2" --locked
```

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for setup and the run loop.
