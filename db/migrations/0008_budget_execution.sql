-- ═══════════════════════════════════════════════════════════════════════════
--  0008 — Budget execution: appropriations → allotments → obligations
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.1: "The system shall record appropriations, allotments, and obligations,
--         and shall prevent obligations that exceed the available allotment."
--
--  RA 7160 §305: no money is paid out of the treasury except in pursuance of
--  an appropriation ordinance. That principle is the whole chain below, and
--  each link is a ceiling on the next:
--
--      Appropriation  (the Sanggunian authorises the amount)
--            ↓  cannot exceed
--      Allotment      (the Budget Officer releases part of it)
--            ↓  cannot exceed
--      Obligation     (a commitment is made against the release)
--            ↓  cannot exceed
--      Disbursement   (money actually leaves)
--
--  Both ceilings are enforced by trigger with row locks, not by application
--  code. Two clerks obligating at the same instant must not both slip under a
--  ceiling that only one of them fits within.
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Appropriations ───────────────────────────────────────────────────────────
CREATE TABLE appropriation (
    id              uuid            PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid            NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    fund_id         uuid            NOT NULL,
    fiscal_year     smallint        NOT NULL CHECK (fiscal_year BETWEEN 2000 AND 2200),

    -- The authorising instrument. Every peso must trace back to one.
    ordinance_no    text            NOT NULL,
    ordinance_date  date            NOT NULL,
    appropriation_type text         NOT NULL DEFAULT 'annual'
                                    CHECK (appropriation_type IN ('annual','supplemental','continuing')),

    office_id       uuid            NOT NULL,
    account_id      uuid            NOT NULL,
    expense_class   expense_class   NOT NULL,

    amount          numeric(18,4)   NOT NULL CHECK (amount > 0),
    particulars     text,

    created_by      uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    created_at      timestamptz     NOT NULL DEFAULT now(),
    updated_at      timestamptz     NOT NULL DEFAULT now(),

    FOREIGN KEY (tenant_id, fund_id)    REFERENCES fund    (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, office_id)  REFERENCES office  (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, account_id) REFERENCES account (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX appropriation_lookup
    ON appropriation (tenant_id, fund_id, fiscal_year, expense_class);

ALTER TABLE appropriation ADD CONSTRAINT appropriation_tenant_key UNIQUE (tenant_id, id);


-- ── Allotments ───────────────────────────────────────────────────────────────
CREATE TABLE allotment (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    appropriation_id  uuid          NOT NULL,
    allotment_no      text          NOT NULL,
    release_date      date          NOT NULL,
    amount            numeric(18,4) NOT NULL CHECK (amount > 0),

    -- Carried from the appropriation rather than joined at read time: the
    -- RAOD registry is one per expense class and per fund, and those reports
    -- are read constantly.
    fund_id           uuid          NOT NULL,
    fiscal_year       smallint      NOT NULL,
    expense_class     expense_class NOT NULL,
    office_id         uuid          NOT NULL,

    released_by       uuid          REFERENCES app_user(id) ON DELETE RESTRICT,
    created_at        timestamptz   NOT NULL DEFAULT now(),
    updated_at        timestamptz   NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, fiscal_year, allotment_no),

    FOREIGN KEY (tenant_id, appropriation_id) REFERENCES appropriation (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, fund_id)          REFERENCES fund          (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, office_id)        REFERENCES office        (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX allotment_by_appropriation ON allotment (appropriation_id);
CREATE INDEX allotment_registry
    ON allotment (tenant_id, fund_id, fiscal_year, expense_class);

ALTER TABLE allotment ADD CONSTRAINT allotment_tenant_key UNIQUE (tenant_id, id);


-- ── Obligations (ORS) ────────────────────────────────────────────────────────
-- Modelled as the live GAM form: Obligation Request and Status, with three
-- boxes signed by three different officers. The earlier prototype modelled a
-- single "approved_by", which cannot represent the certification chain COA
-- actually requires.
CREATE TABLE obligation (
    id                  uuid            PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id           uuid            NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    ors_no              text            NOT NULL,
    allotment_id        uuid            NOT NULL,
    fund_id             uuid            NOT NULL,
    fiscal_year         smallint        NOT NULL,
    expense_class       expense_class   NOT NULL,

    obligation_date     date            NOT NULL,

    -- ── Box A: the requesting office ────────────────────────────────────────
    requesting_office_id uuid           NOT NULL,
    payee_id            uuid,
    particulars         text            NOT NULL,
    amount              numeric(18,4)   NOT NULL CHECK (amount > 0),
    prepared_by         uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    prepared_at         timestamptz,

    -- ── Box B: the Budget Officer certifies the allotment is available ──────
    certified_budget_by uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    certified_budget_at timestamptz,

    -- ── Box C: the Accountant certifies the obligation is valid ─────────────
    certified_accounting_by uuid        REFERENCES app_user(id) ON DELETE RESTRICT,
    certified_accounting_at timestamptz,

    status              document_status NOT NULL DEFAULT 'draft',
    cancelled_reason    text,

    -- Running status carried on the ORS itself, as the form does.
    -- Maintained by trigger from the disbursement side in migration 0009.
    payable_amount      numeric(18,4)   NOT NULL DEFAULT 0 CHECK (payable_amount >= 0),
    paid_amount         numeric(18,4)   NOT NULL DEFAULT 0 CHECK (paid_amount >= 0),

    created_at          timestamptz     NOT NULL DEFAULT now(),
    updated_at          timestamptz     NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, fiscal_year, ors_no),

    FOREIGN KEY (tenant_id, allotment_id)         REFERENCES allotment (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, fund_id)              REFERENCES fund      (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, requesting_office_id) REFERENCES office    (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, payee_id)             REFERENCES payee     (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT obligation_paid_within_payable
        CHECK (paid_amount <= payable_amount),
    CONSTRAINT obligation_payable_within_amount
        CHECK (payable_amount <= amount),
    CONSTRAINT obligation_budget_certification_complete
        CHECK ((certified_budget_by IS NULL) = (certified_budget_at IS NULL)),
    CONSTRAINT obligation_accounting_certification_complete
        CHECK ((certified_accounting_by IS NULL) = (certified_accounting_at IS NULL)),
    CONSTRAINT obligation_cancellation_needs_reason
        CHECK (status <> 'cancelled' OR cancelled_reason IS NOT NULL)
);

CREATE INDEX obligation_by_allotment ON obligation (allotment_id)
    WHERE status <> 'cancelled';
CREATE INDEX obligation_registry
    ON obligation (tenant_id, fund_id, fiscal_year, expense_class, obligation_date);
CREATE INDEX obligation_payee ON obligation (tenant_id, payee_id) WHERE payee_id IS NOT NULL;

ALTER TABLE obligation ADD CONSTRAINT obligation_tenant_key UNIQUE (tenant_id, id);


-- ═══════════════════════════════════════════════════════════════════════════
--  Ceilings
-- ═══════════════════════════════════════════════════════════════════════════

/*
 * An allotment may not exceed what remains of its appropriation.
 *
 * The appropriation row is locked before the sum is taken. Without that lock,
 * two releases evaluated concurrently would each see the other's amount as
 * not-yet-committed, and both would be allowed through -- the classic
 * read-then-write race, and the one that quietly overspends a budget.
 */
CREATE OR REPLACE FUNCTION assert_allotment_within_appropriation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_authorised  numeric(18,4);
    v_released    numeric(18,4);
    v_available   numeric(18,4);
    v_ordinance   text;
BEGIN
    SELECT amount, ordinance_no INTO v_authorised, v_ordinance
      FROM appropriation
     WHERE id = NEW.appropriation_id
       FOR UPDATE;

    SELECT coalesce(sum(amount), 0) INTO v_released
      FROM allotment
     WHERE appropriation_id = NEW.appropriation_id
       AND id <> NEW.id;

    v_available := v_authorised - v_released;

    IF NEW.amount > v_available THEN
        RAISE EXCEPTION
            'This release of % is more than the % still unreleased under appropriation ordinance %.',
            to_char(NEW.amount,   'FM999,999,999,990.00'),
            to_char(v_available,  'FM999,999,999,990.00'),
            v_ordinance
            USING ERRCODE = 'check_violation',
                  HINT    = 'Reduce the amount, or ask the Sanggunian for a supplemental appropriation.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER allotment_within_appropriation
    BEFORE INSERT OR UPDATE OF amount, appropriation_id ON allotment
    FOR EACH ROW EXECUTE FUNCTION assert_allotment_within_appropriation();


/*
 * An obligation may not exceed the balance of its allotment.
 *
 * This is the requirement §4.1 states outright: "shall prevent obligations
 * that exceed the available allotment". Cancelled obligations release their
 * hold and are excluded from the sum.
 */
CREATE OR REPLACE FUNCTION assert_obligation_within_allotment()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_allotted    numeric(18,4);
    v_obligated   numeric(18,4);
    v_available   numeric(18,4);
    v_class       expense_class;
    v_fund        text;
    v_over        numeric(18,4);
BEGIN
    IF NEW.status = 'cancelled' THEN
        RETURN NEW;
    END IF;

    SELECT a.amount, a.expense_class, f.name
      INTO v_allotted, v_class, v_fund
      FROM allotment a
      JOIN fund f ON f.id = a.fund_id
     WHERE a.id = NEW.allotment_id
       FOR UPDATE OF a;

    SELECT coalesce(sum(amount), 0) INTO v_obligated
      FROM obligation
     WHERE allotment_id = NEW.allotment_id
       AND status <> 'cancelled'
       AND id <> NEW.id;

    v_available := v_allotted - v_obligated;

    IF NEW.amount > v_available THEN
        v_over := NEW.amount - v_available;
        -- Deliberately no currency symbol. Formatting money is the
        -- presentation layer's job, and a non-ASCII character in a database
        -- error message has to survive the driver, the JSON encoder, and the
        -- log — it arrived at the client as mojibake when it was here.
        -- Plain grouped digits travel safely and read the same.
        RAISE EXCEPTION
            'This obligation is % more than the allotment still available for % under the %.',
            to_char(v_over, 'FM999,999,999,990.00'),
            v_class,
            v_fund
            USING ERRCODE = 'check_violation',
                  HINT    = 'Ask the budget office to realign the allotment, or reduce the amount.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER obligation_within_allotment
    BEFORE INSERT OR UPDATE OF amount, allotment_id, status ON obligation
    FOR EACH ROW EXECUTE FUNCTION assert_obligation_within_allotment();


/*
 * The two certifications must be made by two different people, and neither by
 * the person who prepared the request.
 *
 * This is maker-checker as COA actually practises it: Box B is the Budget
 * Officer certifying funds are available, Box C is the Accountant certifying
 * the obligation is valid. One person signing both defeats the control, so
 * the database refuses it rather than trusting a screen to hide a button.
 */
CREATE OR REPLACE FUNCTION assert_obligation_separation_of_duties()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.certified_budget_by IS NOT NULL THEN
        PERFORM assert_different_actor(NEW.prepared_by, NEW.certified_budget_by, 'certify');
    END IF;

    IF NEW.certified_accounting_by IS NOT NULL THEN
        PERFORM assert_different_actor(NEW.prepared_by, NEW.certified_accounting_by, 'certify');
    END IF;

    IF NEW.certified_budget_by IS NOT NULL
       AND NEW.certified_budget_by = NEW.certified_accounting_by THEN
        RAISE EXCEPTION
            'The same person cannot certify both that the allotment is available and that the obligation is valid.'
            USING ERRCODE = 'restrict_violation',
                  HINT    = 'Box B is signed by the Budget Officer and Box C by the Accountant.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER obligation_separation_of_duties
    BEFORE INSERT OR UPDATE OF certified_budget_by, certified_accounting_by, prepared_by
    ON obligation
    FOR EACH ROW EXECUTE FUNCTION assert_obligation_separation_of_duties();


-- ── Registries ───────────────────────────────────────────────────────────────
-- RAPAL: Registry of Appropriations and Allotments. What was authorised, what
-- has been released, and what remains unreleased.
CREATE VIEW registry_rapal AS
SELECT
    ap.tenant_id,
    ap.fund_id,
    ap.fiscal_year,
    ap.expense_class,
    ap.office_id,
    ap.id                                   AS appropriation_id,
    ap.ordinance_no,
    ap.account_id,
    ap.amount                               AS appropriation_amount,
    coalesce(sum(al.amount), 0)             AS allotment_amount,
    ap.amount - coalesce(sum(al.amount), 0) AS unreleased_amount
  FROM appropriation ap
  LEFT JOIN allotment al ON al.appropriation_id = ap.id
 GROUP BY ap.id;

-- RAOD: Registry of Allotments, Obligations and Disbursements. Maintained one
-- per expense class; callers filter on expense_class to get RAOD-PS,
-- RAOD-MOOE, RAOD-FE, or RAOD-CO.
CREATE VIEW registry_raod AS
SELECT
    al.tenant_id,
    al.fund_id,
    al.fiscal_year,
    al.expense_class,
    al.office_id,
    al.id                                       AS allotment_id,
    al.allotment_no,
    al.release_date,
    al.amount                                   AS allotment_amount,
    coalesce(sum(ob.amount)         FILTER (WHERE ob.status <> 'cancelled'), 0) AS obligation_amount,
    coalesce(sum(ob.paid_amount)    FILTER (WHERE ob.status <> 'cancelled'), 0) AS disbursement_amount,
    al.amount
      - coalesce(sum(ob.amount)     FILTER (WHERE ob.status <> 'cancelled'), 0) AS unobligated_balance
  FROM allotment al
  LEFT JOIN obligation ob ON ob.allotment_id = al.id
 GROUP BY al.id;

COMMENT ON VIEW registry_raod IS
    'RAOD. Filter by expense_class for the per-class registry COA expects '
    '(RAOD-PS, RAOD-MOOE, RAOD-FE, RAOD-CO).';


CREATE TRIGGER appropriation_touch BEFORE UPDATE ON appropriation FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER allotment_touch     BEFORE UPDATE ON allotment     FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER obligation_touch    BEFORE UPDATE ON obligation    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('appropriation');
SELECT apply_tenant_guards('allotment');
SELECT apply_tenant_guards('obligation');
