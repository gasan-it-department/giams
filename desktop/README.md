# GIAMS desktop shell

The Tauri 2 wrapper around the React client in `../client`.

Deliberately thin — all accounting logic lives on the office node. This process
exists for the two things a browser cannot do:

1. **Credential storage.** The refresh token goes into the Windows Credential
   Manager via DPAPI, encrypted against the Windows account. Two clerks sharing
   a workstation under different logins never see each other's session. In a
   browser the best available is session storage.
2. **A pinned, versioned install.** For an audited system, *"which version
   produced this statement"* has to have an answer.

## The two path bases in `tauri.conf.json`

They are different, and it is not a mistake:

| Setting | Resolved relative to | Value |
|---|---|---|
| `beforeDevCommand`, `beforeBuildCommand` | the **Cargo workspace root** (`giams/`) | `npm --prefix client …` |
| `frontendDist` | **this directory** (`giams/desktop/`) | `../client/dist` |

Making them agree breaks one or the other. The first attempt used `../client`
for both, and the build failed looking for `D:\project\client\package.json`.

## Commands

Run from this directory.

```bash
cargo tauri dev
```

```bash
cargo tauri build
```

`build` produces an MSI and an NSIS installer under
`../target/release/bundle/`.

## `keyring` needs an explicit backend

`keyring` v3 falls back to an **in-memory mock** unless a platform feature is
enabled, and it fails silently — the symptom is every user being quietly signed
out on restart, on the one feature whose purpose is surviving restarts. The
`windows-native`, `apple-native`, and `sync-secret-service` features in
`Cargo.toml` are load-bearing; do not remove them.

The round-trip test in `src/main.rs` runs against the **real** credential store
rather than a mock, because a mock would only have proven the mock worked — and
the mock was the bug.

## Which server it talks to

The app serves its own pages (`http://tauri.localhost`), so it calls the
office server by address: `127.0.0.1:8787` unless someone picks another on the
sign-in screen (**Server: … Change**, with **Find on the office network**).
The choice is kept in the app's local storage. The server lets these origins
in (`desktop_origins` in `server/crates/api/src/lib.rs`) and nothing else, and
`connect-src` allows `http:`/`https:` so a server elsewhere on the LAN works.

## `freezePrototype` is off

It was on, and the release build opened to a blank window: ExcelJS (the JEV
Excel form) assigns `toString` on its own objects while loading, which throws
once `Object.prototype` is frozen. The browser never froze anything, so only
the installed app broke. ExcelJS is now also loaded on first export rather
than at startup.

## Installers

`cargo tauri build` writes both to `../target/release/bundle/`:
`msi/GIAMS_<version>_x64_en-US.msi` and `nsis/GIAMS_<version>_x64-setup.exe`.
