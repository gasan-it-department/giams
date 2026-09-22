-- ═══════════════════════════════════════════════════════════════════════════
--  0019 — A registry of every document number issued
-- ═══════════════════════════════════════════════════════════════════════════
--  `next_document_number` allocates from a counter row, which is gapless and
--  correct so long as *every* document goes through it. It does not stay
--  correct when a number arrives another way:
--
--    * legacy records imported from the office's spreadsheets, which carry the
--      numbers they were issued under (Phase 11)
--    * a correction applied directly in SQL
--    * a database restored from a backup taken mid-series
--
--  In each case the counter falls behind the documents, and the next
--  allocation hands out a number that already exists. That surfaced here as
--  "That journal entry number has already been used for this fund" on a
--  perfectly ordinary entry — a confusing failure with no obvious cause.
--
--  A registry of issued numbers fixes it generically. The allocator claims a
--  number by inserting it, and if the number is taken it advances and tries
--  again. An importer inserts the numbers it is bringing in, and the allocator
--  then skips past them without being told anything.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE document_number (
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    doc_type    text        NOT NULL,
    number      text        NOT NULL,
    issued_at   timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (tenant_id, doc_type, number)
);

COMMENT ON TABLE document_number IS
    'Every document number ever issued. The allocator claims one by inserting '
    'here, so a number that arrived by import or by hand can never be issued '
    'a second time.';


-- Back-fill from the documents that already exist, so the registry is true
-- from the moment it is created.
INSERT INTO document_number (tenant_id, doc_type, number)
SELECT tenant_id, 'JEV', jev_no FROM journal_entry
UNION ALL
SELECT tenant_id, 'DV',  dv_no  FROM disbursement_voucher
UNION ALL
SELECT tenant_id, 'ORS', ors_no FROM obligation
UNION ALL
SELECT tenant_id, 'OR',  or_no  FROM official_receipt
UNION ALL
SELECT tenant_id, 'RCD', rcd_no FROM report_of_collections
ON CONFLICT DO NOTHING;


/*
 * Allocates the next unused document number.
 *
 * Still gapless in the ordinary case: the counter advances by one and the
 * number it produces is free. When the counter has fallen behind — an import,
 * a restore, a hand-edit — it advances past what is taken rather than failing.
 *
 * Skipping numbers that are already in use is not a gap: those numbers exist
 * on documents. A gap is a number nobody holds, and this cannot create one.
 */
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
    v_no       bigint;
    v_prefix   text;
    v_padding  smallint;
    v_number   text;
    v_attempts integer := 0;
BEGIN
    INSERT INTO document_series (tenant_id, doc_type, fund_id, fiscal_year, month)
         VALUES (p_tenant_id, p_doc_type, p_fund_id, p_fiscal_year, p_month)
    ON CONFLICT DO NOTHING;

    LOOP
        v_attempts := v_attempts + 1;

        -- Row-locked by the UPDATE: concurrent callers queue here, so two
        -- clerks can never be handed the same number.
        UPDATE document_series
           SET next_no = next_no + 1
         WHERE tenant_id   = p_tenant_id
           AND doc_type    = p_doc_type
           AND fiscal_year = p_fiscal_year
           AND coalesce(month, 0) = coalesce(p_month, 0)
           AND coalesce(fund_id, '00000000-0000-0000-0000-000000000000'::uuid)
             = coalesce(p_fund_id, '00000000-0000-0000-0000-000000000000'::uuid)
        RETURNING next_no - 1, prefix, padding
             INTO v_no, v_prefix, v_padding;

        IF v_no IS NULL THEN
            RAISE EXCEPTION 'no document series for % in fiscal year %', p_doc_type, p_fiscal_year
                USING ERRCODE = 'no_data_found';
        END IF;

        v_number := concat_ws('-',
            nullif(v_prefix, ''),
            p_doc_type,
            p_fiscal_year::text,
            CASE WHEN p_month IS NOT NULL THEN lpad(p_month::text, 2, '0') END,
            lpad(v_no::text, v_padding, '0')
        );

        -- Claim it. A conflict means the number is already on a document, so
        -- the loop advances the counter and tries the next one.
        BEGIN
            INSERT INTO document_number (tenant_id, doc_type, number)
                 VALUES (p_tenant_id, p_doc_type, v_number);
            RETURN v_number;
        EXCEPTION WHEN unique_violation THEN
            -- A counter far behind its documents would otherwise spin. Ten
            -- thousand is past any plausible drift and short of a hang.
            IF v_attempts > 10000 THEN
                RAISE EXCEPTION
                    'Could not find an unused % number for %.', p_doc_type, p_fiscal_year
                    USING ERRCODE = 'check_violation',
                          HINT = 'The numbering for this document type needs to be reset by an administrator.';
            END IF;
        END;
    END LOOP;
END
$$;

COMMENT ON FUNCTION next_document_number IS
    'Gapless voucher numbering that survives imports and restores: claims each '
    'number in document_number, advancing past any already in use.';

SELECT apply_tenant_guards('document_number');
