-- ═══════════════════════════════════════════════════════════════════════════
--  0004 — Reference data: funds, chart of accounts, periods, numbering
-- ═══════════════════════════════════════════════════════════════════════════
--  Baseline: GAM for LGUs (COA Circular 2015-009) and the Revised Chart of
--  Accounts (COA Circular 2013-002, 8-digit). NGAS / COA Circular 2002-002 is
--  superseded -- see docs/coa-lgu-accounting-reference.md.
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Funds ────────────────────────────────────────────────────────────────────
-- LGU accounting is fund-based: every transaction, budget line, and statement
-- belongs to a fund. Books and financial statements are kept per fund and then
-- consolidated.
CREATE TYPE fund_kind AS ENUM ('general', 'special_education', 'trust');

CREATE TABLE fund (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    code            text        NOT NULL,          -- '01', '02', '03', '01-20DF'
    name            text        NOT NULL,
    kind            fund_kind   NOT NULL,

    -- Statutory sub-funds carved out of the General Fund: the 20% Development
    -- Fund, LDRRMF (5% calamity), and GAD. They report separately but roll up
    -- into their parent fund's statements.
    parent_fund_id  uuid        REFERENCES fund(id) ON DELETE RESTRICT,
    is_statutory_subfund boolean NOT NULL DEFAULT false,

    is_active       boolean     NOT NULL DEFAULT true,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, code),
    -- A sub-fund must name a parent; a top-level fund must not.
    CHECK ( (is_statutory_subfund AND parent_fund_id IS NOT NULL)
         OR (NOT is_statutory_subfund AND parent_fund_id IS NULL) )
);


-- ── Chart of accounts ────────────────────────────────────────────────────────
CREATE TYPE account_type AS ENUM ('asset', 'liability', 'equity', 'income', 'expense');

-- Allotments and obligations are tracked per expense class, and the RAOD
-- registry is one per class.
CREATE TYPE expense_class AS ENUM ('PS', 'MOOE', 'FE', 'CO');

CREATE TABLE account (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    -- 8-digit RCA code, stored with separators as staff read it: '1-07-03-010'.
    code            text        NOT NULL,
    name            text        NOT NULL,
    account_type    account_type NOT NULL,

    -- Derived, never hand-entered: assets and expenses increase on the debit
    -- side, everything else on the credit side.
    normal_balance  text        GENERATED ALWAYS AS (
                                    CASE WHEN account_type IN ('asset','expense')
                                         THEN 'debit' ELSE 'credit' END
                                ) STORED,

    parent_id       uuid        REFERENCES account(id) ON DELETE RESTRICT,

    -- Only leaf accounts may be posted to. Summary accounts exist to roll up.
    is_postable     boolean     NOT NULL DEFAULT true,
    -- Contra accounts (accumulated depreciation, allowances) sit on the
    -- opposite side of their parent group.
    is_contra       boolean     NOT NULL DEFAULT false,

    -- Set when this account requires subsidiary-ledger detail (by payee,
    -- by office, by project).
    requires_subsidiary boolean NOT NULL DEFAULT false,
    expense_class   expense_class,

    -- Cash-in-bank accounts link to a bank account for reconciliation.
    is_cash         boolean     NOT NULL DEFAULT false,

    is_active       boolean     NOT NULL DEFAULT true,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, code),
    -- An expense class only means something on an expense account.
    CHECK (expense_class IS NULL OR account_type = 'expense')
);

CREATE INDEX account_lookup ON account (tenant_id, code) WHERE is_active;
CREATE INDEX account_parent ON account (parent_id);

COMMENT ON COLUMN account.is_postable IS
    'Journal lines may only reference postable accounts. Enforced by trigger '
    'in 0005 -- posting to a summary account silently corrupts every rollup.';


-- ── Organisational units and payees ──────────────────────────────────────────
CREATE TABLE office (
    id          uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    code        text        NOT NULL,
    name        text        NOT NULL,
    head_name   text,
    is_active   boolean     NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, code)
);

CREATE TABLE payee (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    name            text        NOT NULL,
    payee_type      text        NOT NULL DEFAULT 'supplier'
                                CHECK (payee_type IN ('supplier','employee','agency','individual','lgu')),
    tin             text,
    address         text,
    contact_number  text,

    -- Default withholding treatment; the DV may still override per line.
    is_vat_registered boolean   NOT NULL DEFAULT false,
    default_ewt_code  text,

    is_active       boolean     NOT NULL DEFAULT true,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

-- Expression keys need an index rather than a table constraint. Two payees may
-- share a name only if their TINs differ.
CREATE UNIQUE INDEX payee_identity
    ON payee (tenant_id, name, coalesce(tin, ''));

-- Trigram index backs the payee-similarity tier of the reconciliation matcher.
CREATE INDEX payee_name_trgm ON payee USING gin (name gin_trgm_ops);


-- ── Accounting periods ───────────────────────────────────────────────────────
-- Posting into a closed period is refused by the database. Closing writes
-- balance snapshots so the trial balance never re-aggregates all of history.
CREATE TABLE posting_period (
    id          uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    fund_id     uuid        NOT NULL REFERENCES fund(id) ON DELETE RESTRICT,

    fiscal_year smallint    NOT NULL CHECK (fiscal_year BETWEEN 2000 AND 2200),
    month       smallint    NOT NULL CHECK (month BETWEEN 1 AND 12),
    starts_on   date        NOT NULL,
    ends_on     date        NOT NULL,

    status      text        NOT NULL DEFAULT 'open'
                            CHECK (status IN ('open','closed','locked')),
    closed_at   timestamptz,
    closed_by   uuid        REFERENCES app_user(id) ON DELETE SET NULL,

    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, fiscal_year, month),
    CHECK (ends_on >= starts_on),
    CHECK (status = 'open' OR closed_at IS NOT NULL)
);

