-- ════════════════════════════════════════════════════════════════════════════
-- 0029 — Naming the right authority
--
-- The system was built citing two issuances, and both were wrong for a
-- municipality. Recorded here rather than by editing the earlier migrations,
-- whose checksums the migration runner verifies on every start.
--
-- ── What was cited ──────────────────────────────────────────────────────────
--
--   "GAM for LGUs (COA Circular 2015-009)"
--   "Revised Chart of Accounts (COA Circular 2013-002, 8-digit)"
--
-- ── What is correct ─────────────────────────────────────────────────────────
--
--   * COA Circular No. 2020-004, 31 January 2020 — prescribes the Government
--     Accounting Manual (GAM) for Local Government Units, in three volumes:
--         Volume I   — The Accounting Policies
--         Volume II  — The Accounting Books, Registries, Records, Forms and
--                      Reports
--         Volume III — The Chart of Accounts
--     It supersedes the Manual on the New Government Accounting System (NGAS)
--     for LGUs (COA Circular No. 2002-003, 20 June 2002) and COA Circular
--     No. 2015-009.
--
--   * COA Circular No. 2015-009, 1 December 2015 — prescribed the Revised
--     Chart of Accounts for LGUs. Superseded by 2020-004; the chart now lives
--     in GAM for LGUs Volume III.
--
--   * COA Circular No. 2013-002 — the Revised Chart of Accounts for **National
--     Government Agencies**. It was never the LGU chart, and citing it in a
--     municipal system was a category error, not a stale reference.
--
-- ── Why this matters beyond tidiness ────────────────────────────────────────
--
-- These citations are what the office would be asked to stand behind if the
-- system's outputs were questioned during an audit. A form defended by the
-- wrong circular number invites exactly the finding it was meant to prevent.
-- The account codes themselves were taken from the office's own trial balance
-- (migration 0018), so the data was never wrong — only its stated authority.
-- ════════════════════════════════════════════════════════════════════════════

COMMENT ON TABLE account IS
    'Chart of accounts. Per the GAM for LGUs Volume III (COA Circular '
    'No. 2020-004, 31 January 2020). The codes loaded for this municipality '
    'were taken from the Accounting Office''s own trial balance.';

COMMENT ON TABLE fund IS
    'The funds a municipality keeps. General, Special Education, and Trust are '
    'statutory under RA 7160 (Local Government Code); the special accounts '
    'within the General Fund follow the GAM for LGUs (COA Circular 2020-004).';

COMMENT ON TABLE journal_entry IS
    'Journal entry vouchers and their books of original entry, per the GAM for '
    'LGUs Volume II (COA Circular No. 2020-004): General Journal, Cash '
    'Receipts Journal, Check Disbursements Journal, and Cash Disbursements '
    'Journal.';

COMMENT ON TABLE obligation IS
    'Obligation Request and Status (ORS). Certification boxes follow the GAM '
    'for LGUs Volume II and the budget accountability requirements of RA 7160.';

COMMENT ON TABLE disbursement_voucher IS
    'Disbursement Voucher, per the GAM for LGUs Volume II (COA Circular '
    'No. 2020-004). Certification and approval boxes carry the separation of '
    'duties the form itself prescribes.';


-- A record the system can show, rather than a claim in a comment nobody reads.
CREATE TABLE IF NOT EXISTS regulatory_basis (
    id          smallint    PRIMARY KEY,
    citation    text        NOT NULL,
    issued_on   date,
    subject     text        NOT NULL,
    applies_to  text        NOT NULL,
    note        text
);

COMMENT ON TABLE regulatory_basis IS
    'What this system is built to comply with, in a form the office can print '
    'and hand to an auditor who asks.';

INSERT INTO regulatory_basis (id, citation, issued_on, subject, applies_to, note) VALUES
    (1, 'COA Circular No. 2020-004', '2020-01-31',
        'Government Accounting Manual (GAM) for Local Government Units, Volumes I to III',
        'Accounting policies, books, registries, forms, reports, and the chart of accounts',
        'The governing manual. Supersedes the NGAS for LGUs (COA Circular No. 2002-003) '
        'and COA Circular No. 2015-009.'),

    (2, 'COA Circular No. 2015-009', '2015-12-01',
        'Revised Chart of Accounts for Local Government Units',
        'Account codes and titles',
        'Superseded by COA Circular No. 2020-004; the chart is now GAM for LGUs Volume III.'),

    (3, 'Republic Act No. 7160', '1991-10-10',
        'Local Government Code of 1991',
        'Funds, the local budget process, appropriations, and the offices of the LGU',
        'Book II Title Five governs local fiscal administration.'),

    (4, 'Presidential Decree No. 1445', '1978-06-11',
        'Government Auditing Code of the Philippines',
        'Accountability for public funds, and the keeping of accounts',
        NULL),

    (5, '1987 Constitution, Article IX-D', '1987-02-02',
        'The Commission on Audit',
        'COA''s authority to prescribe accounting and auditing rules',
        NULL),

    (6, 'Philippine Public Sector Accounting Standards (PPSAS)', NULL,
        'Accrual-basis public sector accounting standards, harmonised with IPSAS',
        'Recognition, measurement, and the general purpose financial statements',
        'Adopted for LGUs through the GAM.')
ON CONFLICT (id) DO UPDATE
   SET citation   = excluded.citation,
       issued_on  = excluded.issued_on,
       subject    = excluded.subject,
       applies_to = excluded.applies_to,
       note       = excluded.note;
