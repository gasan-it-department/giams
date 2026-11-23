# GIAMS — operations runbook

For whoever keeps the Accounting Office server running. Written to be followed
by someone who was not here when it was built.

---

## What the pieces are

| Piece | What it does | Where it runs |
|---|---|---|
| **PostgreSQL** | Holds the municipality's books. The one thing that must never be lost. | The office server |
| **`giams-node.exe`** | The application. Serves the screens, applies its own database migrations, uploads to the cloud. | The office server, as a Windows Service |
| **Workstation browsers** | The screens. Nothing is installed on them. | Any PC on the office LAN |
| **`giams-cloud.exe`** | Receives signed copies from municipalities. Optional. | Wherever the cloud service is hosted |

The node is the **sole writer of ledger facts**. Workstations hold drafts;
posting happens at the node. That asymmetry is deliberate — see
`docs/ARCHITECTURE.md` §2.

---

## Everyday commands

Run these from the folder holding `giams-node.exe`.

**Back up.** Do this daily, and before any upgrade.

```bash
giams-node backup
```

Writes a timestamped file to `backups\`. **Copy it off this machine.** A backup
stored only on the server it came from is not a backup — the commonest cause of
total loss is the server itself failing.

**Restore** into an empty database:

```bash
pg_restore --clean --if-exists -d giams "backups\giams-2026-08-18_083117.dump"
```

**Somebody is locked out**, or the first-run password was lost:

```bash
giams-node reset-password administrator
```

Prints a new password once, forces a change at next sign-in, and signs that
account out of every session it holds. The person will be asked to choose their
own password before they can reach any screen.

**Check the audit trail** — before any COA submission, and any time tampering
is suspected:

```bash
giams-node verify-audit
```

Every recorded action is hash-linked to the one before it. This walks the whole
chain. It exits non-zero and names the entries if anything has been altered.

---

## First installation

Create the database and its two accounts first. `giams_app` is what the
server signs in as: it owns the tables so it can apply its own migrations, but
it is not a superuser and does not bypass row level security. `giams_backup`
only reads, for backups.

```bash
createdb -U postgres giams
```

Start the server once with `DATABASE_URL` as `postgres`: the first migration
installs PostgreSQL extensions, which only a superuser can do. Stop it, then:

```bash
psql -U postgres -d giams -v app_pw="'<long random>'" -v backup_pw="'<long random>'" -f ops/limited-db-roles.sql
```

Put both into `.env` (`DATABASE_URL` as `giams_app`, `GIAMS_BACKUP_DATABASE_URL`
as `giams_backup`) and keep the `postgres` password off this machine's `.env`.
Run the script again after restoring a backup, since a restore as `postgres`
makes `postgres` the owner again.

Then, from an Administrator PowerShell on the office server:

```bash
.\ops\install-service.ps1 -InstallPath "C:\GIAMS"
```

It refuses to install if `giams-node.exe` or `.env` is missing, or if `.env`
still contains `CHANGE-ME` — each of those otherwise produces a service that
installs cleanly and then will not start. It sets the service to start after
PostgreSQL at boot and to restart itself after a crash or a failed start.
`-SourcePath D:\path\to\giams` copies the release build and its settings in
first; run it again the same way to update an installed server.

The service logs to `logs\giams-node.log` in the install folder. Read the
first-run administrator password there. It is
printed **once** and is not recoverable; if it is lost, use `reset-password`.

Set up the daily backup:

```bash
schtasks /create /tn "GIAMS backup" /tr "C:\GIAMS\giams-node.exe backup" /sc daily /st 18:00 /ru SYSTEM
```

---

## Bringing in the legacy balances

Done once per fund, before anything else is recorded. Until it is done the
ledger starts from zero and disagrees with the books it replaced from its first
day.

Export the closing trial balance from the legacy workbook as CSV — account
code, debit, credit, one row per account. Quoted thousands separators and empty
cells are fine; that is what a spreadsheet writes.

```bash
giams-node import-opening-balances balances.csv 01 2026
```

The balances arrive as one ordinary posted journal entry dated 1 January, so
every report, register and statement picks them up with no special case. That
is also what an accountant would do by hand, which is what an auditor expects
to find.

It refuses, before writing anything, if:

- the debits and credits do not agree — the difference is named, and it is a
  problem in the legacy books that must be fixed there first;
- an account code is not in the chart, or is a heading rather than a postable
  account;
- a row carries both a debit and a credit;
- balances have already been imported for that fund and year.

January must be open for the fund first. Afterwards, **check the trial balance
against the legacy books before recording anything else** — that comparison is
the only moment the two systems can be reconciled cheaply.

---

## Turning on encryption

The node serves plain HTTP until told otherwise. That is defensible on a
switched office LAN and nowhere else — sessions, passwords and voucher figures
all cross the wire in the clear.

```bash
giams-node make-cert 192.168.1.25
```

Names the server by `localhost`, its own machine name, and every address you
give it. Put the two paths it prints into `.env` as `GIAMS_TLS_CERT` and
`GIAMS_TLS_KEY`, and restart the service. The log line then reads
`listening on https://…`.

