-- ═══════════════════════════════════════════════════════════════════════════
--  0035 — The official chart of accounts, with descriptions
-- ═══════════════════════════════════════════════════════════════════════════
--  Source: Annex B of the GAM for LGUs, "Local Government Chart of Accounts,
--  Description of Accounts" (Contract\Part III\CHART-OF-ACCOUNTS (1).pdf).
--  571 accounts, each with its title, number, normal balance and description.
--
--  0018 loaded only the 258 accounts on the office's trial balance. This adds
--  the rest, and replaces the trial balance's titles with the official ones.
--
--  Four numbers in the annex are misprinted; they are corrected here from the
--  accounts around them:
--      1-76-03-061  Accumulated Depreciation - Communication Networks        -> 1-07-03-061
--      1-07-03-092  Accumulated Impairment Losses - Other Infrastructure      -> 1-07-03-992
--      1-07-09-012  Accumulated Impairment Losses - Work/Zoo Animals          -> 1-07-99-012
--      1-07-09-992  Accumulated Impairment Losses - Other Property, Plant ... -> 1-07-99-992
--
--  Normal balance now follows the annex. It was derived from the account type
--  alone, which put every contra account (accumulated depreciation,
--  allowances, discounts) on the wrong side: 136 accounts in the annex are
--  contra. Their stored month-end balances are re-signed to match, and the
--  statement of financial position nets contra accounts in equity and in the
--  surplus line as it already did for assets and liabilities.
-- ═══════════════════════════════════════════════════════════════════════════

ALTER TABLE account ADD COLUMN description text;

COMMENT ON COLUMN account.description IS
    'What the account is used for, from the annex. Shown to the clerk choosing it.';


