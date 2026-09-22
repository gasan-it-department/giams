# Compliance Gap Analysis — Project vs. COA/GAM Requirements

Maps the [COA LGU Accounting Reference](coa-lgu-accounting-reference.md) against the
current codebase. Status as of this review.

---

## ✅ Already implemented (good)

| Requirement | Where |
|---|---|
| Fund types GF / SEF / TF | `backend/app/models/budget.py` → `FundType` enum |
| 5 account groups (Asset/Liability/Equity/Income/Expense) | `models/account.py` → `AccountType` |
| Budget execution chain: Appropriation → Allotment → Obligation → Disbursement | `models/budget.py`, `models/disbursement.py` |
| Double-entry general ledger + trial balance | `models/ledger.py` |
| Official Receipts (OR) for collections | `models/revenue.py` |
| Mandatory deductions (EWT, GSIS, PhilHealth, Pag-IBIG) | `models/disbursement.py` |
| Journal types (GJ, CRJ, CDJ, CkDJ) | Ledger page `journal_type` |
| SFP + Statement of Financial Performance | `routers/reports.py` |

---

## ⚠️ Gaps to close (prioritized)

### P1 — Standard reference is outdated (quick, high-symbolic-value)
- `models/account.py` line 21 says *"COA Circular 2002-002"* — **superseded**.
- **Fix:** update to *Revised Chart of Accounts, COA Circular 2013-002 / GAM for LGUs (PPSAS)*. Re-seed the chart of accounts against the 8-digit RCA.

### P2 — Expense class missing (PS / MOOE / FE / CO)
- GAM tracks allotments & obligations **per expense class**, and the **RAOD** registry is one-per-class.
- **Fix:** add `expense_class` enum to `Budget` (and flow it to `Obligation`). Enables RAOD reporting.

### P3 — Fund cluster not propagated to all transactions
- `FundType` exists on `Budget` only. Revenue, Disbursement, and JournalEntry have no fund.
- GAM requires books **and** financial statements **per fund**.
- **Fix:** add `fund_type` to `RevenueCollection`, `DisbursementVoucher`, `JournalEntry`; add a fund filter to all reports.

### P4 — Obligation should be modeled as ORS (Obligation Request and Status)
- Current model is "OBR" with a single `approved_by`. The live GAM form is **ORS** with three boxes:
  - **Box A** — requesting office, charges/particulars
  - **Box B** — Budget Officer certifies allotment available
  - **Box C** — Accountant certifies obligation valid + running status (obligated / payable / paid / balance)
- **Fix:** rename to ORS; add `certified_by_budget`, `certified_by_accountant`, and status amounts.

### P5 — Missing 3 of the 6 required financial statements
GAM GPFS = **six** statements. Project has 2. Missing:
- Statement of Changes in Net Assets/Equity
- Statement of Cash Flows
- **Statement of Comparison of Budget and Actual Amounts (SCBAA)** ← *most important for LGUs*

### P6 — Budgetary registries not produced
- **RAPAL** (Appropriations vs Allotments) and **RAOD** (Allotments/Obligations/Disbursements per class) are required registries.
- **Fix:** add two report endpoints + pages (mostly aggregation over existing data once P2 lands).

### P7 — Terminology: "IRA" → "NTA"
- "Internal Revenue Allotment" was renamed **National Tax Allotment (NTA)** after the *Mandanas-Garcia* ruling.
- **Fix:** update revenue category labels.

---

## Recommended implementation order

1. **P1** + **P7** — labels/seed (small, no schema migration risk)
2. **P2** — expense class (one enum + column, unlocks RAOD)
3. **P3** — fund propagation (touches several models + reports)
4. **P6** — RAPAL / RAOD registries (reporting only, after P2/P3)
5. **P5** — SCBAA + Cash Flows + Changes in Equity
6. **P4** — ORS restructure (largest single change)