CREATE INDEX posting_period_lookup
    ON posting_period (tenant_id, fund_id, starts_on, ends_on);

COMMENT ON COLUMN posting_period.status IS
    'open = accepts postings. closed = month-end done, reopenable by an '
    'administrator. locked = year-end audited and submitted, never reopenable.';


-- ── Document numbering ───────────────────────────────────────────────────────
-- Deliberately NOT a PostgreSQL sequence. Sequences leak numbers on rollback,
-- and a gap in a voucher series is an audit finding the Accountant has to
-- explain. A counter row taken with FOR UPDATE inside the same transaction
-- gives a strictly gapless series.
CREATE TABLE document_series (
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    doc_type    text        NOT NULL CHECK (doc_type IN
                            ('ORS','DV','JEV','OR','CHK','ADA','RCD','BRS')),
    fund_id     uuid        REFERENCES fund(id) ON DELETE RESTRICT,
    fiscal_year smallint    NOT NULL,
    month       smallint    CHECK (month BETWEEN 1 AND 12),

    prefix      text        NOT NULL DEFAULT '',
    next_no     bigint      NOT NULL DEFAULT 1 CHECK (next_no >= 1),
    padding     smallint    NOT NULL DEFAULT 5
);

-- A series is identified per tenant, type, year, and optionally month and
-- fund. NULL month/fund mean "not segmented by that dimension", so they are
-- coalesced to sentinels -- NULLs would otherwise never compare equal and the
-- counter would silently fork.
CREATE UNIQUE INDEX document_series_key ON document_series (
    tenant_id,
    doc_type,
    fiscal_year,
    coalesce(month, 0),
    coalesce(fund_id, '00000000-0000-0000-0000-000000000000'::uuid)
);

CREATE OR REPLACE FUNCTION next_document_number(
    p_tenant_id   uuid,
    p_doc_type    text,
    p_fiscal_year smallint,
    p_month       smallint DEFAULT NULL,
    p_fund_id     uuid     DEFAULT NULL
)
RETURNS text
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_no      bigint;
    v_prefix  text;
    v_padding smallint;
BEGIN
    INSERT INTO document_series (tenant_id, doc_type, fund_id, fiscal_year, month)
         VALUES (p_tenant_id, p_doc_type, p_fund_id, p_fiscal_year, p_month)
    ON CONFLICT DO NOTHING;

    -- FOR UPDATE via the returning UPDATE: concurrent callers queue here, so
    -- two clerks can never be handed the same voucher number.
    UPDATE document_series
       SET next_no = next_no + 1
     WHERE tenant_id   = p_tenant_id
       AND doc_type    = p_doc_type
       AND fiscal_year = p_fiscal_year
       AND coalesce(month, 0)   = coalesce(p_month, 0)
       AND coalesce(fund_id, '00000000-0000-0000-0000-000000000000'::uuid)
         = coalesce(p_fund_id, '00000000-0000-0000-0000-000000000000'::uuid)
    RETURNING next_no - 1, prefix, padding
         INTO v_no, v_prefix, v_padding;

    IF v_no IS NULL THEN
        RAISE EXCEPTION 'no document series for % in fiscal year %', p_doc_type, p_fiscal_year
            USING ERRCODE = 'no_data_found';
    END IF;

    RETURN concat_ws('-',
        nullif(v_prefix, ''),
        p_doc_type,
        p_fiscal_year::text,
        CASE WHEN p_month IS NOT NULL THEN lpad(p_month::text, 2, '0') END,
        lpad(v_no::text, v_padding, '0')
    );
END
$$;

COMMENT ON FUNCTION next_document_number IS
    'Gapless voucher numbering, e.g. DV-2026-08-00117. Must be called inside '
    'the same transaction that inserts the document.';


-- ── Composite keys for tenant-safe foreign keys ──────────────────────────────
-- Foreign key checks run as the system and are not subject to row level
-- security. A plain "REFERENCES account(id)" would therefore happily accept
-- another municipality's account id. Referencing tables use composite keys
-- (tenant_id, id) instead, which makes cross-tenant references impossible at
-- the storage layer rather than merely unlikely.
ALTER TABLE fund           ADD CONSTRAINT fund_tenant_key           UNIQUE (tenant_id, id);
ALTER TABLE account        ADD CONSTRAINT account_tenant_key        UNIQUE (tenant_id, id);
ALTER TABLE office         ADD CONSTRAINT office_tenant_key         UNIQUE (tenant_id, id);
ALTER TABLE payee          ADD CONSTRAINT payee_tenant_key          UNIQUE (tenant_id, id);
ALTER TABLE posting_period ADD CONSTRAINT posting_period_tenant_key UNIQUE (tenant_id, id);


CREATE TRIGGER fund_touch           BEFORE UPDATE ON fund           FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER account_touch        BEFORE UPDATE ON account        FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER office_touch         BEFORE UPDATE ON office         FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER payee_touch          BEFORE UPDATE ON payee          FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER posting_period_touch BEFORE UPDATE ON posting_period FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('fund');
SELECT apply_tenant_guards('account');
SELECT apply_tenant_guards('office');
SELECT apply_tenant_guards('payee');
SELECT apply_tenant_guards('posting_period');
SELECT apply_tenant_guards('document_series');
