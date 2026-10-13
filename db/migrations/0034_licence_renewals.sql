-- ═══════════════════════════════════════════════════════════════════════════
--  0034 — Renewing a subscription without issuing a new key
-- ═══════════════════════════════════════════════════════════════════════════
--  A municipality installs its licence key once. When it pays for another
--  month or year, the renewal is recorded here, on the licensing server, and
--  the municipality's own server collects it when it checks in — nobody makes,
--  sends, or pastes a new key.
--
--  A profile is paid through the later of its key's end and its latest
--  renewal. Like the keys, renewals are a permanent record: a payment that was
--  made stays made.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE licence_renewal (
    id            uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id     uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    licensee_id   uuid        NOT NULL,

    period_start  timestamptz NOT NULL,
    paid_through  timestamptz NOT NULL,
    notes         text,

    renewed_by    uuid        REFERENCES app_user(id) ON DELETE RESTRICT,
    renewed_at    timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT licence_renewal_runs_forward CHECK (paid_through > period_start),
    FOREIGN KEY (tenant_id, licensee_id) REFERENCES licensee (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX licence_renewal_by_licensee ON licence_renewal (licensee_id, paid_through DESC);

CREATE OR REPLACE FUNCTION forbid_licence_renewal_change()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'A recorded renewal is a permanent record and cannot be changed or removed.'
        USING ERRCODE = 'restrict_violation',
              HINT    = 'Record another renewal instead.';
END
$$;

CREATE TRIGGER licence_renewal_append_only
    BEFORE UPDATE OR DELETE ON licence_renewal
    FOR EACH ROW EXECUTE FUNCTION forbid_licence_renewal_change();

SELECT apply_tenant_guards('licence_renewal');