-- ── The annex ────────────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION official_chart_of_accounts()
RETURNS TABLE (
    code          text,
    name          text,
    account_type  account_type,
    is_contra     boolean,
    expense_class expense_class,
    is_cash       boolean,
    description   text
)
LANGUAGE sql
IMMUTABLE
AS $fn$
    SELECT t.code, t.name, t.kind::account_type, t.contra, t.class::expense_class, t.cash, t.description
      FROM (VALUES
        ('1-01-01-010','Cash - Local Treasury','asset',false,NULL,true,
         'This account is used to record the amount of cash received by the Local Treasurers for deposit to Authorized Government Depository Bank (AGDB). Credit this account for deposits to AGDBs.'),
        ('1-01-01-020','Petty Cash','asset',false,NULL,true,
         'This account is used to record the amount of cash granted to duly designated Petty Cash custodian for payment of authorized petty or miscellaneous expenses which cannot be conveniently paid through checks. Credit this account every time there is a change in Petty Cash custodian and the end of the year for refund and liquidation of cash advance.'),
        ('1-01-02-010','Cash in Bank - Local Currency, Current Account','asset',false,NULL,true,
         'This account is used to record deposits with AGDBs, trust receipts authorized under specific contracts and agreements; Revolving Fund under specific laws; and credit advices/memos received from the AGDBs. This account is credited for check drawn against the account, debit advices/memos received from the AGDBs.'),
        ('1-01-02-020','Cash in Bank - Local Currency, Savings Account','asset',false,NULL,true,
         'This account is used to record deposits to combined savings account of local government units with AGDBs. Credit this account for transfers to the current account.'),
        ('1-01-03-010','Cash in Bank - Foreign Currency, Current Account','asset',false,NULL,true,
         'This account is used to record foreign currency deposits of agencies implementing foreign-assisted projects and required by regulation or by the lending/donor institution to maintain Special Account/ Working Fund. Credit this account for withdrawals made.'),
        ('1-01-03-020','Cash in Bank - Foreign Currency, Savings Account','asset',false,NULL,true,
         'This account is used to record foreign currency deposits of agencies implementing foreign-assisted projects and required to maintain Special Accounts.'),
        ('1-02-01-010','Cash in Bank - Local Currency, Time Deposits','asset',false,NULL,false,
         'This account is used to record placements of cash in local currency with AGDBs for a specific period of time. Credit this account for withdrawal of placements.'),
        ('1-02-01-020','Cash in Bank - Foreign Currency, Time Deposits','asset',false,NULL,false,
         'This account is used to record placements of cash in foreign currency with AGDBs for a specific period of time. Credit this account for withdrawal of placements.'),
        ('1-02-01-030','Treasury Bills','asset',false,NULL,false,
         'This account is used to record amounts placed in treasury bills issued by the National Government thru the Bureau of Treasury (BTr). Credit this account when the rights to the cash flows have expired or transferred to another party thru resale or other disposition.'),
        ('1-02-02-010','Financial Assets Held for Trading','asset',false,NULL,false,
         'This account is used to record government authorized debt and equity security investments that are: (a) acquired principally for the purpose of selling or repurchasing them in the near term; or (b) part of a portfolio of identified securities that are managed together and for which there is evidence of a recent actual pattern of short term profit-taking. Credit this account when the rights to the cash flows have expired or transferred to another party thru resale or other disposition.'),
        ('1-02-02-020','Financial Assets Designated at Fair Value Through Surplus or Deficit','asset',false,NULL,false,
         'This account is used to record government authorized debt and equity security investments that, upon initial recognition are designated by the entity as at fair value through profit or loss. This shall comprise both debt and equity securities. Credit this account when the rights to the cash flows have expired or transferred to another party thru resale or other disposition.'),
        ('1-02-03-010','Investments in Treasury Bills - Local','asset',false,NULL,false,
         'This account is used to record money invested in treasury bills issued by the National Government with the intention to hold until maturity. Credit this account upon settlement or derecognition.'),
        ('1-02-03-011','Allowance for Impairment - Investments in Treasury Bills - Local','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the investments in local treasury bills are impaired. Debit this account upon derecognition of the investments in local treasury bills or reversal of impairment loss.'),
        ('1-02-03-020','Investments in Bonds - Local','asset',false,NULL,false,
         'This account is used to record investment in bonds issued by the BTr and other local governments. Credit this account upon settlement or derecognition.'),
        ('1-02-03-021','Allowance for Impairment - Investments in Bonds - Local','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the investments in local bonds are impaired. Debit this account upon derecognition of the investments in local treasury bonds or reversal of impairment loss.'),
        ('1-02-04-010','Investments in Stocks','asset',false,NULL,false,
         'This account is used to record record investment in government authorized equity shares not recognized at fair value through surplus or deficit. Credit this account upon transfer, settlement or derecognition.'),
        ('1-02-04-020','Investments in Bonds','asset',false,NULL,false,
         'This account is used to record investment in government authorized bonds not recognized at fair value through surplus or deficit or held to maturity. Credit this account upon settlement, transfer or derecognition.'),
        ('1-02-05-010','Deposits on Letters of Credit','asset',false,NULL,false,
         'This account is used to record the amount paid by the agency to the bank for opening letters of credit. Credit this account for delivery of goods purchased.'),
        ('1-02-05-011','Allowance for Impairment - Deposits on Letters of Credit','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the deposit on letters of credit are impaired. Debit this account upon derecognition of the deposit or reversal of impairment loss.'),
        ('1-02-05-020','Guaranty Deposits','asset',false,NULL,false,
         'This account is used to record amount deposited for goods such as deposit for containers and deposits made to guarantee compliance with the terms of an agreement. Credit this account for application of deposit.'),
        ('1-02-05-021','Allowance for Impairment - Guaranty Deposits','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the guaranty deposits are impaired. Debit this account upon derecognition of the deposit or reversal of impairment loss.'),
        ('1-02-05-990','Other Investments','asset',false,NULL,false,
         'This account is used to record other deposits or investments. Credit this account upon settlement, transfer or derecognition.'),
        ('1-02-05-991','Allowance for Impairment - Other Investments','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the other deposits or investments are impaired. Debit this account upon derecognition of the deposits or investments or reversal of impairment loss.'),
        ('1-02-06-010','Investments in Joint Ventures','asset',false,NULL,false,
         'This account is used to record investments in joint venture activities. Credit this account for return of investment, sale, transfer or derecognition.'),
        ('1-02-06-011','Allowance for Impairment - Investments in Joint Ventures','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the investment in joint venture is impaired. Debit this account for return of investment, sale, transfer or derecognition.'),
        ('1-02-07-010','Sinking Fund','asset',false,NULL,false,
         'This account is used to record the amount set aside on a regular basis to pay off at maturity long-term debt such as bonds, with maturities of three years or more. This includes investments of the LGU and LGU-managed sinking funds in time deposits and securities. Credit this account for payment of long term debt interest expenses and principal.'),
        ('1-03-01-010','Accounts Receivable','asset',false,NULL,false,
         'This account is used to record the amount due from clients/customers arising from regular trade and business transactions. Credit this account for collection of receivable or transfers.'),
        ('1-03-01-011','Allowance for Impairment - Accounts Receivable','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the accounts receivable is impaired. Debit this account for collection of receivables, transfer or reversal of impairment.'),
        ('1-03-01-020','Real Property Tax Receivable','asset',false,NULL,false,
         'This account is used to record amount of receivables for real property tax income accruing to the agency due from the taxpayers for basic real property tax from real property, such as: land, building and equipment.'),
        ('1-03-01-021','Allowance for Impairment - Real Property Tax Receivable','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the real property tax receivable is impaired. Debit this account for collection of receivables, transfer or reversal of impairment.'),
        ('1-03-01-030','Special Education Tax Receivable','asset',false,NULL,false,
         'This account is used to record amount receivable for special education tax income accruing to the agency due from the taxpayers for the additional 1% tax from real property for special education.'),
        ('1-03-01-031','Allowance for Impairment - Special Education Tax Receivable','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the special education tax receivable is impaired. Debit this account for collection of receivables, transfer or reversal of impairment.'),
        ('1-03-01-040','Notes Receivable','asset',false,NULL,false,
         'This account is used to record claims expected to be received at a future date, usually with interest, for which a formal instrument of credit is issued as evidence of debt, such as promissory notes. Credit this account for encashment of notes.'),
        ('1-03-01-041','Allowance for Impairment - Notes Receivable','asset',true,NULL,false,
         'This account is credited when there is an objective evidence that the notes receivable is impaired. Debit this account upon encashment of notes, transfer or reversal of impairment.'),
        ('1-03-01-050','Loans Receivable - Government-Owned and/or Controlled Corporations','asset',false,NULL,false,
         'This account is used to record credits/loans in foreign or local currency extended by the LGU to the GFIs/GOCCs covered by loan agreements. Credit this account for receipt of loan repayments.'),
        ('1-03-01-051','Allowance for Impairment - Loans Receivable - Government-Owned and/or Controlled Corporations','asset',true,NULL,false,
         'This account is credited upon to set up provision for losses which may arise from the non-collection of loans receivable. Debit this account upon receipt of loan repayments or transfers.'),
        ('1-03-01-060','Loans Receivable - Local Government Units','asset',false,NULL,false,
         'This account is used to record amounts of credit extended to local government units covered by loan agreements. Credit this account for receipt of loan repayments or transfers.'),
        ('1-03-01-061','Allowance for Impairment - Loans Receivable - Local Government Units','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-repayment of loans receivable. Debit this account upon receipt of loan repayments, transfers, or reversal of impairment.'),
        ('1-03-01-070','Interests Receivable','asset',false,NULL,false,
         'This account is used to record the amount of accrued interests on advances and investments. Credit this account for receipt of interest.'),
        ('1-03-01-071','Allowance for Impairment - Interests Receivable','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of interests receivable. Debit this account upon receipt of interests, transfers, or reversal of impairment.'),
        ('1-03-01-080','Dividends Receivable','asset',false,NULL,false,
         'This account is used to record dividends due from investments in stocks in GOCCs and other public utility companies. Credit this account for receipt of dividends.'),
        ('1-03-01-081','Allowance for Impairment - Dividends Receivable','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of dividend receivable. Debit this account upon receipt of dividends, or reversal of impairment.'),
        ('1-03-01-990','Loans Receivable - Others','asset',false,NULL,false,
         'This account is used to record amounts of credit extended to debtors not otherwise specifically classified. Credit this account for receipt of loan repayments.'),
        ('1-03-01-991','Allowance for Impairment - Loans Receivable - Others','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of other loan receivables. Debit this account upon receipt of loan repayments, transfers, or reversal of impairment.'),
        ('1-03-02-010','Operating Lease Receivable','asset',false,NULL,false,
         'This account is used to record the accrual of rental income from lease of assets under operating lease. Credit this account upon collection of rental income or transfers.'),
        ('1-03-02-011','Allowance for Impairment - Operating Lease Receivable','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of operating lease receivables. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-02-020','Finance Lease Receivable','asset',false,NULL,false,
         'This account is used to record the amount due from lease of assets under finance lease or sale of government property on installment basis. Credit this account upon collection of the installment due or transfers.'),
        ('1-03-02-021','Allowance for Impairment - Finance Lease Receivable','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of finance lease receivables. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-03-010','Due from National Government Agencies','asset',false,NULL,false,
         'This account is used to record advances for purchase of goods/services as authorized by law, fund transfers to the NGAs for implementation of projects, receivables of LGUs'' share from national taxes covered by notice received from DBM, expenses advanced by LGUs in behalf of the NGAs and other receivables from NGAs. Credit this account for delivery of goods /services, liquidation of fund transfers, receipt of share from national taxes, payment of advances and other receivables.

(Subsidiary ledger shall be maintained by agency such as Procurement Service, National Printing Office, etc.)'),
        ('1-03-03-011','Allowance for Impairment - Due from National Government Agencies','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of receivables from national government agencies. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-03-020','Due from Government-Owned and/or Controlled Corporations','asset',false,NULL,false,
         'This account is used to record advances for purchase of goods/services as authorized by law, fund transfers to the GOCCs for implementation of projects. Credit this account for delivery of goods /services, liquidation of fund transfers, payment of advances and other receivables.

(Subsidiary ledger shall be maintained by agency.)'),
        ('1-03-03-021','Allowance for Impairment - Due from Government-Owned and/or Controlled Corporations','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of receivables from government-owned and/or controlled corporations. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-03-030','Due from Local Government Units','asset',false,NULL,false,
         'This account is used to record amount of advances for purchase of goods/services as authorized by law, fund transfers to the LGUs for implementation of projects, share from LGUs income and other receivables. Credit this account for delivery of goods /services, liquidation of fund transfers and receipt of share/payment of other receivables.'),
        ('1-03-03-031','Allowance for Impairment - Due from Local Government Units','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of receivables from local government units. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-03-040','Due from Joint Venture','asset',false,NULL,false,
         'This account is used to record amount of advances and/or fund transfers to the Joint Ventures for implementation of projects. Credit this account for receipt of share/payment of other receivables.'),
        ('1-03-03-041','Allowance for Impairment - Due from Joint Venture','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of receivables from joint ventures. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-04-050','Due from Other Funds','asset',false,NULL,false,
         'This account is used to record in the agency books the amount of advances to other funds. Credit this account upon liquidation.'),
        ('1-03-04-060','Due from Special Accounts','asset',false,NULL,false,
         'This account is used to record the amount granted to other special accounts in the general fund of the local government unit. Credit this account upon liquidation.'),
        ('1-03-04-070','Due from Local Economic Enterprise','asset',false,NULL,false,
         'This account is used to record the amount granted to local economic enterprises maintained as special accounts in the general fund of the local government unit. Credit this account upon liquidation.'),
        ('1-03-05-010','Advances for Operating Expenses','asset',false,NULL,false,
         'This account is used to record in the agency books the amount of advances granted to accountable officers for payment of operating expenses of operating/field units. Credit this account upon liquidation.'),
        ('1-03-05-020','Advances for Payroll','asset',false,NULL,false,
         'This account is used to record the amount granted to regular disbursing officer for payment of salaries, wages, honoraria, allowances and other personnel benefits. Credit this account upon liquidation.'),
        ('1-03-05-030','Advances to Special Disbursing Officer','asset',false,NULL,false,
         'This account is used to record the amount granted to agency''s accountable officers and employees for special purpose/time-bound undertakings to be liquidated within a specified period. This includes labor payroll for projects undertaken by administration. Credit this account upon liquidation.'),
        ('1-03-05-040','Advances to Officers and Employees','asset',false,NULL,false,
         'This account is used to record amount advanced to officers and employees for official travel. Credit this account for liquidation.'),
        ('1-03-06-010','Receivables - Disallowances/Charges','asset',false,NULL,false,
         'This account is used to record the amount of disallowances/charges in post audit due from public/private individuals/entities which have become final and executory. Credit this account for settlement of disallowances/charges.'),
        ('1-03-06-011','Allowance for Impairment - Receivables - Disallowances/Charges','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-collection of receivables-disallowances/charges. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-06-020','Due from Officers and Employees','asset',false,NULL,false,
         'This account is used to record amount of claims from agency''s officers and employees for overpayment, cash shortage, loss of assets and other bills issued by the agency. Credit this account for collection of receivable.'),
        ('1-03-06-021','Allowance for Impairment - Due from Officers and Employees','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-liquidation of advances from officers and employees. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-06-030','Due from Non-Government Organizations/People''s Organizations','asset',false,NULL,false,
         'This account is used to record amount of advances granted to Non- Government Organizations (NGOs)/ People''s Organizations (POs) for implementation of specific projects. Credit this account for liquidation of advances.'),
        ('1-03-06-031','Allowance for Impairment - Due from Non-Government Organizations/People''s Organizations','asset',true,NULL,false,
         'This account is credited to set up provision for losses which may arise from the non-liquidation of advances from officers and employees. Debit this account upon collection of the receivables, transfers, or reversal of impairment.'),
        ('1-03-06-990','Other Receivables','asset',false,NULL,false,
         'This account is used to record amount due from debtors and other agencies not falling under any of the specific receivable account. Credit this account for payment/liquidation of receivable.'),
        ('1-03-06-991','Allowance for Impairment - Other Receivables','asset',false,NULL,false,
         'This account is credited to reduce the cost/amortized cost/ fair value of other receivables due to impairment. Debit this account upon receipt of other receivables, transfers and reversal of impairment loss.'),
        ('1-04-01-010','Merchandise Inventory','asset',false,NULL,false,
         'This account is used to record the cost of goods purchased/acquired which are intended for sale in the ordinary course of business. This includes agricultural produce and other merchandise. Credit this account for the sale, transfer or write-down of merchandise inventory.

Subsidiary ledgers: 01- Accountable Forms 02- Drugs and Medicines 03- Agricultural Produce 04- Ammunitions 05- Property and Equipment 99 - Others'),
        ('1-04-02-010','Food Supplies for Distribution','asset',false,NULL,false,
         'This account is used to record cost of food for hospital/rehabilitation patients, jail inmates and the like. Credit this account for issuance to end- users, transfers or write-down.'),
        ('1-04-02-020','Welfare Goods for Distribution','asset',false,NULL,false,
         'This account is used to record the cost of goods for distribution to people affected by calamities/disasters/ground conflicts such as canned goods, blankets, mats, kitchen utensils, flashlights and other similar items. Credit this account for issuance to end-users, transfers or write-down.'),
        ('1-04-02-030','Drugs and Medicines for Distribution','asset',false,NULL,false,
         'This account is used to record the cost of drugs and medicines purchased/received for distribution. Credit this account for issuance to end- users, transfers or write-down.'),
        ('1-04-02-040','Medical, Dental and Laboratory Supplies for Distribution','asset',false,NULL,false,
         'This account is used to record the cost of medical, dental and laboratory supplies purchased/received for distribution. Credit this account for issuance to end-users, transfers or write-down.'),
        ('1-04-02-050','Agricultural and Marine Supplies for Distribution','asset',false,NULL,false,
         'This account is used to record the cost of fertilizers, pesticides and other marine and agricultural supplies for distribution. This includes supplies for aquaculture researches, environment protection/preservations and the like. Credit this account for issuance to end-users, transfers or write-down of the inventory.'),
        ('1-04-02-060','Agricultural Produce for Distribution','asset',false,NULL,false,
         'This account is used to record the cost of agricultural produce for distribution. This includes livestock and poultry; crops and fruits and other agricultural products. Credit this account for issuance to end-users, transfers, or write-down of agricultural produce.'),
        ('1-04-02-070','Textbooks and Instructional Materials for Distribution','asset',false,NULL,false,
         'This account is used to record the cost of textbooks and instructional materials including flipcharts, video clips/slides, and the like, purchased/received for distribution. Credit this account for issuance to end- users, transfers or write-down.'),
        ('1-04-02-080','Construction Materials for Distribution','asset',false,NULL,false,
         'This account is used to record cost of construction materials for distribution. Credit this account for issuance to end-users, transfers or write-down .'),
        ('1-04-02-090','Property and Equipment for Distribution','asset',false,NULL,false,
         'This account is used to record cost of property and equipment purchased/received for distribution. Credit this account for issuance to end-users, transfers or write-down.'),
        ('1-04-02-990','Other Supplies and Materials for Distribution','asset',false,NULL,false,
         'This account is used to record cost of purchased/acquired inventories not falling under any of the specific inventory accounts held for distribution. Credit this account for issuance to end-users, transfers or write-down.'),
        ('1-04-03-010','Raw Materials Inventory','asset',false,NULL,false,
         'This account is used to record cost of unprocessed materials that serve as component of produced goods. Credit this account for the issuance of raw materials for production, transfers or write-down of raw materials.'),
        ('1-04-03-020','Work-in-Process Inventory','asset',false,NULL,false,
         'This account is used to record cost of goods still in the process of being manufactured or fabricated to produce an end product. Credit this account upon completion of the product/inventory, transfers or write-down.'),
        ('1-04-03-030','Finished Goods Inventory','asset',false,NULL,false,
         'This account is used to record cost of completed/manufactured product available for sale or use. Credit this account for transfer to other inventory accounts or other disposal and write-down.'),
        ('1-04-04-010','Office Supplies Inventory','asset',false,NULL,false,
         'This account is used to record the cost or value of purchased/acquired office supplies such as bond paper, ink, and small tangible items like staple wire remover, puncher, stapler and other similar items for use in the government operations. Credit this account for issuance to end-users, transfers or other disposal.'),
        ('1-04-04-020','Accountable Forms, Plates and Stickers','asset',false,NULL,false,
         'This account is used to record the cost of accountable forms acquired for government operation. It includes accountable forms with and without face value, such as official receipts, passports, tickets, stickers, permit/license plates, LTO plates, and the like. Credit this account for issuance to end- users, transfers, or other disposal.'),
        ('1-04-04-030','Non-Accountable Forms Inventory','asset',false,NULL,false,
         'This account is used to record the cost of non-accountable forms such as pre-printed application forms, tax returns forms, accounting forms and the like. Credit this account for issuance to end-users, transfers, or other disposal.'),
        ('1-04-04-040','Animal/Zoological Supplies Inventory','asset',false,NULL,false,
         'This account is used to record the cost of food, medicines, veterinary and other maintenance needs of animals for use/consumption of government parks, zoos, wildlife sanctuaries and botanical gardens. This also includes supplies for zoological researches, preservations and the like. Credit this account for issuance to end-users, transfers or other disposal.'),
        ('1-04-04-050','Food Supplies Inventory','asset',false,NULL,false,
         'This account is used to record cost of food for hospital/rehabilitation patients, jail inmates and the like. Credit this account for issuance to end- users, transfers or other disposal.'),
        ('1-04-04-060','Drugs and Medicines Inventory','asset',false,NULL,false,
         'This account is used to record the cost of drugs and medicines purchased/received for government operations. Credit this account for issuance to end-users, transfers or other disposal.'),
        ('1-04-04-070','Medical, Dental and Laboratory Supplies Inventory','asset',false,NULL,false,
         'This account is used to record the cost of medical, dental and laboratory supplies purchased/received for government operations. Credit this account for issuance to end-users, transfers, or other disposal.'),
        ('1-04-04-080','Fuel, Oil and Lubricants Inventory','asset',false,NULL,false,
         'This account is used to record the cost of fuel and oil in government depots and lubricants for use in government vehicles and other equipment in connection with government operations/projects. Credit this account for issuance to end-users, transfers, or other disposal.'),
        ('1-04-04-090','Agricultural and Marine Supplies Inventory','asset',false,NULL,false,
         'This account is used to record the cost of fertilizers, pesticides and other marine and agricultural supplies for use in government operations. This includes supplies for aquaculture researches, environment

protection/preservations and the like. Credit this account for issuance to end-users, transfers, or other disposal.'),
        ('1-04-04-100','Textbooks and Instructional Materials Inventory','asset',false,NULL,false,
         'This account is used to record the cost of textbooks and instructional materials including flipcharts, video clips/slides, and the like, purchased/received for use in government schools operation. Credit this account for issuance to end-users, transfers or other disposal.'),
        ('1-04-04-110','Military, Police and Traffic Supplies Inventory','asset',false,NULL,false,
         'This account is used to record the cost of military, police and traffic supplies acquired for government operations such as truncheons, ammunitions, night sticks, clubs/cudgels, police/ traffic gears, flashlights, helmets and the like. Credit this account for issuance to end-users, transfers or other disposal.'),
        ('1-04-04-120','Chemical and Filtering Supplies Inventory','asset',false,NULL,false,
         'This account is used to record cost of chemicals and filtering supplies for use in government operations. Credit this account for issuance to end- users, transfers or other disposal.'),
        ('1-04-04-130','Construction Materials Inventory','asset',false,NULL,false,
         'This account is used to record cost of construction materials purchased/acquired for stock and later issuance for the construction, fabrication, repair and rehabilitation of government facilities. Credit this account for issuance to projects, transfers or other disposal.'),
        ('1-04-04-990','Other Supplies and Materials Inventory','asset',false,NULL,false,
         'This account is used to record cost of purchased/acquired inventories not falling under any of the specific supplies and materials inventory accounts held for consumption. Credit this account for issuance, transfers or other disposal.'),
        ('1-05-01-010','Advances to Contractors','asset',false,NULL,false,
         'This account is used to record amount advanced to contractors as authorized by law. Credit this account for recoupment of advances.'),
        ('1-05-01-020','Prepaid Rent','asset',false,NULL,false,
         'This account is used to record the amount advanced/deposited for lease/rentals of property, plant and equipment used in government operations. Credit this account for the expended amount.'),
        ('1-05-01-030','Prepaid Registration','asset',false,NULL,false,
         'This account is used to record the amount advanced for registration of government property. Credit this account for the expended amount.'),
        ('1-05-01-040','Prepaid Interest','asset',false,NULL,false,
         'This account is used to record the amount advanced for interest of loans contracted by the government. Credit this account for the expended amount.'),
        ('1-05-01-050','Prepaid Insurance','asset',false,NULL,false,
         'This account is used to record the amount advanced for insurance of government properties. Credit this account for the expended amount.'),
        ('1-05-01-990','Other Prepayments','asset',false,NULL,false,
         'This account is used to record the amount prepayments not falling under any of the specific prepayment accounts. Credit this account for the expended amount.'),
        ('1-05-02-010','Discount on Advance Payments','asset',false,NULL,false,
         'This account is used to record the discounts given to taxpayer for paying real property tax in advance. Credit this account for application of discount.'),
        ('1-06-01-010','Investment Property, Land','asset',false,NULL,false,
         'This account is used to record the cost of land or part of a land held by the owner (or by the lessee under a finance lease) to earn rentals or for capital appreciation or both. Credit this account for disposal or reclassification to Property, Plant and Equipment.'),
        ('1-06-01-011','Accumulated Impairment Losses - Investment Property, Land','asset',false,NULL,false,
         'This account is credited for the loss in the future economic benefit or service potential of the investment property. Debit this account upon disposal of the asset or reversal of impairment loss.'),
        ('1-06-01-020','Investment Property, Building','asset',false,NULL,false,
         'This account is used to record the cost of building or part of a building held by the owner (or by the lessee under a finance lease) to earn rentals or for capital appreciation or both. Credit this account for disposal or reclassification to Property, Plant and Equipment.'),
        ('1-06-01-021','Accumulated Depreciation - Investment Property, Buildings','asset',false,NULL,false,
         'This account is credited for the allocation of cost of building held as investment property in accordance with the prescribed policy on depreciation. Debit this account upon disposal of the asset or reclassification to Property, Plant and Equipment account.'),
        ('1-06-01-022','Accumulated Impairment Losses - Investment Property, Buildings','asset',false,NULL,false,
         'This account is credited for the loss in the future economic benefit or service potential of the investment property. Debit this account upon disposal of the asset or reversal of impairment loss.'),
        ('1-06-01-030','Construction in Progress - Investment Property, Buildings','asset',false,NULL,false,
         'This account is used to record the accumulated cost or other appropriate value of Investment Property, Buildings still in the process of construction or development. Credit this account for reclassification to the appropriate Investment Property account upon completion.'),
        ('1-07-01-010','Land','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value if acquired through donation or transfers without cost of land. Credit this account for disposal or reclassification to Investment Property.'),
        ('1-07-01-011','Accumulated Impairment Losses - Land','asset',false,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying cost of land to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment loss.'),
        ('1-07-02-010','Land Improvements - Aquaculture Structures','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of fishery and marine structures such as fishpond system, fish pens, fish tanks, and the like. Credit this account for disposal or derecognition of the asset.'),
        ('1-07-02-011','Accumulated Depreciation - Land Improvements, Aquaculture Structures','asset',true,NULL,false,
         'This account is credited for the allocation of cost of land improvements – aquaculture structures in accordance with the prescribed policy on depreciation. Debit this account upon disposal or derecognition of the asset.'),
        ('1-07-02-012','Accumulated Impairment Losses - Land Improvements, Aquaculture Structures','asset',true,NULL,false,
         'This account is credited for amount recognized to reduce the carrying value of land improvements, aquaculture structures to its recoverable amount. Debit this account upon disposal or derecognition of the asset or reversal of impairment loss.'),
        ('1-07-02-990','Other Land Improvements','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of land

improvements such as parking lots, landscape, walkways, driveways, covered walks, fences, and the like. Credit this account for disposal or derecognition of the asset.'),
        ('1-07-02-991','Accumulated Depreciation - Other Land Improvements','asset',true,NULL,false,
         'This account is credited for the allocation of cost of land improvements – aquaculture structures in accordance with the prescribed policy on depreciation. Debit this account upon disposal or derecognition of the asset.'),
        ('1-07-02-992','Accumulated Impairment Losses - Other Land Improvements','asset',true,NULL,false,
         'This account is credited for amount recognized to reduce the carrying value of other land improvements to its recoverable amount. Debit this account upon disposal or derecognition of the asset or reversal of impairment loss.'),
        ('1-07-03-010','Road Networks','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of roads, highways and bridges, railways, subways, and other road network facilities such as footbridge, traffic lights and road signages for public use. Credit this account for the derecognition of the carrying amount of the replaced portion.

Subsidiary ledgers shall be maintained for each road and each component of the road.'),
        ('1-07-03-011','Accumulated Depreciation - Road Networks','asset',true,NULL,false,
         'This account is used to record the allocation of cost of road networks in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.

Subsidiary ledger shall be maintained for the depreciation of each road component.'),
        ('1-07-03-012','Accumulated Impairment Losses - Road Networks','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of road networks to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.

Subsidiary ledger shall be maintained for the impairment of each road component.'),
        ('1-07-03-020','Flood Control System','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of seawalls, river walls, and other flood control system facilities for public use. Credit this account for the derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-021','Accumulated Depreciation - Flood Control System','asset',true,NULL,false,
         'This account is used to record the allocation of cost of flood control system in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-022','Accumulated Impairment Losses - Flood Control System','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of flood control system to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-030','Sewer Systems','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of waste treatment plants and other sewer system facilities for public use. This also includes pumps, purification works, rising/gravity mains, air release valves, screens, overflows and associated infrastructures. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-031','Accumulated Depreciation - Sewer Systems','asset',true,NULL,false,
         'This account is used to record the allocation of cost of sewer systems in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-032','Accumulated Impairment Losses - Sewer Systems','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of sewer systems to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-040','Water Supply Systems','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of water source facility such as artesian wells, dams/reservoirs, pumping stations and conduits; irrigation canals and laterals; waterways, aqueducts, water utilities systems and other water supply facilities for public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-041','Accumulated Depreciation - Water Supply Systems','asset',true,NULL,false,
         'This account is used to record the allocation of cost of water supply systems in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-042','Accumulated Impairment Losses - Water Supply Systems','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of water supply systems to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-050','Power Supply Systems','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of installations for generation and distribution of electricity such as power stations, electric transformers and other power and energy supply facilities for public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-051','Accumulated Depreciation - Power Supply Systems','asset',true,NULL,false,
         'This account is used to record the allocation of cost of power supply systems in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-052','Accumulated Impairment Losses - Power Supply Systems','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the power supply systems to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-060','Communication Networks','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of communication networks such as towers/transmitters/ transceivers and other communication/telecommunication facilities for public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-061','Accumulated Depreciation - Communication Networks','asset',true,NULL,false,
         'This account is used to record the allocation of cost of communication network in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-062','Accumulated Impairment Losses - Communication Networks','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the communication networks to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-070','Seaport Systems','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of ports, lighthouses, harbors and other seaport facilities, for public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-071','Accumulated Depreciation - Seaport Systems','asset',true,NULL,false,
         'This account is used to record the allocation of cost of seaport systems in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-072','Accumulated Impairment Losses - Seaport Systems','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the seaport systems to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-080','Airport Systems','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of landing and taking-off area for aircraft, passenger arrival and departure area, facilities for aircraft maintenance, and other airport facilities such as airport runways and taxiways, radio beacon, aprons, and the like, for public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-081','Accumulated Depreciation - Airport Systems','asset',true,NULL,false,
         'This account is used to record the allocation of cost of airport systems in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-082','Accumulated Impairment Losses - Airport Systems','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the airport systems to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-090','Parks, Plazas and Monuments','asset',false,NULL,false,
         'This account is used to record the cost incurred in the construction or fair value, if acquired through donation or transfers without cost, of parks, plazas and monuments, not classified as Heritage Assets, for recreation and public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-091','Accumulated Depreciation - Parks, Plazas and Monuments','asset',true,NULL,false,
         'This account is used to record the allocation of cost of parks, plazas and monuments in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-092','Accumulated Impairment Losses - Parks, Plazas and Monuments','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the parks, plazas and monuments to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-03-990','Other Infrastructure Assets','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of other public infrastructures which cannot be classified under any specific type of public infrastructures for public use or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion.'),
        ('1-07-03-991','Accumulated Depreciation - Other Infrastructure Assets','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other infrastructure assets in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-03-992','Accumulated Impairment Losses - Other Infrastructure Assets','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the other infrastructure assets to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-010','Buildings','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of buildings, such as: office buildings, research/convention/training centers, agricultural laboratories, warehouses, cold storage and the like, for use in government operations. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, disposal or reclassification to Investment Property.

Subsidiary ledgers shall be maintained for the components of the building depreciated separately from the building.'),
        ('1-07-04-011','Accumulated Depreciation - Buildings','asset',true,NULL,false,
         'This account is used to record the allocation of cost of buildings in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-012','Accumulated Impairment Losses - Buildings','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the buildings to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-020','School Buildings','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of school buildings for the implementation of government''s education/learning programs. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, or disposal.'),
        ('1-07-04-021','Accumulated Depreciation - School Buildings','asset',true,NULL,false,
         'This account is used to record the allocation of cost of school buildings in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-022','Accumulated Impairment Losses - School Buildings','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the school buildings to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-030','Hospitals and Health Centers','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of hospitals and health centers, for use in the delivery of public health services. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, or disposal.

Subsidiary ledgers shall be maintained for the components of the hospitals and health centers depreciated separately.'),
        ('1-07-04-031','Accumulated Depreciation - Hospitals and Health Centers','asset',true,NULL,false,
         'This account is used to record the allocation of cost of hospitals and health centers in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-032','Accumulated Impairment Losses - Hospitals and Health Centers','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the hospitals and health centers to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-040','Markets','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of markets for income generation. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, or disposal.'),
        ('1-07-04-041','Accumulated Depreciation - Markets','asset',true,NULL,false,
         'This account is used to record the allocation of cost of markets in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-042','Accumulated Impairment Losses - Markets','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the markets to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-050','Slaughterhouses','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of facilities where farm animals are butchered and processed. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, or disposal.'),
        ('1-07-04-051','Accumulated Depreciation - Slaughterhouses','asset',true,NULL,false,
         'This account is used to record the allocation of cost of slaughterhouses in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-052','Accumulated Impairment Losses - Slaughterhouses','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of the slaughterhouses to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-060','Hostels and Dormitories','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of hostels and dormitories for use in the operation/commercial and/or income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, or disposal.'),
        ('1-07-04-061','Accumulated Depreciation - Hostels and Dormitories','asset',true,NULL,false,
         'This account is used to record the allocation of cost of hostels and dormitories in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-062','Accumulated Impairment Losses - Hostels and Dormitories','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of hostels and dormitories to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-04-990','Other Structures','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or construction or fair value, if acquired through donation or transfers without cost, of other structures for use in government operations or for income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, or disposal.'),
        ('1-07-04-991','Accumulated Depreciation - Other Structures','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other structures in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-04-992','Accumulated Impairment Losses - Other Structures','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of other structures to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-010','Machinery','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of machinery. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-011','Accumulated Depreciation - Machinery','asset',true,NULL,false,
         'This account is used to record the allocation of cost of machinery in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-012','Accumulated Impairment Losses - Machinery','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of machinery to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-020','Office Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of office equipment for use in government operations. It includes duplicating/photocopying machines, air conditioning units and the like. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-021','Accumulated Depreciation - Office Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other office equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-022','Accumulated Impairment Losses - Office Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of office equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-030','Information and Communication Technology Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase of information and communication technology equipment, which includes hardware (computers, printers, scanners, and the like) and pre-loaded software such as but not limited to operating systems which are included in the cost of the computer hardware. This also includes data communications equipment and such other peripherals and auxiliary equipment necessary to put the system into operational mode. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-031','Accumulated Depreciation - Information and Communication Technology Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of information and communication technology equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-032','Accumulated Impairment Losses - Information and Communication Technology Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of information and communication technology equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-040','Agricultural and Forestry Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of agricultural and forestry equipment. It includes threshing and milling machines, plow, saw used in cutting/trimming/pruning trees, grass cutting machines, and the like. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-041','Accumulated Depreciation - Agricultural and Forestry Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of agricultural and forestry equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-042','Accumulated Impairment Losses - Agricultural and Forestry Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of agricultural and forestry equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-050','Marine and Fishery Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of marine and fishery equipment. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-051','Accumulated Depreciation - Marine and Fishery Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of marine and fishery equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-052','Accumulated Impairment Losses - Marine and Fishery Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of marine and fishery equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-060','Airport Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of airport equipment for use in government operations. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-061','Accumulated Depreciation - Airport Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of airport equipment in accordance with the prescribed policy on depreciation. Debit this account

upon derecognition of the asset.'),
        ('1-07-05-062','Accumulated Impairment Losses - Airport Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of airport equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-070','Communication Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of communication equipment for use in government operations. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-071','Accumulated Depreciation - Communication Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of communication equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-072','Accumulated Impairment Losses - Communication Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of communication equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-080','Construction and Heavy Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of construction and heavy equipment for use in government operations, such as: bulldozers, forklifts, graders, payloader, dump trucks, and the like. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-081','Accumulated Depreciation - Construction and Heavy Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of construction and heavy equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-082','Accumulated Impairment Losses - Construction and Heavy Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of construction and heavy equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-090','Disaster Response and Rescue Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of equipment used solely for disaster response and rescue activities. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.

Subsidiary Ledgers: 01 – Firefighting Equipment and Accessories 02 - Flood Rescue Equipment 03 - Earthquake Rescue Equipment 04 - Volcanic Eruption Rescue Equipment 05 - Landslide Rescue Equipment'),
        ('1-07-05-091','Accumulated Depreciation - Disaster Response and Rescue Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of disaster response and rescue equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-092','Accumulated Impairment Losses - Disaster Response and Rescue Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of disaster response and rescue equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-100','Military, Police and Security Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of military, police and security equipment acquired for use in government operations, such as: guns, armored vehicles, bomber aircraft, military tanks, mobile cars, patrol boats, navy ships, closed circuit television, security cameras, etc. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-101','Accumulated Depreciation - Military, Police and Security Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of military, police and security equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-102','Accumulated Impairment Losses - Military, Police and Security Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of military, police and security equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-110','Medical Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value if acquired through donation or transfers without cost of medical equipment acquired for delivery of medical services, such as: diagnostic equipments, (i.e. ultrasound, magnetic resonance imaging, CT scan, x-ray, reflex hammer, etc.), therapeutic equipment (i.e. infusion pumps, medical lasers, surgical machines, etc.), life support (i.e. ventilators, dialysis, etc.), monitors (electrocardiograph, electroencephalograph, sphygmomanometer, stethoscope, etc.). It also includes hospital beds, medicine trolleys, medical furniture, (stretchers, wheelchair), mobile clinics; and other hospital, dental and laboratory equipment. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-111','Accumulated Depreciation - Medical Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of medical equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-112','Accumulated Impairment Losses - Medical Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of medical equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-120','Printing Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of large

printing equipment, such as offset press, letter press, bindery equipment, prepress machine, and the like, for use in government printing needs. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-121','Accumulated Depreciation - Printing Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of printing equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-122','Accumulated Impairment Losses - Printing Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of printing equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-130','Sports Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of sports equipment for use in the government''s promotion of sports, such as: treadmills, stationary bikes, weights, gymnastic facilities, boxing rings, basketball goals, and the like. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-131','Accumulated Depreciation - Sports Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of sports equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-132','Accumulated Impairment Losses - Sports Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of sports equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-140','Technical and Scientific Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without

cost, of technical and scientific equipment for use in government operations, such as survey equipments (telescopes, laser distance meters), weather tracking equipment, cartographic, photographic and reprographic equipments, and other specialized equipments, (i.e. musical instruments), and the like. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-141','Accumulated Depreciation - Technical and Scientific Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of technical and scientific equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-142','Accumulated Impairment Losses - Technical and Scientific Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of technical and scientific equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-05-990','Other Machinery and Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of other machinery and equipment, not otherwise classified under the specific equipment accounts. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-05-991','Accumulated Depreciation - Other Machinery and Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other machinery and equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-05-992','Accumulated Impairment Losses - Other Machinery and Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of other machinery and equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-06-010','Motor Vehicles','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of motor vehicles for official use such as service vehicles (cars, bus, motorcycles, and the like). Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-06-011','Accumulated Depreciation - Motor Vehicles','asset',true,NULL,false,
         'This account is used to record the allocation of cost of motor vehicles in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-06-012','Accumulated Impairment Losses - Motor Vehicles','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of motor vehicles to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-06-020','Trains','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of trains for public use. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-06-021','Accumulated Depreciation - Trains','asset',true,NULL,false,
         'This account is used to record the allocation of cost of trains in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-06-022','Accumulated Impairment Losses - Trains','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of motor vehicles to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-06-030','Aircrafts and Aircrafts Ground Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or fair value, if acquired through donation or transfers without cost, of aircrafts and aircrafts ground equipment for civil aviation to transport passengers, equipments and goods. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-06-031','Accumulated Depreciation - Aircrafts and Aircrafts Ground Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of aircrafts and aircrafts ground equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-06-032','Accumulated Impairment Losses - Aircrafts and Aircrafts Ground Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of aircrafts and aircrafts ground equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-06-040','Watercrafts','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly or fair value, if acquired through donation or transfers without cost, of watercrafts used to ferry passengers, equipment and goods for government operations or public use such as ships, small vessels, speedboats, motor boats, rafts and bancas. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-06-041','Accumulated Depreciation - Watercrafts','asset',true,NULL,false,
         'This account is used to record the allocation of cost of watercrafts in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-06-042','Accumulated Impairment Losses - Watercrafts','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of watercrafts to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-06-990','Other Transportation Equipment','asset',false,NULL,false,
         'This account is used to record the cost incurred in the purchase or assembly, or fair value, if acquired through donation or transfers without cost, of other transportation equipment which are not classified under the specific transportation equipment accounts. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-06-991','Accumulated Depreciation - Other Transportation Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other transportation equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-06-992','Accumulated Impairment Losses - Other Transportation Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of other transportation equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-07-010','Furniture and Fixtures','asset',false,NULL,false,
         'This account is used to record the cost of acquisition or assembly, or fair value, if acquired through donation or transfers without cost, of furniture and fixtures for use in government operations. Credit this account for disposal (sale, transfers, destruction) and adjustments.'),
        ('1-07-07-011','Accumulated Depreciation - Furniture and Fixtures','asset',true,NULL,false,
         'This account is used to record the allocation of cost of furniture and fixtures in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-07-012','Accumulated Impairment Losses - Furniture and Fixtures','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of furniture and fixtures to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-07-020','Books','asset',false,NULL,false,
         'This account is used to record the cost of acquisition or production, or fair value, if acquired through donation or transfers without cost, of books and other reference materials including those in electronic copies (in CD/DVD), for use in government operations. Credit this account for disposal.'),
        ('1-07-07-021','Accumulated Depreciation - Books','asset',true,NULL,false,
         'This account is used to record the allocation of cost of books in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-07-022','Accumulated Impairment Losses - Books','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of books to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-08-010','Leased Assets - Land','asset',false,NULL,false,
         'This account is used to record the value of land under a finance lease. Credit this account to reclassify to the land account upon full payment of lease or to close the account upon termination of the lease contract.'),
        ('1-07-08-011','Accumulated Impairment Losses - Leased Assets, Land','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of land under finance lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-08-020','Leased Assets - Buildings and Other Structures','asset',false,NULL,false,
         'This account is used to record the value of buildings and other structures under a finance lease. Credit this account to reclassify to the buildings and other structures account upon full payment of lease or to close the account upon termination of the lease contract.'),
        ('1-07-08-021','Accumulated Depreciation - Leased Assets, Buildings and Other Structures','asset',true,NULL,false,
         'This account is used to record the allocation of cost of buildings and structures under finance lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-08-022','Accumulated Impairment Losses - Leased Assets, Buildings and Other Structures','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of building and other structures under finance lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-08-030','Leased Assets - Machinery and Equipment','asset',false,NULL,false,
         'This account is used to record the value of machinery and equipment under a finance lease. Credit this account to reclassify to the machinery and equipment account upon full payment of lease or to close the account upon termination of the lease contract'),
        ('1-07-08-031','Accumulated Depreciation - Leased Assets, Machinery and Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of machinery and equipment under finance lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-08-032','Accumulated Impairment Losses - Leased Assets, Machinery and Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of machinery and equipment under finance lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-08-040','Leased Assets - Transportation Equipment','asset',false,NULL,false,
         'This account is used to record the value of transportation equipment under a finance lease. Credit this account to reclassify to the transportation equipment account upon full payment of lease or to close the account upon termination of the lease contract.'),
        ('1-07-08-041','Accumulated Depreciation - Leased Assets, Transportation Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of transportation equipment under finance lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-08-042','Accumulated Impairment Losses - Leased Assets, Transportation Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of transportation equipment under finance lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-08-990','Other Leased Assets','asset',false,NULL,false,
         'This account is used to record the value of other property, plant and equipment under a finance lease. Credit this account to reclassify to the appropriate property, plant and equipment account upon full payment of lease or to close the account upon termination of the lease contract.'),
        ('1-07-08-991','Accumulated Depreciation - Other Leased Assets','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other leased assets under finance lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-08-992','Accumulated Impairment Losses - Other Leased Assets','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of other leased assets under finance lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-09-010','Leased Asset Improvements, Land','asset',false,NULL,false,
         'This account is used to record the cost of improvements and alterations made on leased land under operating lease which is used for government operations or for commercial and/or income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, disposal or termination of lease contract.'),
        ('1-07-09-011','Accumulated Depreciation - Leased Asset Improvements, Land','asset',true,NULL,false,
         'This account is used to record the allocation of cost of leased assets improvements on land under operating lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-09-012','Accumulated Impairment Losses - Leased Asset Improvements, Land','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of leased assets improvements on land under operating lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-09-020','Leased Asset Improvements, Buildings','asset',false,NULL,false,
         'This account is used to record the cost incurred in the construction of improvements on buildings under operating lease for use in government operations or for commercial and/or income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, disposal or termination of lease contract.'),
        ('1-07-09-021','Accumulated Depreciation - Leased Asset Improvements, Buildings','asset',true,NULL,false,
         'This account is used to record the allocation of cost of improvements on buildings under operating lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-09-022','Accumulated Impairment Losses - Leased Asset Improvements, Buildings','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of leased asset improvements on buildings under operating lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-09-990','Other Leased Asset Improvements','asset',false,NULL,false,
         'This account is used to record the cost incurred in the construction of improvements on other assets/structures under operating lease for use in government operations or for commercial and/or income generating purposes. Credit this account for derecognition of the carrying amount of the replaced portion in major repairs, disposal or termination of lease contract.'),
        ('1-07-09-991','Accumulated Depreciation - Other Leased Asset Improvements','asset',true,NULL,false,
         'This account is used to record the allocation of cost of improvements on other leased assets/structures under operating lease in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-09-992','Accumulated Impairment Losses - Other Leased Asset Improvements','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of leased asset improvements on other assets/structures under operating lease to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-10-010','Construction in Progress - Land Improvements','asset',false,NULL,false,
         'This account is used to record the accumulated cost or other appropriate value of land improvements still in the process of construction or development. Credit this account for reclassification to the appropriate Land Improvement account upon completion.'),
        ('1-07-10-020','Construction in Progress - Infrastructure Assets','asset',false,NULL,false,
         'This account is used to record the accumulated cost or other appropriate value of infrastructure assets which are still in the process of construction or acquisition. Credit this account for reclassification to the appropriate Infrastructure Asset account upon completion.'),
        ('1-07-10-030','Construction in Progress - Buildings and Other Structures','asset',false,NULL,false,
         'This account is used to record the accumulated cost or other appropriate value of buildings and other structures which are still in the process of construction or development. Credit this account for reclassification to the appropriate Buildings and Other Structures account upon completion.'),
        ('1-07-10-040','Construction in Progress - Leased Assets','asset',false,NULL,false,
         'This account is used to record the accumulated cost or other appropriate value of assets under finance lease, still in the process of construction or development. Credit this account for reclassification to the appropriate Leased Assets Improvement account upon completion.'),
        ('1-07-10-050','Construction in Progress - Leased Assets Improvements','asset',false,NULL,false,
         'This account is used to record the accumulated cost or other appropriate value of l improvements still in the process of construction or development on assets under operating lease. Credit this account for reclassification to the appropriate Leased Assets Improvements account upon completion.'),
        ('1-07-11-010','Service Concession Assets','asset',false,NULL,false,
         'This account is used to record the cost or fair value of assets recognized under service concession arrangements. Credit this account upon derecognition or transfer after the end of the concession agreement.

Maintain subsidiary ledger for each line of concession arrangement.'),
        ('1-07-11-011','Accumulated Depreciation - Service Concession Assets','asset',true,NULL,false,
         'This account is used to record the allocation of cost of service concession assets in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-11-012','Accumulated Impairment Losses - Service Concession Assets','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of service concession assets to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-99-010','Work/Zoo Animals','asset',false,NULL,false,
         'This account is used to record the cost or fair value, if acquired through donation or transfers without cost, of animals acquired by an agency for use in its operations, entertainment or recreation. This includes animals used in detective services, in zoos, wildlife sanctuaries, aviaries, aquariums, and the like. Credit this account for transfer to other agency or other disposal.'),
        ('1-07-99-011','Accumulated Depreciation - Work/Zoo Animals','asset',true,NULL,false,
         'This account is used to record the allocation of cost of work/zoo animals in accordance with the prescribed policy on depreciation. Debit this account

upon derecognition of the asset.'),
        ('1-07-99-012','Accumulated Impairment Losses - Work/Zoo Animals','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of work/zoo animals to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-07-99-990','Other Property, Plant and Equipment','asset',false,NULL,false,
         'This account is used to record the cost of acquisition or fair value, if acquired through donation or transfers without cost, of other property, plant and equipment not falling under any of the specific property, plant and equipment account. Credit this account for derecognition of the carrying amount of the replaced parts in major repairs, or disposal.'),
        ('1-07-99-991','Accumulated Depreciation - Other Property, Plant and Equipment','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other property, plant and equipment in accordance with the prescribed policy on depreciation. Debit this account upon derecognition of the asset.'),
        ('1-07-99-992','Accumulated Impairment Losses - Other Property, Plant and Equipment','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of other property, plant and equipment to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-08-01-010','Breeding Stocks','asset',false,NULL,false,
         'This account is used to record the cost or fair value, if acquired through donation or transfers without cost, of breeding stocks such as carabao, horse, swine and the like. Credit this account for sale, loss or death.'),
        ('1-08-01-020','Plants and Trees','asset',false,NULL,false,
         'This account is used to record the cost or fair value, if acquired through

donation or transfers without cost, of trees for forestry, bearer plants, annual or perennial cropping, cultivating orchards and plantations. Credit this account upon disposal or derecognition of asset.'),
        ('1-08-01-030','Aquaculture','asset',false,NULL,false,
         'This account is used to record the cost or fair value, if acquired through donation or transfers without cost, of fishes and other marine species. Credit this account disposal or derecognition of asset.'),
        ('1-08-01-990','Other Bearer Biological Assets','asset',false,NULL,false,
         'This account is used to record the cost or fair value, if acquired through donation or transfers without cost, of other agricultural biological assets not falling under any of the specific biological assets account. Credit this account for sale, loss or death.'),
        ('1-09-01-010','Patents/Copyrights','asset',false,NULL,false,
         'This account is used to record the cost of obtaining an exclusive legal right for an invention to enable its holder to manufacture, sell and control an item or process. Credit this account for disposal, expiration and/or revocation of the patent/copyright.'),
        ('1-09-01-011','Accumulated Amortization - Patents/Copyrights','asset',true,NULL,false,
         'This account is used to record the allocation of cost of patents/copyrights in accordance with the prescribed policy on amortization. Debit this account upon derecognition of the asset.'),
        ('1-09-01-012','Accumulated Impairment Losses - Patents/Copyrights','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of patents/copyrights to its recoverable amount due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-09-01-020','Computer Software','asset',false,NULL,false,
         'This account is used to record the purchase cost or capitalized development cost of computer software program for use in government operation. Development costs include cost of coding, testing and cost to produce product masters. Credit this account for obsolescence.'),
        ('1-09-01-021','Accumulated Amortization - Computer Software','asset',true,NULL,false,
         'This account is used to record the allocation of cost of computer software in accordance with the prescribed policy on amortization. Debit this account upon derecognition of the asset.'),
        ('1-09-01-022','Accumulated Impairment Losses - Computer Software','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of computer software to its recoverable amount or service potential due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-09-01-990','Other Intangible Assets','asset',false,NULL,false,
         'This account is used to record the cost of obtaining other non-monetary assets without physical substance which grants the owner legal and contractual rights and future economic benefits not otherwise classified under the specific intangible assets account. This includes trademarks, motion picture films, marketing rights, franchise and the like. Credit this account for termination and/or revocation of rights and benefits.'),
        ('1-09-01-991','Accumulated Amortization - Other Intangible Assets','asset',true,NULL,false,
         'This account is used to record the allocation of cost of other intangible assets in accordance with the prescribed policy on amortization. Debit this account upon derecognition of the asset.'),
        ('1-09-01-992','Accumulated Impairment Losses - Other Intangible Assets','asset',true,NULL,false,
         'This account is credited for the amount recognized to reduce the carrying value of other intangible assets to its recoverable amount or service potential due to impairment. Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('1-09-02-010','Service Concession Assets - Intangible Assets','asset',false,NULL,false,
         'This account is debited for the amount recognized representing the cost of intangible assets Debit this account upon derecognition of the asset or reversal of impairment.'),
        ('2-01-01-010','Accounts Payable','liability',false,NULL,false,
         'This account is used to record receipt of goods or services on account in the normal course of trade and business operation. Debit this account for payment or settlement of liabilities.'),
        ('2-01-01-020','Due to Officers and Employees','liability',false,NULL,false,
         'This account is used to record incurrence of liability to officers and employees for salaries, benefits and other emoluments. Debit this account for settlement or payment to officers and employees.'),
        ('2-01-01-040','Notes Payable','liability',false,NULL,false,
         'This account is used to record issuance of promissory notes. Debit this account for payment or settlement of promissory notes.'),
        ('2-01-01-050','Interest Payable','liability',false,NULL,false,
         'This account is used to record accrual of interest on loans/bonds or other indebtedness, whether short-term or long-term. Debit this account for payment of the interest.'),
        ('2-01-01-060','Operating Lease Payable','liability',false,NULL,false,
         'This account is used to record incurrence of liability arising from operating lease contract. Debit this account for payment of lease.'),
        ('2-01-01-070','Finance Lease Payable','liability',false,NULL,false,
         'This account is used to record incurrence of liability arising from finance lease contract. Debit this account for payment of lease.'),
        ('2-01-01-080','Awards and Rewards Payable','liability',false,NULL,false,
         'This account is used to record granting of awards in recognition of any civic or professional achievement and of rewards to informers for the receipt of reliable information leading to successful arrest/capture of fugitives, seizure/confiscation of smuggled goods, or collection/recovery of unpaid taxes/surcharges/fines/penalties. Debit this account for payment to the grantee of the award/reward.'),
        ('2-01-01-090','Service Concession Arrangement Payable','liability',false,NULL,false,
         'This account is used to record the liability arising from unconditional obligation of the grantor to make payments to the operator upon recognition of service concession assets. Debit this account for payment.'),
        ('2-01-01-100','Pension Benefits Payable','liability',false,NULL,false,
         'This account is used to record accrual of pension to government employees. Debit this account for payment of pension benefits.'),
        ('2-01-01-110','Leave Benefits Payable','liability',false,NULL,false,
         'This account is used to record the money value of the earned leave credits of government personnel. Debit this account for payment of leave benefits.'),
        ('2-01-01-120','Retirement Gratuity Payable','liability',false,NULL,false,
         'This account is used to record the liability for retirement gratuity benefits due to government personnel. Debit this account upon payment of retirement gratuity benefits.'),
        ('2-01-02-020','Bonds Payable - Domestic','liability',false,NULL,false,
         'This account is used to record issuances/flotation of peso-denominated bonds. Debit this account for redemptions of bonds.'),
        ('2-01-02-021','Discount on Bonds Payable - Domestic','liability',true,NULL,false,
         'This account is used to record discounts on issuance of peso-denominated bonds. Credit this account for amortization of the discount.'),
        ('2-01-02-022','Premium on Bonds Payable - Domestic','liability',false,NULL,false,
         'This account is used to record premiums on issuance of peso-denominated bonds. Debit this account for amortization of the premium.'),
        ('2-01-02-040','Loans Payable - Domestic','liability',false,NULL,false,
         'This account is used to record receipt of loan proceeds (cash or non-cash) from local creditors. Debit this account for payments of loan amortization.'),
        ('2-01-02-050','Loans Payable - Foreign','liability',false,NULL,false,
         'This account is used to record receipt of loan proceeds (cash or non-cash) from foreign governments or international financial institutions. Debit this account for payments of loan amortization.'),
        ('2-02-01-010','Due to BIR','liability',false,NULL,false,
         'This account is used to record withholding of taxes from officers/employees and other entities. Debit this account for remittance of the taxes withheld to the Bureau of Internal Revenue (BIR).'),
        ('2-02-01-020','Due to GSIS','liability',false,NULL,false,
         'This account is used to record the withholding of employees'' premium payment and other payables for remittance to the Government Service Insurance System (GSIS). Debit this account for the remittance of withheld amount.

Subsidiary Ledgers: 01 - Life and Retirement Premium 02 - ECC 03 - Salary Loan 04 - Policy Loan'),
        ('2-02-01-030','Due to Pag-IBIG','liability',false,NULL,false,
         'This account is used to record the withholding of employees'' premium payment and other payables for remittance to the Home Development Mutual Fund (HDMF). Debit this account for the remittance of withheld amount.

Subsidiary Ledgers: 01 – Pag-ibig Premium 02 – Pag-ibig Multi-Purpose Loan 03 - Pag-ibig Housing Loan'),
        ('2-02-01-040','Due to PhilHealth','liability',false,NULL,false,
         'This account is used to record the withholding of employees'' premium payment for remittance to the Philippine Health Insurance Corporation (PHIC). Debit this account for the remittance of withheld amount.'),
        ('2-02-01-050','Due to National Government Agencies','liability',false,NULL,false,
         'This account is used to record the receipt of funds for delivery of goods/services as authorized by law, fund transfers from NGAs for implementation of specific programs or projects and other inter-agency transactions. Debit this account for delivery of goods /services, liquidation of funds received and settlement of liabilities.'),
        ('2-02-01-060','Due to GOCCs','liability',false,NULL,false,
         'This account is used to record the receipt of: a) funds for delivery of goods/services as authorized by law; b) fund transfers from the GOCCs for the implementation of specific programs or projects; c) NG-managed fund; d) escrow accounts; e) collateral deposits for the account of GOCCs/GFIs; and f) other inter-agency transactions except those pertaining to GSIS, Philhealth and Pag-ibig. Debit this account for delivery of goods /services, liquidation/release of funds received and settlement of liabilities.'),
        ('2-02-01-070','Due to LGUs','liability',false,NULL,false,
         'This account is used to record the receipt of funds for delivery of goods/services as authorized by law, fund transfers from LGUs for the implementation of specific programs or projects, share in LGU income and other inter-agency transactions. Debit this account for delivery of goods/services, liquidation of funds received, receipt of share in LGU income and settlement of liabilities.'),
        ('2-02-01-080','Due to Joint Venture','liability',false,NULL,false,
         'This account is used to record the amounts payable to joint venture. Debit this account for settlement of liabilities.'),
        ('2-03-01-010','Due to Other Funds','liability',false,NULL,false,
         'This account is used to record the authorized receipt of funds from one fund to another fund maintained by the same agency. Debit this account for transfer of the funds to another fund.

The Subsidiary Ledgers shall be the different funds maintained by the agency.'),
        ('2-03-01-020','Due to Special Accounts','liability',false,NULL,false,
         'This account is used to record the authorized receipt of funds from one special account to another maintained by the same agency. Debit this account for transfer of the funds.

The Subsidiary Ledgers shall be the different special accounts in the general fund of the agency.'),
        ('2-03-01-030','Due to Local Economic Enterprise','liability',false,NULL,false,
         'This account is used to record the authorized receipt of funds from one local economic enterprise to another maintained in the same fund of the agency. Debit this account for transfer of the funds.

The Subsidiary Ledgers shall be the different local economic enterprise maintained of the agency.'),
        ('2-04-01-010','Trust Liabilities','liability',false,NULL,false,
         'This account is used to record the receipt of amount held in trust for specific purpose. Debit this account for payment or settlement of the liability or compliance of condition.'),
        ('2-04-01-020','Trust Liabilities - Disaster Risk Reduction and Management Fund','liability',false,NULL,false,
         'This account is used to record the receipt of amount held in trust for disaster risk reduction and management fund. Debit this account for settlement or compliance of the condition.

Subsidiary ledgers shall be maintained based on source. Unspent DRRM funds of LGUs from current year appropriations shall likewise maintain subsidiary ledger by year.'),
        ('2-04-01-030','Bail Bonds Payable','liability',false,NULL,false,
         'This account is used to record the incurrence of liability arising from the receipt of cash bond from a person who is in the custody of the law to guaranty his appearance in court at the appointed day and time or the compliance with the conditions of the bond. Debit this account for refund or forfeiture of the bond.'),
        ('2-04-01-040','Guaranty/Security Deposits Payable','liability',false,NULL,false,
         'This account is used to record the incurrence of liability arising from the receipt of cash or cash equivalents to guaranty (a) that the winning bidder shall enter into contract with the procuring entity; and (b) performance by the contractor of the terms of the contract. Debit this account for refund after the fulfillment of the purpose of the bond or forfeiture upon failure to comply with the purpose of the bond.'),
        ('2-04-01-050','Customers’ Deposits','liability',false,NULL,false,
         'This account is used to record the receipt of cash deposits from customers for goods/services to be delivered and property to be leased. Debit this account upon application of the deposit to the cost of the goods/services delivered and of the damages to the leased property.'),
        ('2-05-01-010','Deferred Real Property Tax','liability',false,NULL,false,
         'This account is used to record the receipt of advance payment for basic real property tax on land, building, machinery and other improvements. Debit this account when Real Property Tax is earned.'),
        ('2-05-01-020','Deferred Special Education Fund Tax','liability',false,NULL,false,
         'This account is used to record the receipt of advance payment of additional levy on the assessed value of real property. Debit this account when Special Education Tax is earned.'),
        ('2-05-01-030','Deferred Finance Lease Revenue','liability',false,NULL,false,
         'This account is used to set up in the books of the lessor the unearned income from finance lease. Debit this account when lease income is earned.'),
        ('2-05-01-040','Deferred Service Concession Revenue','liability',false,NULL,false,
         'This account is used to record the unearned service concession revenue arising from the grant to operator the right to earn revenue. Debit this account as revenue is recognized.'),
        ('2-05-01-050','Unearned Revenue - Investment Property','liability',false,NULL,false,
         'This account is used to record the receipt of advance rent/lease income from investment property. Debit this account when the revenue is earned.'),
        ('2-05-01-990','Other Deferred Credits','liability',false,NULL,false,
         'This account is used to record the other income received in advance not falling under any of the specific deferred credits accounts. Debit this account when related income is earned.'),
        ('2-06-01-040','Termination Benefits','liability',false,NULL,false,
         'This account is used to record other liabilities incurred of uncertain timing for termination of employees in view of division or consolidation of offices or political subdivisions. Debit this account for settlement of liabilities.'),
        ('2-06-01-990','Other Provisions','liability',false,NULL,false,
         'This account is used to record liabilities incurred of uncertain timing . This includes provisions due to litigations, provisions for dismantling costs of buildings and other claims. Debit this account for settlement of liabilities.'),
        ('2-99-99-990','Other Payables','liability',false,NULL,false,
         'This account is used to record other liabilities not falling under any of the specific payable accounts. Debit this account for settlement of liabilities.'),
        ('3-01-01-010','Government Equity','equity',false,NULL,false,
         'This account represents the cumulative results of normal and continuous operations of an agency including prior period adjustments, effect of changes in accounting policy and other capital adjustments.'),
        ('3-01-01-020','Prior Period Adjustment','equity',false,NULL,false,
         'This account is used to record prior period adjustments due to effect of changes in accounting policy and adjustments due to errors. This account is closed to the government equity account.'),
        ('3-02-01-010','Income and Expense Summary','equity',false,NULL,false,
         'This is an intermediary account used to close in the agency books all the income and expense accounts of the LGU. This account is closed to the Accumulated Surplus.'),
        ('3-03-01-010','Equity in Joint Venture','equity',false,NULL,false,
         'This account represents the equity shareholding in a joint venture.'),
        ('3-04-01-010','Unrealized Gain/(Loss) from changes in the Fair Value of Financial Assets','equity',false,NULL,false,
         'This account represents the unrealized gain/(Loss) from changes in the fair value of financial assets recognized directly in the net assets/equity.'),
        ('4-01-01-020','Professional Tax','income',false,NULL,false,
         'This account is used to record the taxes imposed on all persons engaged in the exercise/practice of their professions requiring government examination.'),
        ('4-01-01-050','Community Tax','income',false,NULL,false,
         'This account is used to record the taxes levied upon every inhabitant of the Philippines, 18 years of age or over, who have been regularly employed for at least 30 continuous working days during the calendar year; or who is in business or occupation; who owns real property with an aggregate assessed value of P1,000 or more and who is required by law to file an income tax return. This account also includes the annual community tax levied upon partnership/corporation engaged in or doing business in the Philippines and required by law to file an annual income tax return.

Subsidiary ledger:

51 - Community Tax – Individual 52 - Community Tax – Corporation'),
        ('4-01-02-040','Real Property Tax - Basic','income',false,NULL,false,
         'This account is used to record the basic tax imposed on real property such as land, buildings, machinery and improvements affixed or attached to the real property.

Subsidiary ledger:

Subsidiary:

01 - Real Property Tax Land 02 - Real Property Tax Building 03 - Real Property Tax Machinery'),
        ('4-01-02-041','Discount on Real Property Tax - Basic','income',true,NULL,false,
         'This account is used to record the discount on the basic tax imposed on real property. It is a deduction from the total real property tax.'),
        ('4-01-02-050','Special Education Tax','income',false,NULL,false,
         'This account is used to record the tax levied on real property (land, building machinery and improvement) in addition to the basic real property tax. The proceeds accrue exclusively to the Special Education Fund.'),
        ('4-01-02-051','Discount on Special Education Tax','income',true,NULL,false,
         'This account is used to record the discount on the special education tax levied on real property. It is a deduction from the total special education tax.'),
        ('4-01-02-060','Special Levy on Idle Lands','income',false,NULL,false,
         'This account is used to record the additional ad valorem tax levied on idle lands in addition to the basic real property tax.'),
        ('4-01-02-070','Special Levy on Lands Benefited by Public Works Projects','income',false,NULL,false,
         'This account is used to record the special levy imposed on lands comprised with territorial jurisdiction specially benefited by public works and projects or improvements funded by local governments concerned.'),
        ('4-01-02-080','Real Property Transfer Tax','income',false,NULL,false,
         'This account is used to record the tax imposed on the transfer of ownership or title of real property (by sale, donation/barter or other modes).'),
        ('4-01-03-030','Business Tax','income',false,NULL,false,
         'This account is used to record the taxes on persons or entities in the course of trade or business.

Subsidiary ledgers:

01 – Business Tax – Manufacturers, assemblers, etc. – this includes taxes imposed on manufactures, assemblers, repackers, processors, brewers, distillers, rectifiers and compounders of liquors, distilled spirits, and wines or manufacturers of any article of commerce. (Sec. 143 (a) of RA 7160. 02 – Business Tax – Wholesalers, distributors, etc. – this includes taxes imposed on wholesalers, distributors or dealers in any article of commerce. (Sec. 143 (b) o RA 7160 03 – Business Tax – Exporters, etc. – this includes taxes on exporters, and on manufacturers, millers, producers, wholesalers, distributors, dealers or retailers of essential commodities. (Sec. 143 (c). 04 – Business Tax- Retailers – this includes taxes imposed on business establishments engaged in the sale of goods where the purchaser buys the commodity for their own consumption irrespective of the quantity of the commodity sold. 05 – Business Tax- Contractors and Independent Contractors – this includes taxes imposed on persons (natural or juridical) not subject to professional tax involve in the sale of all kinds of services for a fee. 06 – Business Tax – Banks and other Financial Institutions – this includes business taxes imposed on banks and banking institutions, insurance companies and financing companies. 07 – Business Tax - Peddlers – this includes business taxes imposed on retailers selling their commodities on foot or motorized along the way. 99 – Business Tax – Other Businesses - this includes business taxes imposed not otherwise previously classified.'),
        ('4-01-03-040','Tax on Sand, Gravel and Other Quarry Products','income',false,NULL,false,
         'This account is used to record the taxes imposed by local government units on sand, gravel, stones, earth and other quarry resources extracted from public lands or from beds of seas, lakes, rivers, streams, creeks and other public waters within the LGU''s territorial jurisdiction.'),
        ('4-01-03-050','Tax on Delivery Trucks and Vans','income',false,NULL,false,
         'This account is used to record the annual fixed tax levied on delivery trucks, vans or any vehicles used by manufacturers, producers, wholesalers, dealers or retailers in the delivery or distribution of distilled spirits, fermented liquors, soft drinks, cigars and cigarettes and other products to sales outlets, or consumers, whether directly or indirectly within the locality.'),
        ('4-01-03-060','Amusement Tax','income',false,NULL,false,
         'This account is used to record the tax imposed by local government units on the proprietors, lessees or operators of theaters, cinemas, concert halls, circuses, boxing stadia, and other places of amusement (Sec. 140 RA 7160).'),
        ('4-01-03-070','Franchise Tax','income',false,NULL,false,
         'This account is used to record the tax imposed by local government units on businesses enjoying franchise.'),
        ('4-01-03-080','Printing and Publication Tax','income',false,NULL,false,
         'This account is used to record taxes imposed on the business of persons engaged in the printing/publication of books, cards, posters, leaflets, handbills, certificates, receipts, pamphlets and others of similar nature'),
        ('4-01-04-990','Other Taxes','income',false,NULL,false,
         'This account is used to record all other taxes levied not falling under any of the specific tax accounts.'),
        ('4-01-05-010','Tax Revenue - Fines and Penalties - Taxes on Individual and Corporation','income',false,NULL,false,
         'This account is used to record all fines and penalties charged in relation to the collection of taxes on individual and corporation.'),
        ('4-01-05-020','Tax Revenue - Fines and Penalties - Property Taxes','income',false,NULL,false,
         'This account is used to record all fines and penalties charged in relation to the collection of property taxes.

01 - Real Property Tax Basic Land – Penalty 02 - Real Property Tax Basic Building – Penalty 03 - Real Property Tax Basic Machinery – Penalty 04 - Special Education Tax – Penalty - Others'),
        ('4-01-05-030','Tax Revenue - Fines and Penalties - Taxes on Goods and Services','income',false,NULL,false,
         'This account is used to record all fines and penalties charged in relation to the collection of taxes on goods and services'),
        ('4-01-05-040','Tax Revenue - Fines and Penalties - Other Taxes','income',false,NULL,false,
         'This account is used to record all fines and penalties imposed on other taxes.'),
        ('4-01-06-010','Share from Internal Revenue Collections','income',false,NULL,false,
         'This account is used to record the share of local government units from the national internal revenue taxes collected.'),
        ('4-01-06-020','Share from Expanded Value Added Tax','income',false,NULL,false,
         'This account is used to record the share of local government units in incremental collection from value added tax (VAT). This account also includes share from VAT in lieu of franchise tax collected from racetrack operations of the Manila Jockey Club, Inc. and the Philippine Racing Club, Inc.'),
        ('4-01-06-030','Share from National Wealth','income',false,NULL,false,
         'This account includes the share of local government units in the utilization and development of the national wealth and from its share in any co- production, joint venture or production sharing agreement in the utilization and development of national wealth within their respective territorial jurisdiction. This also includes the share of the local government unit based on the preceding fiscal year proceeds derived by any government agency or government-owned or controlled corporation engaged in the utilization and development of national wealth.

Subsidiary ledgers:

01- Share from Mining Taxes 02- Share from Royalty Fees 03- Share from Proceeds Derived from Hydro-Power Plants, Geothermal and Other Sources of Energy 04- Share from Forestry and Fishery Charges'),
        ('4-01-06-040','Share from Tobacco Excise Tax (RA 7171 and 8240)','income',false,NULL,false,
         'This account is used to record the share of local government units from tobacco excise taxes as provided under R.A. No. 7171 and 8240.'),
        ('4-01-06-050','Share from Economic Zones','income',false,NULL,false,
         'This account is used to record the share of local government units from the five percent of the gross income tax paid by all businesses and enterprises located within ECOZONES (economic, industrial and commercial parks or zones).'),
        ('4-02-01-010','Permit Fees','income',false,NULL,false,
         'This account is used to record the fees and charges collected in the exercise of regulatory powers. This includes permit fees collected from businesses, building, zonal location and others.

Subsidiary ledgers:

05- Business Permit Fees 06- Building Permit Fees 07- Zonal/Location Permit Fees 08- Tricycle Operators Permit Fees 09- Fishery Rental Fees and Privilege Fees 10- Fees on Weights and Measures 11- Other Permit and Licenses'),
        ('4-02-01-020','Registration Fees','income',false,NULL,false,
         'This account is used to record the fees collected for civil registration, animal registration, etc.

Subsidiary ledgers:

01 - Civil Registration Fees 02 - Cattle/Animal Registration Fees'),
        ('4-02-01-030','Registration Plates, Tags and Stickers Fees','income',false,NULL,false,
         'This account is used to record fees collected for registration plates, tags and stickers issued by local government units.'),
        ('4-02-01-040','Clearance and Certification Fees','income',false,NULL,false,
         'This account is used to record the fees collected for the issuance of clearances/certificates to individuals/organizations/groups/corporations.

Subsidiary ledgers:

01 Police Clearance 02 Secretary''s Fees 03 Health Certificate 04 Real Property Tax Clearance 05 Other Clearance and Certification'),
        ('4-02-01-070','Supervision and Regulation Enforcement Fees','income',false,NULL,false,
         'This account is used to record the fees and charges collected for supervision and enforcement of laws and regulations, such as traffic enforcement and other LGU regulations and the like.'),
        ('4-02-01-100','Inspection Fees','income',false,NULL,false,
         'This account is used to record the fees for conduct of inspections by authorized government officials. This includes health inspection fees, sanitary inspection fees, mechanical inspection fees, etc.'),
        ('4-02-01-110','Verification and Authentication Fees','income',false,NULL,false,
         'This account is used to record the fees collected for verification of official documents on record. This account also includes fees collected for issuance of authenticated copies of legal documents such as birth/death/marriage certificates.'),
        ('4-02-01-130','Processing Fees','income',false,NULL,false,
         'This account is used to record the fees collected for the processing of documents for securing permits/application. This also includes fees for processing birth certificates and other civil registry certificates, permit application and the like.'),
        ('4-02-01-140','Occupation Fees','income',false,NULL,false,
         'This account is used to record the fees imposed on all individuals employed or practicing their callings which do not require board examinations.'),
        ('4-02-01-150','Fishery Rentals, Fees and Charges','income',false,NULL,false,
         'This account is used to record the fees imposed on individuals/entity granted fishery privileges in the municipal waters in accordance with Sec. 149 of RA 7160.'),
        ('4-02-01-160','Fees for Sealing and Licensing of Weights and Measures','income',false,NULL,false,
         'This account is used to record the fees imposed for sealing and licensing of weights and measures in accordance with Sec. 148 of RA 7160.'),
        ('4-02-01-980','Fines and Penalties - Service Income','income',false,NULL,false,
         'This account is used to record the fees for fines and penalties imposed for delayed or non- payment of service fees.'),
        ('4-02-01-990','Other Service Income','income',false,NULL,false,
         'This account is used to record the fees collected from service income not falling under any of the specific service income accounts.'),
        ('4-02-02-010','School Fees','income',false,NULL,false,
         'This account is used to record the fees imposed to students of local universities/colleges and public schools. It includes tuition, registration, affiliation, athletic, cultural, diploma and graduation, transcript of records, library, training, entrance examination fee, comprehensive examination and the like.'),
        ('4-02-02-020','Affiliation Fees','income',false,NULL,false,
         'This account is used to record the fees collected by local institutions from students undertaking practice of their professions.'),
        ('4-02-02-040','Seminar/Training Fees','income',false,NULL,false,
         'This account is used to record the fees collected or billed for attendance in short course trainings and seminars.'),
        ('4-02-02-050','Rent Income','income',false,NULL,false,
         'This account is used to record the income from use of government properties/facilities, such as rent/lease of function rooms/buildings, fees on storage, warehousing, use of heavy equipment of other government agencies, and receipt of contingent rent. Contingent rent are additional rent income on top of the agreed rate as provided in the lease agreement.'),
        ('4-02-02-060','Communication Network Fees','income',false,NULL,false,
         'This account is used to record the fees collected from domestic money transfer, social telegram service, two-way radio service, public calling service, fixed line telephone service, telegraphic transfer, electronic messaging service and other related services. It includes fees collected for the connection of telephone services and the monthly fees for the use of the facility.'),
        ('4-02-02-070','Transportation System Fees','income',false,NULL,false,
         'This account is used to record the income from the operation of transportation system like trains, buses, watercrafts, aircrafts, and the use of land transport system terminals.'),
        ('4-02-02-080','Road Network Fees','income',false,NULL,false,
         'This account is used to record the toll fees charged for use of road networks and bridges.'),
        ('4-02-02-090','Waterworks System Fees','income',false,NULL,false,
         'This account is used to record the income from the operation of waterworks system, such as water connection fees, water utilization fees, irrigation fees, and the like.'),
        ('4-02-02-100','Power Supply System Fees','income',false,NULL,false,
         'This account is used to record the income earned from power generation, transmission and distribution.'),
        ('4-02-02-110','Seaport System Fees','income',false,NULL,false,
         'This account is used to record the fees imposed on vessels for use of port facilities, such as loading/discharging of cargoes, embarking/disembarking passengers, bunkering and anchoring at the port; fees charged for cargoes whether for domestic shipment or transshipment and security fee for the transport of containers.'),
        ('4-02-02-120','Parking Fees','income',false,NULL,false,
         'This account is used to record the fees imposed (fixed and ambulant) on use of public areas as parking fees, and the like.'),
        ('4-02-02-130','Receipts from Operation of Hostels/Dormitories and Other Like Facilities','income',false,NULL,false,
         'This account is used to record the fees/charges for the use of hostels and dormitories, cottages, guest houses, and the like.'),
        ('4-02-02-140','Receipt from Market Operation','income',false,NULL,false,
         'This account is used to record the collections from the operations of market. It includes rental of market spaces (fixed and ambulant) and other income earning facilities of the market.'),
        ('4-02-02-150','Receipts from Slaughterhouse Operation','income',false,NULL,false,
         'This account is used to record the collections from the operations of slaughterhouse which includes rentals of spaces for warehousing, livestock slaughter fees and share from ante/post-mortem fees.'),
        ('4-02-02-160','Receipts from Cemetery Operation','income',false,NULL,false,
         'This account is used to record the collections from the operations of cemetery which includes rentals of spaces for interment and operation of the crematorium and columbary.'),
        ('4-02-02-170','Receipts from Printing and Publication','income',false,NULL,false,
         'This account is used to record the receipts from sale of printed forms, materials or other publications.'),
        ('4-02-02-180','Sales Revenue','income',false,NULL,false,
         'This account is used to record the income from sale of merchandise and other inventory items and services in the regular course of business.'),
        ('4-02-02-181','Sales Discounts','income',false,NULL,false,
         'This account is used to record the discounts on sale of goods and services in the regular course of business. This account is closed to sales revenue.'),
        ('4-02-02-190','Garbage Fees','income',false,NULL,false,
         'This account is used to record the fees for garbage collection and other environmental and sanitation fees.'),
        ('4-02-02-200','Hospital Fees','income',false,NULL,false,
         'This account is used to record the fees/charges for hospital services rendered including medical, dental and laboratory services.'),
        ('4-02-02-210','Dividend Income','income',false,NULL,false,
         'This account is used to record the dividends earned from equity investments.'),
        ('4-02-02-220','Interest Income','income',false,NULL,false,
         'This account is used to record the interest earned on loans receivable, investments, and bank deposits.'),
        ('4-02-02-230','Service Concession Revenue','income',false,NULL,false,
         'This account is used to record the revenue recognized for the allocation of the cost of the asset put up by the operator for use in the service concession agreement.'),
        ('4-02-02-240','Other Service Concession Revenue','income',false,NULL,false,
         'This account is used to record the revenue received from operators of service concession asset in excess of any threshold set for the operator''s

collections.'),
        ('4-02-02-250','Lease Revenue','income',false,NULL,false,
         'This account is used to record the revenue recognized from collection of finance lease receivable'),
        ('4-02-02-260','Share in the Profit from Joint Venture','income',false,NULL,false,
         'This account is used to record the share of government agencies from profit on joint venture operations.'),
        ('4-02-02-980','Fines and Penalties - Business Income','income',false,NULL,false,
         'This account is used to record the fines, penalties charged for delayed or non-compliance with business regulatory requirements.'),
        ('4-02-02-990','Other Business Income','income',false,NULL,false,
         'This account is used to record the income not falling under any of the specific business income accounts.'),
        ('4-03-01-010','Subsidy from National Government','income',false,NULL,false,
         'This account is used to record transfers from the National government without specific purpose or stipulation.'),
        ('4-03-01-020','Subsidy from Local Government Units','income',false,NULL,false,
         'This account is used to record the amount of funds/assets transferred from other LGUs (Province, City, Municipality or Barangay) without specific purpose or condition.'),
        ('4-03-01-030','Subsidy from Government-Owned and/or Controlled Corporations (GOCCs)','income',false,NULL,false,
         'This account is used to record the amount of funds/assets transferred from GOCCs without specific purpose or condition.'),
        ('4-03-01-040','Subsidy from Other Funds','income',false,NULL,false,
         'This account is used to record the financial assistance received by one fund from another fund maintained by the same government agency as expressly authorized by law.'),
        ('4-03-01-050','Subsidy from General Fund Proper /Other Special Accounts','income',false,NULL,false,
         'This account is used to record transfers of funds from the general fund proper to an economic enterprise/special accounts of the same LGU.'),
        ('4-03-01-060','Subsidy from Local Economic Enterprises','income',false,NULL,false,
         'This account is used to record transfers of funds as subsidy from economic enterprises/special accounts to general fund proper or to other economic enterprises/special accounts of the same LGU.

Maintain subsidiary for each economic enterprise.'),
        ('4-03-02-010','Transfers from General Fund of LGU Counterpart/Equity Share','income',false,NULL,false,
         'This account is used to record the receipt of LGU counterpart or equity share.'),
        ('4-03-02-020','Transfers from General Fund of Unspent DRRMF','income',false,NULL,false,
         'This account is used to record the receipt of the unspent current year appropriations for DRRMF.'),
        ('4-04-01-010','Share from Philippine amusement and Gaming Corporation (PAGCOR)','income',false,NULL,false,
         'This account is used to record the share of the local government unit from PAGCOR.'),
        ('4-04-01-020','Share from Philippine Charity Sweepstakes Office (PCSO)','income',false,NULL,false,
         'This account is used to record the share of the local government unit from PCSO.'),
        ('4-04-02-010','Grants and Donations in Cash','income',false,NULL,false,
         'This account is used to record all grants and donations in cash received from private sector or international institutions without specific purpose.'),
        ('4-04-02-020','Grants and Donations in Kind','income',false,NULL,false,
         'This account is used to record all grants and donations in kind received from other levels of government, private sector or international institutions.'),
        ('4-04-02-030','Grants from concessionary Loan','income',false,NULL,false,
         'This account is used to record the difference between the loan proceeds and the fair value of the loan on initial recognition.'),
        ('4-05-01-010','Gain from Changes in Fair Value of Financial Instruments','income',false,NULL,false,
         'This account is used to record the gain in the fair value of financial instruments.'),
        ('4-05-01-020','Gain on Foreign Exchange (FOREX)','income',false,NULL,false,
         'This account is used to record the gain in the revaluation of accounts in foreign currency denomination to local currency at balance sheet date. It also includes actual gain realized in the conversion of foreign currency to local currency.'),
        ('4-05-01-030','Gain on Sale of Investments','income',false,NULL,false,
         'This account is used to record the gain on sale of government investments, such bonds or securities, etc.'),
        ('4-05-01-040','Gain on Sale of Investment Property','income',false,NULL,false,
         'This account is used to record the gain on sale of investment property.'),
        ('4-05-01-050','Gain on Sale of Property, Plant and Equipment','income',false,NULL,false,
         'This account is used to record the gain on sale of government PPEs.'),
        ('4-05-01-060','Gain on Initial Recognition of Biological Assets','income',false,NULL,false,
         'This account is used to record the gain on the initial recognition of biological assets.'),
        ('4-05-01-070','Gain on Sale of Biological Assets','income',false,NULL,false,
         'This account is used to record the gain on the sale of biological assets.'),
        ('4-05-01-080','Gain from Changes in Fair Value Less Cost to Sell of Biological Assets Due to Physical Change','income',false,NULL,false,
         'This account is used to record gains from changes in fair value less cost to sell of biological assets due to physical change.'),
        ('4-05-01-090','Gain from Changes in Fair Value Less Cost to Sell of Biological Assets Due to Price Change','income',false,NULL,false,
         'This account is used to record gains from changes in fair value less cost to sell of biological assets due to price change.'),
        ('4-05-01-100','Gain from Initial Recognition of Agricultural Produce','income',false,NULL,false,
         'This account is used to record gains from changes in fair value less cost to sell of biological assets due to price change.'),
        ('4-05-01-110','Gain on Sale of Intangible Assets','income',false,NULL,false,
         'This account is used to record the gains on sale of intangible assets.'),
        ('4-05-01-120','Reversal of Impairment Losses','income',false,NULL,false,
         'This account is used to record the reversal of impairment losses recognized in the previous year''s.'),
        ('4-05-01-990','Other Gains','income',false,NULL,false,
         'This account is used to record the gains not falling under any of the specific gain accounts.'),
        ('4-06-01-010','Miscellaneous Income','income',false,NULL,false,
         'This account is used to record the receipt revenue and receipts not elsewhere classified under any specific income account.'),
        ('4-07-01-010','Sale of Garnished/Confiscated/Abandoned Goods and Properties','income',false,NULL,false,
         'This account is used to record the receipts derived from the sale of garnished/confiscated/abandoned/seized goods or properties title of which had been decided in favor of the government.'),
        ('5-01-01-010','Salaries and Wages - Regular','expense',false,'PS',false,
         'This account is used to record the pay proper for services rendered of government employees occupying regular plantilla positions.

Subsidiary Ledger: 01 - civilian employees 02 - uniformed personnel'),
        ('5-01-01-020','Salaries and Wages - Casual/Contractual','expense',false,'PS',false,
         'This account is used to record the pay proper for services rendered by employees contracted/hired to perform regular agency functions and specific vital activities or services which cannot be provided by the regular or permanent staff of the agency. These are considered employees of the hiring agency limited to the year when their services are reasonably required. This includes payment for services rendered by substitute and part-time teachers.'),
        ('5-01-02-010','Personnel Economic Relief Allowance (PERA)','expense',false,'PS',false,
         'This account is used to record the allowance granted to government personnel occupying regular, contractual or casual positions; appointive or elective; rendering services on full-time or part-time basis; and whose positions are covered by Republic Act No. 6758, as amended.'),
        ('5-01-02-020','Representation Allowance (RA)','expense',false,'PS',false,
         'This account is used to record the monthly commutable representation allowance granted to authorized officials and employees in the actual performance of their respective functions, such as hosting official meetings.'),
        ('5-01-02-030','Transportation Allowance (TA)','expense',false,'PS',false,
         'This account is used to record the monthly commutable transportation allowance granted to authorized officials and employees in the actual performance of their respective functions.'),
        ('5-01-02-040','Clothing/Uniform Allowance','expense',false,'PS',false,
         'This account is used to record the fixed amount granted to authorized government officials/employees for the upkeep/replacement of clothing/uniform paid in cash or in kind.'),
        ('5-01-02-050','Subsistence Allowance','expense',false,'PS',false,
         'This account is used to record the amount granted to authorized government officials/employees to cover cost of meals, and incidental expenses incurred by employees who are on official business.'),
        ('5-01-02-060','Laundry Allowance','expense',false,'PS',false,
         'This account is used to record the amount granted to authorized government officials/employees to cover cost of laundering uniform incurred by employees who are on official business.'),
        ('5-01-02-070','Quarters Allowance','expense',false,'PS',false,
         'This account is used to record the amount granted to authorized government officials/employees to cover cost of suitable and adequate living quarters incurred by employees who are on official business.'),
        ('5-01-02-080','Productivity Incentive Allowance','expense',false,'PS',false,
         'This account is used to record the fixed amount granted to authorized government officials/employees for at least satisfactory performance in the discharge of their official duties.'),
        ('5-01-02-090','Overseas Allowance','expense',false,'PS',false,
         'This account is used to record the allowances granted to authorized government officials/employees on foreign assignment/mission.'),
        ('5-01-02-100','Honoraria','expense',false,'PS',false,
         'This account is used to record the payment given to professionals for services in recognition of their expertise, broad and superior knowledge in specific fields and special projects. This also include payments to lecturers, resource persons, coordinators and facilitators in seminars, training programs and other similar activities in training institutions including those who render services beyond their regular workload.'),
        ('5-01-02-110','Hazard Pay','expense',false,'PS',false,
         'This account is used to record the payment to government officials/employees assigned in difficult/ dangerous/strife-torn/embattled areas as certified by appropriate government body and to those whose lives are directly exposed to work conditions which may cause injury/sickness/death/harmful change in the human being.'),
        ('5-01-02-120','Longevity Pay','expense',false,'PS',false,
         'This account is used to record the additional compensation to government officials and employees based on years of service rendered to the

government.'),
        ('5-01-02-130','Overtime and Night Pay','expense',false,'PS',false,
         'This account is used to record the extra compensation paid to government employees who are authorized to work for more than the normal 40 hours in one workweek or on days of rest.'),
        ('5-01-02-140','Year End Bonus','expense',false,'PS',false,
         'This account is used to record the amount granted annually to government officials and employees as authorized by law.'),
        ('5-01-02-150','Cash Gift','expense',false,'PS',false,
         'This account is used to record the amount granted annually in addition to the year end bonus to government officials and employees as authorized by law.'),
        ('5-01-02-990','Other Bonuses and Allowances','expense',false,'PS',false,
         'This account is used to record other authorized bonuses and allowances to government officials/employees not falling under any of the specific other compensation accounts.'),
        ('5-01-03-010','Retirement and Life Insurance Contributions','expense',false,'PS',false,
         'This account is used to record the government''s share in premium contributions to the Government Service Insurance System and other life and retirement benefit systems.'),
        ('5-01-03-020','Pag-IBIG Contributions','expense',false,'PS',false,
         'This account is used to record the government''s share in premium contributions to the Home Development Mutual Fund.'),
        ('5-01-03-030','PhilHealth Contributions','expense',false,'PS',false,
         'This account is used to record the government''s share in premium contributions to the Philippine Health Insurance Corporation.'),
        ('5-01-03-040','Employees Compensation Insurance Premiums','expense',false,'PS',false,
         'This account is used to record the government''s share in premium contributions to the Employees'' Compensation Commission (ECC).'),
        ('5-01-03-050','Provident/Welfare Fund Contribution','expense',false,'PS',false,
         'This account is used to record the amount contributed by the government for the employees'' provident or welfare fund.'),
        ('5-01-04-010','Pension Benefits','expense',false,'PS',false,
         'This account is used to record the amount of pension to government employees.

Subsidiary Ledger: 01 - civilian employees 02 - uniformed personnel'),
        ('5-01-04-020','Retirement Gratuity','expense',false,'PS',false,
         'This account is used to record the amount of retirement gratuity to government employees.'),
        ('5-01-04-030','Terminal Leave Benefits','expense',false,'PS',false,
         'This account is used to record the money value of the accumulated leave credits of government officials and employees.'),
        ('5-01-04-990','Other Personnel Benefits','expense',false,'PS',false,
         'This account is used to record the amount contributed by the government for the employees'' benefits not falling under any of the specific other personnel accounts.'),
        ('5-02-01-010','Traveling Expenses - Local','expense',false,'MOOE',false,
         'This account is used to record the costs incurred in the movement/transport of government officers and employees within the country. This includes transportation, travel per diems, ferriage, and all other related expenses.'),
        ('5-02-01-020','Traveling Expenses - Foreign','expense',false,'MOOE',false,
         'This account is used to record the costs incurred in the movement/transport of government officers and employees outside the country. This includes transportation, travel per diems, passport and visa processing and all other related expenses.'),
        ('5-02-02-010','Training Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs incurred for the participation/ attendance in and conduct of trainings, conventions and seminars/workshops. It includes training fees, honoraria of lecturers, cost of handouts, supplies, materials, meals, snacks and all other expenses related to training.'),
        ('5-02-02-020','Scholarship Grants/Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of scholarships granted by the government to individuals in the pursuit of further learning, study or research.'),
        ('5-02-03-010','Office Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost or value of office supplies such as bond paper, ink, and small tangible items like staple wire remover, puncher, stapler and other similar items issued to end-users for government operations.'),
        ('5-02-03-020','Accountable Forms Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of accountable forms with or without money value such as official receipts, passports, tickets, permit/license plates, LTO plates, and the like, issued to end-users.'),
        ('5-02-03-030','Non-Accountable Forms Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of non-accountable forms such as pre- printed application forms, tax returns forms, accounting forms and the like, issued to end-users.'),
        ('5-02-03-040','Animal/Zoological Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of food, medicines, veterinary and other maintenance needs of animals issued for use in government parks, zoos, wildlife sanctuaries and botanical gardens. This also includes supplies issued for zoological researches and preservations.'),
        ('5-02-03-050','Food Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of food issued to hospital/rehabilitation patients, jail inmates and the like.'),
        ('5-02-03-060','Welfare Goods Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of goods issued/distributed to people affected by calamities/disasters/ground conflicts such as canned goods, blankets, mats, kitchen utensils, flashlights and other similar items. This also includes the cost of food served to people affected by calamities/disasters/ground conflicts.'),
        ('5-02-03-070','Drugs and Medicines Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of drugs and medicines issued to end- users for government operations.'),
        ('5-02-03-080','Medical, Dental and Laboratory Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of medical, dental and laboratory supplies issued to end-users for government operations.'),
        ('5-02-03-090','Fuel, Oil and Lubricants Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of fuel, oil and lubricants issued for use of government vehicles and other equipment in connection with government operations/projects.'),
        ('5-02-03-100','Agricultural and Marine Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of fertilizers, pesticides and other marine and agricultural supplies issued in government operations/projects. This includes supplies issued for aquaculture researches, environment protection/preservations and the like.'),
        ('5-02-03-110','Textbooks and Instructional Materials Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of books and instructional materials distributed to public schools including flipcharts, video clips/slides, and the like.'),
        ('5-02-03-120','Military, Police and Traffic Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost or value of military and police supplies issued/used in government operations such as clubs/cudgels, night sticks, police/ traffic gears, flashlights, truncheons, ammunitions and the like.'),
        ('5-02-03-130','Chemical and Filtering Supplies Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of chemical and filtering supplies used in government operations.'),
        ('5-02-03-990','Other Supplies and Materials Expense','expense',false,'MOOE',false,
         'This account is used to record the cost of inventories issued to end-users not otherwise classified under the specific inventory expense accounts.'),
        ('5-02-04-010','Water Expenses','expense',false,'MOOE',false,
         'This account is used to record cost of water consumed in government operations/projects.'),
        ('5-02-04-020','Electricity Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of electricity consumed in government operations/projects.'),
        ('5-02-05-010','Postage and Courier Service','expense',false,'MOOE',false,
         'This account is used to record the cost of delivery/transmission of official messages, mails, documents, records and the like.'),
        ('5-02-05-020','Telephone Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of transmitting messages thru telephone lines (mobile or landlines), faxes, telex and the like whether prepaid or postpaid.'),
        ('5-02-05-030','Internet Subscription Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of using internet services in government operations.'),
        ('5-02-05-040','Cable, Satellite, Telegraph and Radio Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of using cable/satellite/ telegram/radio services.'),
        ('5-02-06-010','Awards and Rewards Expenses','expense',false,'MOOE',false,
         'This account is used to record the amount given in recognition of any civic or professional achievement and of rewards to informers for the receipt of reliable information leading to successful arrest/capture of fugitives, seizure/confiscation of smuggled goods, or collection of unpaid taxes/surcharges/fines/penalties.'),
        ('5-02-06-020','Prizes','expense',false,'MOOE',false,
         'This account is used to record the amount paid to winners of competitive and promotional activities.'),
        ('5-02-07-010','Survey Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost incurred in the conduct of cadastral, structural, topographical, statistical and other type of surveys conducted by government agencies.'),
        ('5-02-07-020','Research, Exploration and Development Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost incurred in the conduct of studies to gain scientific or technical knowledge on future projects including development, refinement or evaluation of policies for use of management.'),
        ('5-02-08-010','Demolition and Relocation Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of demolition of structures and relocation of structures and settlers affected by government projects.'),
        ('5-02-08-020','Desilting and Dredging Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs incurred in removing large accumulation of decomposed litters and other organic debris in and deepening of canals, sewerage, rivers, creeks, and the like.'),
        ('5-02-09-010','Generation, Transmission and Distribution Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of generation, transmission and distribution of water, electricity, information/ communication, power and other related services intended for sale and/or redistribution.'),
        ('5-02-10-010','Confidential Expenses','expense',false,'MOOE',false,
         'This account is used to record the amount paid for expenses related to surveillance activities in civilian government agencies that are intended to support the mandate or operations of the agency.'),
        ('5-02-10-020','Intelligence Expenses','expense',false,'MOOE',false,
         'This account is used to record the amount paid for expenses related to surveillance activities in civilian government agencies that are intended to support the mandate or operations of the agency.'),
        ('5-02-10-030','Extraordinary and Miscellaneous Expenses','expense',false,'MOOE',false,
         'This account is used to record the amount paid for expenses incidental to the performance of official functions, such as: meetings and conferences, public relations, educational, cultural and athletic activities, membership fees in government organizations, etc.'),
        ('5-02-11-010','Legal Services','expense',false,'MOOE',false,
         'This account is used to record the cost incurred for authorized legal services rendered by private lawyers. This includes special counsel allowance granted to government lawyers deputized to represent the government in court as special counsel.'),
        ('5-02-11-020','Auditing Services','expense',false,'MOOE',false,
         'This account is used to record the cost of operating expenses provided by auditees for auditing services rendered by the Commission on Audit.'),
        ('5-02-11-030','Consultancy Services','expense',false,'MOOE',false,
         'This account is used to record the cost of services rendered by consultants contracted to perform particular outputs or services primarily advisory in nature and requiring highly specialized or technical expertise which cannot be provided by the regular staff of the agency.'),
        ('5-02-11-990','Other Professional Services','expense',false,'MOOE',false,
         'This account is used to record the cost of other professional services contracted by the agency not otherwise classified under any of the specific professional services accounts. This includes honoraria paid by local government units (LGUs) to national government personnel (judges, prosecutors, public elementary and high school teachers and other national government personnel) assigned in LGUs. This also includes other operating expenses granted to national government agencies stationed in the LGU.'),
        ('5-02-12-010','Environment/Sanitary Services','expense',false,'MOOE',false,
         'This account is used to record the cost of services contracted/undertaken by administration for the upkeep and sanitation of the public places. This includes the cost of clean and green program, garbage and hospital waste collection and disposal.'),
        ('5-02-12-020','Janitorial Services','expense',false,'MOOE',false,
         'This account is used to record the cost of janitorial services contracted by the government.'),
        ('5-02-12-030','Security Services','expense',false,'MOOE',false,
         'This account is used to record the cost of security services contracted by the government.'),
        ('5-02-12-990','Other General Services','expense',false,'MOOE',false,
         'This account is used to record the cost of other general services contracted by the agency not otherwise classified under any of the specific general services accounts.'),
        ('5-02-13-010','Repairs and Maintenance - Investment Property','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on buildings/warehouses and other structures held for rent/lease or held for capital appreciation or both.'),
        ('5-02-13-020','Repairs and Maintenance - Land Improvements','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on Aquaculture structures and other land improvements constructed/ acquired/ developed for public use.

Subsidiary ledger:

01- Aquaculture Structures 99- Other Land Improvement'),
        ('5-02-13-030','Repairs and Maintenance - Infrastructure Assets','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on road networks; sewer systems; water supply systems; power supply systems; communication networks; seaport systems; airport systems; parks, plazas, monuments; and other infrastructure assets.

Subsidiary ledger: 01 - Road Networks 02-Flood Control System 02 - Sewer Systems 03 - Water Supply systems 04 - Power Supply Systems 05 - Communication Networks 06 - Seaport Systems 07 - Airport Systems 08 - Parks, Plazas, Monuments 99 - Other Infrastructure Assets'),
        ('5-02-13-040','Repairs and Maintenance - Buildings and Other Structures','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on office buildings; school buildings; hospitals and health centers; markets; slaughterhouses; hotels and dormitories; and other structures.

Subsidiary ledgers: 01 – Buildings 02 - School Buildings 03 - Hospitals and Health Centers 04 - Markets 05 – Slaughterhouses 06 - Hotels and Dormitories 99 - Other Structures'),
        ('5-02-13-050','Repairs and Maintenance - Machinery and Equipment','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on office equipment; ICT equipment ; agricultural and forestry equipment; marine and fishery equipment; airport equipment; communication equipment; construction and heavy equipment; firefighting equipment and accessories ; military, police and security equipment; medical equipment; printing equipment; sports equipment; technical and scientific equipment; and other machinery and equipment.

Subsidiary ledger: 01 - Machinery 02 - Office Equipment 03 - ICT Equipment 04 - Agricultural and Forestry Equipment 05 - Marine and Fishery Equipment 06 - Airport Equipment 07 - Communication Equipment 08 - Construction and Heavy Equipment 09 – Disaster Response and Rescue Equipment

10 - Military, Police and Security Equipment 11 - Medical Equipment 12 - Printing Equipment 13 - Sports Equipment 14 - Technical and Scientific Equipment 99 - Other Machinery and Equipment'),
        ('5-02-13-060','Repairs and Maintenance - Transportation Equipment','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on motor vehicles; train; aircraft; watercraft; and other transportation equipment.

Subsidiary ledger: 01 - Motor Vehicles 02 – Train 03 – Aircraft 04 – Watercraft 99 - Other Transportation Equipment'),
        ('5-02-13-070','Repairs and Maintenance - Furniture and Fixtures','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance on furniture and fixtures.

Subsidiary ledger: 01 - Furniture and Fixtures'),
        ('5-02-13-080','Repairs and Maintenance - Leased Assets','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance of buildings; and other leased assets acquired by a lessee under a finance lease.

Subsidiary ledger: 01 – Buildings and Other Structures 02 – Machinery and Equipment 03 - Transportation Equipment 99 - Other Leased Assets'),
        ('5-02-13-090','Repairs and Maintenance - Leased Assets Improvements','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance of improvements on land; buildings; and other leasehold improvements under lease contract/agreement.

Subsidiary ledger: 01 – Land

02 – Buildings 99 - Other Leasehold Improvements'),
        ('5-02-13-990','Repairs and Maintenance - Other Property, Plant and Equipment','expense',false,'MOOE',false,
         'This account is used to record the cost of repairs and maintenance of other property, plant and equipment not falling under any of the specific property, plant and equipment account.

Subsidiary Ledgers:

01 – Work/Zoo Animals 02 – Other Property, Plant and Equipment'),
        ('5-02-14-020','Subsidy to National Government Agencies','expense',false,'MOOE',false,
         'This account is used to record the assistance without specific purpose granted to NGAs.'),
        ('5-02-14-030','Subsidy to Other Local Government Units','expense',false,'MOOE',false,
         'This account is used to record the transfer of funds/assets without specific purpose to LGUs.'),
        ('5-02-14-060','Subsidy to Other Funds','expense',false,'MOOE',false,
         'This account is used to record fund transfers to one fund from another fund maintained by the same LGU as expressly authorized by law.'),
        ('5-02-14-070','Subsidy to General Fund Proper /Special Accounts','expense',false,'MOOE',false,
         'This account is used to record fund transfers to special accounts from the general fund proper or other economic enterprise of the same LGU.'),
        ('5-02-14-080','Subsidy to Local Economic Enterprises','expense',false,'MOOE',false,
         'This account is used to record fund transfers to economic enterprises from the general fund proper or other economic enterprise of the same LGU.'),
        ('5-02-14-990','Subsidy - Others','expense',false,'MOOE',false,
         'This account is used to record the financial assistance to government institutions not falling under any of the specific financial assistance accounts.'),
        ('5-02-15-010','Transfers of Unspent Current Year DRRM Funds to the Trust Funds','expense',false,'MOOE',false,
         'This account is used to record the current year unspent disaster risk reduction and management funds to the Trust Fund.'),
        ('5-02-15-020','Transfers for Project Equity Share/LGU Counterpart','expense',false,'MOOE',false,
         'This account is used to record the transfer of LGU counterpart funding for projects/activities funded from grants/transfers from other government agencies or foreign funding institutions equity component.'),
        ('5-02-16-010','Taxes, Duties and Licenses','expense',false,'MOOE',false,
         'This account is used to record the amount of taxes, duties, licenses and other fees due to regulatory agencies.'),
        ('5-02-16-020','Fidelity Bond Premiums','expense',false,'MOOE',false,
         'This account is used to record the amount of premiums paid by the agency for the fidelity bonds of accountable officers.'),
        ('5-02-16-030','Insurance Expenses','expense',false,'MOOE',false,
         'This account is used to record the amount of premiums paid by the agency for the insurable risks of government property.'),
        ('5-02-99-010','Advertising Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs incurred for advertisement, such as expenses to (a) promote and market products and services; and (b) publish invitations to bid and other authorized government advertisements.'),
        ('5-02-99-020','Printing and Publication Expenses','expense',false,'MOOE',false,
         'This account is used to record the costs of printing and binding of manuscripts/documents, forms, manuals, brochures, pamphlets, and the like.'),
        ('5-02-99-030','Representation Expenses','expense',false,'MOOE',false,
         'This account is used to record the expenses incurred for official meetings/ conferences/entertainments.'),
        ('5-02-99-040','Transportation and Delivery Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of transporting goods/merchandise sold in the course of business operations. This includes the cost of moving agency''s own people and property from one station to another.'),
        ('5-02-99-050','Rent Expenses','expense',false,'MOOE',false,
         'This account is used to record rental of land, buildings, facilities, equipment, vehicles, machineries, and the like.'),
        ('5-02-99-060','Membership Dues and Contributions to Organizations','expense',false,'MOOE',false,
         'This account is used to record membership fees/dues/contributions to recognized/authorized professional organizations.'),
        ('5-02-99-070','Subscription Expenses','expense',false,'MOOE',false,
         'This account is used to record the cost of subscription to library and other reading materials.'),
        ('5-02-99-080','Donations','expense',false,'MOOE',false,
         'This account is used to record the amount of aids/assistance to other levels of government/individuals/institutions.'),
        ('5-02-99-990','Other Maintenance and Operating Expenses','expense',false,'MOOE',false,
         'This account is used to record expenses not falling under any of the specific maintenance and other operating expense accounts.'),
        ('5-03-01-010','Management Supervision/Trusteeship Fees','expense',false,'FE',false,
         'This account is used to record the amount paid to an appointed person or institution that manages assets, including mutual funds and unit trusts, for the benefit of the government; or an agent who handles the administrative aspects of bond issuance and ensures that the borrower complies with the terms of the bond indenture'),
        ('5-03-01-020','Interest Expenses','expense',false,'FE',false,
         'This account is used to record interest charges paid for the use of borrowed money. This also includes discounts on treasury bills and treasury bonds; premiums on bond investments; coupon payments/interests on domestic and foreign bonds issued by the NG; and interest payment on NG-direct and relent loans, and NG-assumed liabilities.'),
        ('5-03-01-030','Guarantee Fees','expense',false,'FE',false,
         'This account is used to record guarantee fee paid by a debtor government agency to the guarantor for the assurance to pay the agency''s obligation to the creditor as stipulated in the guarantee clause.'),
        ('5-03-01-040','Bank Charges','expense',false,'FE',false,
         'This account is used to record the charges imposed by the bank for various services rendered excluding interest charges. This also includes cost of checkbooks, penalties and surcharges on overdrafts.'),
        ('5-03-01-050','Commitment Fees','expense',false,'FE',false,
         'This account is used to record the commitment charges imposed by creditors based on an agreed percentage of the undrawn loan amount.'),
        ('5-03-01-990','Other Financial Charges','expense',false,'FE',false,
         'This account is used to record the charges not falling under any of the specific financial expense accounts.'),
        ('5-04-01-010','Direct Materials','expense',false,'MOOE',false,
         'This account is used to record the cost of raw materials used as inputs to production or manufacturing. These are specifically and consistently assigned to or associated with the manufacture of a product or a particular work order.'),
        ('5-04-01-020','Direct Labor','expense',false,'MOOE',false,
         'This account is used to record the cost of payroll or part of wage-bill that can be specifically and consistently assigned to or associated with the manufacture of a product or a particular work order.'),
        ('5-04-01-030','Manufacturing Overhead','expense',false,'MOOE',false,
         'This account is used to record the cost incurred in the manufacture of a product other than the costs of direct materials and direct labor. Examples of manufacturing overhead include the depreciation or the rent on the factory building, depreciation on the factory equipment, supervisors in the factory, the factory quality control department, factory maintenance employees, electricity and gas for the factory, indirect factory supplies, and the like.'),
        ('5-04-02-010','Cost of Sales','expense',false,'MOOE',false,
         'This account is used to record the cost of merchandise purchased and resold in the course of business operations.'),
        ('5-05-01-010','Depreciation - Investment Property','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of buildings/warehouses and other structures held for rent/lease or held for capital appreciation or both in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger: 01 – Building 02 - Other Structures'),
        ('5-05-01-020','Depreciation - Land Improvements','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of aquaculture structures and other land improvements constructed/ acquired/ developed for public use in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger:

01- Aquaculture Structures 02- Other Land Improvement'),
        ('5-05-01-030','Depreciation - Infrastructure Assets','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of road networks; sewer systems; water supply systems; power supply systems; communication networks; seaport systems; airport systems; parks, plazas, monuments; and other infrastructure assets in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger: 01 - Road Networks 02 - Sewer Systems 03 - Water Supply systems 04 - Power Supply Systems 05 - Communication Networks 06 - Seaport Systems 07 - Airport Systems 08 - Parks, Plazas, Monuments 99 - Other Infrastructure Assets'),
        ('5-05-01-040','Depreciation - Buildings and Structures','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of office buildings; school buildings; hospitals and health centers; markets; slaughterhouses; hotels and dormitories; and other structures in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledgers: 01 – Buildings 02 - School Buildings 03 - Hospitals and Health Centers 04 - Markets 05 – Slaughterhouses 06 - Hotels and Dormitories 99 - Other Structures'),
        ('5-05-01-050','Depreciation - Machinery and Equipment','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of office equipment; ICT equipment ; agricultural and forestry equipment; marine and fishery equipment; airport equipment; communication equipment; construction and heavy equipment; firefighting equipment and accessories ; military, police and security equipment; medical equipment; printing equipment; sports equipment; technical and scientific equipment; and other machinery and equipment.

Subsidiary ledger: 01 - Office Equipment 02 - ICT Equipment 03 - Agricultural and Forestry Equipment 04 - Marine and Fishery Equipment 05 - Airport Equipment 06 - Communication Equipment 07 - Construction and Heavy Equipment 08 - Firefighting Equipment and Accessories 09 - Military, Police and Security Equipment 10 - Medical Equipment 11 - Printing Equipment 12 - Sports Equipment 13 - Technical and Scientific Equipment 99 - Other Machinery and Equipment'),
        ('5-05-01-060','Depreciation - Transportation Equipment','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of motor vehicles; train; aircraft; watercraft; and other transportation equipment in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger: 01 - Motor Vehicles 02 – Train 03 – Aircraft 04 – Watercraft

99 - Other Transportation Equipment'),
        ('5-05-01-070','Depreciation - Furniture, Fixtures and Books','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of furniture and fixtures; and books in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger: 01 - Furniture and Fixtures 02 - Books'),
        ('5-05-01-080','Depreciation - Leased Assets','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of buildings, and other leased assets under finance lease contract/agreement in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger: 01 – Buildings and Structures 02 – Machinery and equipment 03 – Transportation Equipment 99 - Other Leasehold Improvements'),
        ('5-05-01-090','Depreciation - Leased Asset Improvements','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of improvements on lands, buildings, and other leasehold improvements under lease contract/agreement in accordance with the prescribed policy on depreciation by COA.

Subsidiary ledger: 01 – Land 02 – Buildings 99 - Other Leasehold Improvements'),
        ('5-05-01-100','Depreciation - Service Concession Assets','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of other assets under service concession arrangement in accordance with the prescribed policy on depreciation by COA.'),
        ('5-05-01-990','Depreciation - Other Property, Plant and Equipment','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost for the wear and tear of other property, plant and equipment not falling under any of the specific property, plant and equipment account in accordance with the prescribed policy on depreciation by COA.'),
        ('5-05-02-010','Amortization - Intangible Assets','expense',false,'MOOE',false,
         'This account is used to record the periodic allocation of cost of intangible assets.

Subsidiary ledger: 01 – Patents/Copyrights 02 - Computer Software 04 – Other Intangible Assets'),
        ('5-05-03-010','Impairment Loss - Financial Assets Held to Maturity','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of financial assets held to maturity.'),
        ('5-05-03-020','Impairment Loss - Loans and Receivables','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of loans and receivables and other financial assets where the amortized cost is greater than the value of estimated future cash flows discounted at original effective interest rate.'),
        ('5-05-03-030','Impairment Loss - Lease Receivables','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of operating and finance lease receivables.'),
        ('5-05-03-040','Impairment Loss - Investments in GOCCs','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of investments in GOCCs.'),
        ('5-05-03-050','Impairment Loss - Investments in Joint Venture','expense',false,'MOOE',false,
         'This account is used to record the loss incurred in writing down investments in joint venture below the carrying value.'),
        ('5-05-03-060','Impairment Loss - Other Receivables','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of other receivables.'),
        ('5-05-03-070','Impairment Loss - Inventories','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of inventories for sale.'),
        ('5-05-03-080','Impairment Loss - Investment Property','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of investment property where the carrying value is greater than the depreciated replacement cost.'),
        ('5-05-03-090','Impairment Loss - Property, Plant and Equipment','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of PPE where the carrying value is greater than the depreciated replacement cost.'),
        ('5-05-03-100','Impairment Loss - Intangible Assets','expense',false,'MOOE',false,
         'This account is used to record the loss incurred due to impairment of intangible assets where the carrying value is greater than the service potential cost.'),
        ('5-05-04-010','Loss on Foreign Exchange (FOREX)','expense',false,'MOOE',false,
         'This account is used to record the losses arising from revaluation of assets and liabilities denominated in foreign currencies.'),
        ('5-05-04-020','Loss on Sale of Investments','expense',false,'MOOE',false,
         'This account is used to record the amount of loss on sale of government investments, such as bonds or securities.'),
        ('5-05-04-030','Loss on Sale of Investment Property','expense',false,'MOOE',false,
         'This account is used to record the amount of loss on sale of investment property.'),
        ('5-05-04-040','Loss on Sale of Property, Plant and Equipment','expense',false,'MOOE',false,
         'This account is used to record the amount of loss on sale of PPE.'),
        ('5-05-04-050','Loss on Sale of Biological Assets','expense',false,'MOOE',false,
         'This account is used to record the amount of loss on sale of biological assets.'),
        ('5-05-04-060','Loss on Sale of Intangible Assets','expense',false,'MOOE',false,
         'This account is used to record the amount of loss on sale of intangible assets.'),
        ('5-05-04-070','Loss on Sale of Assets','expense',false,'MOOE',false,
         'This account is used to record amount of loss suffered by government due theft/fortuitous events/calamities/civil unrest and events of same nature.'),
        ('5-05-04-080','Loss on Initial Recognition of Biological Assets','expense',false,'MOOE',false,
         'This account is used to record the loss when the purchase price of biological assets is higher than the fair value less cost to sell .'),
        ('5-05-04-090','Loss on Assets','expense',false,'MOOE',false,
         'This account is used to record the loss of assets upon the grant of relief from accountability of funds and property.'),
        ('5-05-04-100','Loss on Guaranty','expense',false,'MOOE',false,
         'This account is used to record the losses on debts guaranteed by government agencies.'),
        ('5-05-04-990','Other Losses','expense',false,'MOOE',false,
         'This account is used to record the amount of losses not falling under any of the specific loss accounts.'),
        ('5-05-05-010','Grants for Concessionary Loans','expense',false,'MOOE',false,
         'This account is used to record the difference between the loan granted and the fair value of the loan on initial recognition.')
      ) AS t(code, name, kind, contra, class, cash, description)
$fn$;

COMMENT ON FUNCTION official_chart_of_accounts() IS
    'Annex B, Local Government Chart of Accounts: 571 accounts with their descriptions.';


-- ── Seeding ──────────────────────────────────────────────────────────────────
-- Adds what is missing and fills in descriptions nobody has written yet. Never
-- renames or reflags an account that exists, so an office's own edits survive
-- every restart.
CREATE OR REPLACE FUNCTION seed_official_chart_of_accounts(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $fn$
DECLARE
    v_inserted integer;
BEGIN
    PERFORM set_config('app.tenant_id', p_tenant_id::text, true);

    INSERT INTO account (tenant_id, code, name, account_type, is_postable,
                         is_contra, requires_subsidiary, expense_class, is_cash,
                         description)
    SELECT p_tenant_id, o.code, o.name, o.account_type, true,
           o.is_contra, false, o.expense_class, o.is_cash, o.description
      FROM official_chart_of_accounts() o
    ON CONFLICT (tenant_id, code) DO NOTHING;

    GET DIAGNOSTICS v_inserted = ROW_COUNT;

    UPDATE account a
       SET description = o.description
      FROM official_chart_of_accounts() o
     WHERE a.tenant_id = p_tenant_id
       AND a.code = o.code
       AND a.description IS NULL;

    INSERT INTO account (tenant_id, code, name, account_type, is_postable)
         VALUES (p_tenant_id,'1-00-00-000','Assets','asset',false),
                (p_tenant_id,'2-00-00-000','Liabilities','liability',false),
                (p_tenant_id,'3-00-00-000','Net Assets/Equity','equity',false),
                (p_tenant_id,'4-00-00-000','Income','income',false),
                (p_tenant_id,'5-00-00-000','Expenses','expense',false)
    ON CONFLICT (tenant_id, code) DO NOTHING;

    UPDATE account child
       SET parent_id = parent.id
      FROM account parent
     WHERE child.tenant_id  = p_tenant_id
       AND parent.tenant_id = p_tenant_id
       AND parent.code      = left(child.code, 1) || '-00-00-000'
       AND child.code      <> parent.code
       AND child.parent_id IS NULL;

    RETURN v_inserted;
END
$fn$;

COMMENT ON FUNCTION seed_official_chart_of_accounts(uuid) IS
    'Adds any account in Annex B the municipality does not have yet. Runs at '
    'every start; never changes an existing account.';


-- ── Normal balance follows the annex ─────────────────────────────────────────
ALTER TABLE account ALTER COLUMN normal_balance SET EXPRESSION AS (
    CASE WHEN (account_type IN ('asset','expense')) <> is_contra
         THEN 'debit' ELSE 'credit' END
);

COMMENT ON COLUMN account.normal_balance IS
    'The side the account normally sits on. The type''s side, flipped for a '
    'contra account -- accumulated depreciation is an asset with a credit balance.';


-- ── Existing municipalities, once ────────────────────────────────────────────
-- Official titles and flags for the accounts already there, then the rest.
DO $do$
DECLARE
    t record;
BEGIN
    FOR t IN SELECT id FROM tenant LOOP
        PERFORM set_config('app.tenant_id', t.id::text, true);

        -- month-end snapshots were signed to the old normal side; an account
        -- whose side flips has its snapshots flipped with it
        UPDATE account_period_balance b
           SET ending_balance = -b.ending_balance
          FROM account a
          LEFT JOIN official_chart_of_accounts() o ON o.code = a.code
         WHERE b.account_id = a.id
           AND a.tenant_id  = t.id
           AND coalesce(o.is_contra, a.is_contra);

        UPDATE account a
           SET name        = o.name,
               is_contra   = o.is_contra,
               description = o.description
          FROM official_chart_of_accounts() o
         WHERE a.tenant_id = t.id
           AND a.code = o.code
           AND (a.name, a.is_contra, a.description)
               IS DISTINCT FROM (o.name, o.is_contra, o.description);

        PERFORM seed_official_chart_of_accounts(t.id);
    END LOOP;
END
$do$;


-- ── Statement of financial position ──────────────────────────────────────────
-- Contra balances are now positive on their own side, so every group nets them
-- out -- equity and the surplus line included.
CREATE OR REPLACE FUNCTION statement_financial_position(
    p_tenant_id uuid,
    p_as_of     date,
    p_fund_id   uuid DEFAULT NULL
)
RETURNS TABLE (
    section      text,
    account_code text,
    account_name text,
    amount       numeric(18,2),
    is_total     boolean
)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    v_assets      numeric(18,2);
    v_liabilities numeric(18,2);
    v_equity      numeric(18,2);
    v_surplus     numeric(18,2);
BEGIN
    -- Detail lines. Contra accounts are shown as negatives within their group,
    -- which is how accumulated depreciation appears on a real balance sheet.
    RETURN QUERY
    SELECT CASE tb.account_type
               WHEN 'asset'     THEN 'Assets'
               WHEN 'liability' THEN 'Liabilities'
               ELSE 'Net assets and equity'
           END,
           tb.account_code, tb.account_name,
           CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END,
           false
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type IN ('asset', 'liability', 'equity')
     ORDER BY tb.account_code;

    SELECT coalesce(sum(CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END), 0)
      INTO v_assets
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type = 'asset';

    SELECT coalesce(sum(CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END), 0)
      INTO v_liabilities
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type = 'liability';

    SELECT coalesce(sum(CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END), 0)
      INTO v_equity
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type = 'equity';

    -- Income less expenses to date: the result not yet closed to equity. A
    -- contra income account (a tax discount) reduces income.
    SELECT coalesce(sum(
               CASE WHEN tb.account_type = 'income' THEN 1 ELSE -1 END
             * CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END
           ), 0)
      INTO v_surplus
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type IN ('income', 'expense');

    RETURN QUERY VALUES
        ('Assets',                'TOTAL', 'Total assets',                      v_assets,      true),
        ('Liabilities',           'TOTAL', 'Total liabilities',                 v_liabilities, true),
        ('Net assets and equity', 'RESULT','Surplus (deficit) for the period',  v_surplus,     false),
        ('Net assets and equity', 'TOTAL', 'Total net assets and equity',       v_equity + v_surplus, true),
        ('Check',                 'CHECK', 'Assets less liabilities and equity',
             v_assets - v_liabilities - (v_equity + v_surplus), true);
END
$$;


-- ── Semi-expendable property ─────────────────────────────────────────────────
-- Added by COA after the annex, so not in it. The trial balance spelled them
-- "Semi-Expandable"; these are COA's titles.
DO $do$
DECLARE
    t record;
BEGIN
    FOR t IN SELECT id FROM tenant LOOP
        PERFORM set_config('app.tenant_id', t.id::text, true);

        UPDATE account a
           SET name = v.name
          FROM (VALUES
            ('1-04-05-020','Semi-Expendable Office Equipment'),
            ('1-04-05-030','Semi-Expendable Information and Communications Technology Equipment'),
            ('1-04-05-080','Semi-Expendable Disaster Response and Rescue Equipment'),
            ('1-04-05-090','Semi-Expendable Military, Police and Security Equipment'),
            ('1-04-05-990','Semi-Expendable Other Machinery and Equipment'),
            ('1-04-06-010','Semi-Expendable Furniture and Fixtures')
          ) AS v(code, name)
         WHERE a.tenant_id = t.id
           AND a.code = v.code
           AND a.name LIKE 'Semi-Expandable%';
    END LOOP;
END
$do$;
