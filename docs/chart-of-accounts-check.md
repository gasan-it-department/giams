# Chart of accounts check

Source: `Contract\Part III\CHART-OF-ACCOUNTS (1).pdf` — Annex B, Local Government Chart of Accounts, Description of Accounts (98 pages).

Loaded by migration `db/migrations/0035_official_chart_of_accounts.sql`.

| | Count |
|---|---:|
| Accounts in the annex | 571 |
| In GIAMS, same number, title, description and normal balance | 571 |
| Added (were missing; the office's trial balance had only 258) | 319 |
| Kept, not in the annex (semi-expendable property, added by COA later) | 6 |
| Group headings (Assets, Liabilities, Equity, Income, Expenses) | 5 |
| **Total in GIAMS** | **582** |

## Titles

Every account now carries the annex's title. The trial balance's own wording was replaced, e.g. "Accu. Depreciation - Buildings" → "Accumulated Depreciation - Buildings", "Due from LEE -(KAPEC & GRC)" → "Due from Local Economic Enterprise", "Subsidy to NGAs" → "Subsidy to National Government Agencies". The six semi-expendable accounts were corrected from "Semi-Expandable" to COA's "Semi-Expendable" titles.

## Normal balance

Follows the annex. 136 accounts are contra — their normal balance is opposite their group's:

- 133 assets: accumulated depreciation, accumulated impairment losses, allowances for impairment
- 1 liability: Discount on Bonds Payable - Domestic (Debit)
- 2 income: Discount on Real Property Tax - Basic, Discount on Special Education Tax (Debit)

Before 0035 only one account was flagged contra, and normal balance came from the account type alone. The statement of financial position nets every contra account out of its group.

## Numbers misprinted in the annex

Corrected from the accounts around them:

| In the annex | Account | Used |
|---|---|---|
| 1-76-03-061 | Accumulated Depreciation - Communication Networks | 1-07-03-061 |
| 1-07-03-092 (second use) | Accumulated Impairment Losses - Other Infrastructure Assets | 1-07-03-992 |
| 1-07-09-012 (second use) | Accumulated Impairment Losses - Work/Zoo Animals | 1-07-99-012 |
| 1-07-09-992 (second use) | Accumulated Impairment Losses - Other Property, Plant and Equipment | 1-07-99-992 |
| 1-03--01-050 | Loans Receivable - Government-Owned and/or Controlled Corporations | 1-03-01-050 |
