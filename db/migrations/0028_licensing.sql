-- ════════════════════════════════════════════════════════════════════════════
-- 0028 — Licensing
--
-- Two tables. The licence itself, and the clock history that stops an expiry
-- date being defeated with the BIOS.
--
-- Neither is replicated to the cloud. A licence is about this installation,
-- and a copy of it in a shared database would be one more place to edit it.
-- ════════════════════════════════════════════════════════════════════════════


-- ── The licence on file ──────────────────────────────────────────────────────
-- One per municipality. The token is the authority; the columns beside it are
-- a decoded copy for querying and display, and are never trusted on their own.
-- Anything that decides what the system may do re-verifies the token.
CREATE TABLE licence (
    tenant_id       uuid        PRIMARY KEY REFERENCES tenant(id) ON DELETE RESTRICT,

    token           text        NOT NULL,

    plan            text        NOT NULL,
    seats           integer     NOT NULL CHECK (seats > 0),
    not_before      timestamptz NOT NULL,
    expires_at      timestamptz NOT NULL,
    grace_days      integer     NOT NULL CHECK (grace_days >= 0),

    installed_at    timestamptz NOT NULL DEFAULT now(),
    installed_by    uuid        REFERENCES app_user(id),

    CHECK (expires_at > not_before)
);

COMMENT ON TABLE licence IS
    'The subscription in force. The token is authoritative; the decoded '
    'columns exist so the office can be shown what it holds without decoding '
    'anything, and are re-derived from the token on every install.';

COMMENT ON COLUMN licence.token IS
    'Ed25519-signed. Editing any decoded column beside it changes nothing: the '
    'system re-verifies the token itself before deciding what is permitted.';


-- ── Clock history ────────────────────────────────────────────────────────────
-- The high-water mark that makes an offline expiry date meaningful. Time is
-- only ever allowed to move forward; each entry commits to the one before it,
-- so the record cannot be quietly edited by whoever finds the database.
CREATE TABLE licence_clock (
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    seq         bigint      NOT NULL,
    observed_at timestamptz NOT NULL,
    chain_hash  bytea       NOT NULL,
    entry_hash  bytea       NOT NULL,

    PRIMARY KEY (tenant_id, seq)
);

COMMENT ON TABLE licence_clock IS
    'The latest moment this installation has ever seen. A clock reading '
    'earlier than this is not believed, and an unbelievable clock is treated '
    'as expiry -- never as a reason to lock anyone out of their own books.';

CREATE INDEX licence_clock_latest ON licence_clock (tenant_id, seq DESC);


SELECT apply_tenant_guards('licence');
SELECT apply_tenant_guards('licence_clock');