Install the `.crt` on each workstation under **Trusted Root Certification
Authorities**. Until that is done, every workstation shows a certificate
warning on every visit — and a warning that appears daily is a warning nobody
reads. From an Administrator PowerShell on each PC:

```bash
certutil -addstore Root giams-server.crt
```

Then, in the desktop app's sign-in screen, **Server: … Change** and either
**Find on the office network** (it picks up `https://` from the server) or type
`https://<server address>`.

If this server reaches the cloud or the licensing server over HTTPS on a GIAMS
certificate, set `GIAMS_TLS_CA` to that certificate in `.env`; otherwise
uploads and renewal checks are refused as untrusted.

Turn it on together with `GIAMS_BIND="0.0.0.0:8787"`, when workstations first
connect over the network. While the server only answers itself
(`127.0.0.1`), nothing crosses the wire.

A self-signed certificate proves the traffic is *encrypted*, not that the
server is who it claims. Against someone already inside the office network who
can redirect traffic, the protection is only as good as that per-workstation
trust step. Stated plainly so nobody assumes more of it than it gives.

Keep `giams-server.key` off shared drives. Anyone holding it can impersonate
this server to a workstation that trusts the certificate.

---

## Starting and stopping

The node runs as a Windows Service. Under `services.msc`, or:

```bash
sc stop GIAMS
```

Stopping it does not affect the database. Clerks lose the screens; nothing that
was saved is lost.

Check it is alive:

```bash
curl http://127.0.0.1:8787/api/health
```

---

## When something is wrong

**"Cannot reach the office server."** The node is stopped, or the workstation
is off the LAN. Check the service, then `GIAMS_BIND` — `127.0.0.1` accepts only
the server itself, so workstations need `0.0.0.0`.

**The node will not start, saying a migration was modified.** A migration file
was edited after being applied. Restore the original file. Do **not** edit the
recorded checksum unless you know precisely why it differs.

**Uploads have stopped, with "records are missing from the upload queue."**
Gap detection has halted replication because a record vanished from the outbox.
This is not self-healing by design: pushing past it would leave a voucher
permanently absent from the cloud books. Investigate before uploading again.

**"The subscription for this system has ended."** Everything stays readable and
printable — reports, statements, exports, all of it. Only new records are
paused. Install a renewal key under **System → Subscription**.

**"This computer's clock is set N days earlier…"** The server's clock reads
before a date the system has already seen, so the subscription cannot be
checked. Usually a flat CMOS battery. Correct the date and time; recording
resumes by itself.

---

## Security, in one page

- **Sessions** are signed with `GIAMS_TOKEN_SECRET`. It must be unique per
  installation. Changing it signs everyone out, which is the correct response
  to a suspected leak.
- **Passwords** are Argon2id. Nobody, including an administrator, can read one.
  Accounts lock after repeated failures, and a single computer is rate limited
  after 10 failed attempts in 5 minutes.
- **The audit trail** is hash-chained. Altering a record breaks the chain, and
  `verify-audit` finds it.
- **Posted entries cannot be edited or deleted**, by anyone, including at the
  database. Corrections are made by reversing entries, which is what an audit
  needs.
- **The node signing key** (`node-signing.key`) proves uploads came from this
  server. Keep a copy somewhere else; never commit it.
- **Row level security** scopes every query to one municipality. A query that
  forgets its tenant returns nothing rather than someone else's ledger.

---

## Before handing over to an auditor

1. `giams-node verify-audit` — confirm the trail is intact.
2. Print or export the trial balance and the statements for the period.
3. `SELECT * FROM regulatory_basis;` — the issuances this system is built to,
   ready to print. The governing manual is the **Government Accounting Manual
   for Local Government Units, COA Circular No. 2020-004 (31 January 2020)**.

---

## Still outstanding

Honest list, so nobody discovers these at the wrong moment:

- **Data-entry forms** for appropriations, obligations, and disbursement
  vouchers. The engines, registries and posting all work and are tested; the
  screens to key them in are not built. Until then those records come from the
  legacy process.
- **Bank statement import.** The matching engine works; the screen to upload a
  statement file is not built.
- **Statement of Cash Flows** and **Statement of Changes in Net Assets/Equity**.
  Both need prior-year opening balances, which arrive with the legacy data
  migration.
- **Prior-year comparatives.** Opening balances now import, so the balance
  sheet and the performance statement are correct from day one. The
  side-by-side prior-year column those statements normally carry needs a second
  year of history, which arrives with the first year-end close.
