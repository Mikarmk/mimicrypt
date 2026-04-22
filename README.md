# Mimicrypt

Serverless E2EE email messenger with a Rust secure core and a real cross-platform Tauri product shell for desktop and mobile packaging.

## What is implemented

- Protocol types for versioning, metadata, receipts, trust state, and queue state
- Cryptographic primitives using `Ed25519`, `X25519`, `HKDF-SHA256`, and `ChaCha20-Poly1305`
- Peer-to-peer bootstrap with invitation tokens, signed prekeys, one-time prekeys, and TOFU pinning
- Signal-style Double Ratchet wrapper built on `double-ratchet-2`
- JSON payload -> padded plaintext -> encrypted binary envelope -> base64 transport encoding
- UID-based IMAP cursor model, delivery states, retry scheduler, and idempotent inbound tracking
- Local SQLite-backed app state plus optional OS keyring-backed secret storage
- Formal protocol draft under `docs/protocol/spec.md`
- Real local-first desktop UI flow for onboarding, contact invite, verification, queue status, and encrypted export/import placeholders

## Repository layout

- `crates/spec-types`: shared wire types and invariants
- `crates/crypto`: key generation, signing, AEAD helpers, fingerprints
- `crates/bootstrap`: invite flow, signed prekey bundles, identity pinning
- `crates/ratchet`: Double Ratchet session wrapper and replay guards
- `crates/envelope`: JSON canonicalization, padding, AAD, envelope encoding
- `crates/mail-transport`: SMTP send, IMAP receive, IDLE/polling config
- `crates/reliability`: outbox queue, retry, delivery receipts, dedupe
- `crates/storage`: local durable state and encrypted export scaffolding
- `crates/app-services`: orchestration layer used by the desktop shell
- `apps/desktop-tauri`: runnable React/Vite product shell prepared for Tauri desktop and mobile integration
- `scripts/prepare-mobile-macos.sh`: macOS mobile toolchain bootstrap for Android and iPhone targets
- `scripts/bootstrap-mobile-targets.sh`: Tauri mobile target generator for Android and iPhone shells

## Run Rust core

```bash
. "$HOME/.cargo/env"
cargo test
```

## Run product shell

```bash
cd apps/desktop-tauri
npm install
npm run dev
```

Then open `http://127.0.0.1:4173`.

## Build installers

### macOS

```bash
cd apps/desktop-tauri
npm install
npm run tauri:build
```

Artifacts are emitted under:

- `target/release/bundle/macos/Mimicrypt.app`
- `target/release/bundle/dmg/`

### Windows

Windows installers are built through GitHub Actions on a Windows runner:

- workflow: `.github/workflows/desktop-release.yml`
- outputs: `nsis` and `msi`

This is intentional because cross-building Windows installers from macOS is not the reliable path for Tauri packaging.

## Mobile targets

The project ships from one codebase for:

- macOS installer
- Windows installer
- Android app package
- iPhone app shell

### Prepare mobile toolchain on macOS

```bash
zsh scripts/prepare-mobile-macos.sh
```

This installs or verifies:

- `openjdk@17`
- `cocoapods`
- Android SDK command-line packages
- Android platform tools / build tools / NDK

Then bootstrap the native mobile targets:

```bash
cd apps/desktop-tauri
npm install
npm run mobile:bootstrap
```

### Android

On a machine with Android SDK/NDK and Java 17 configured:

```bash
cd apps/desktop-tauri
npm install
npm run android:init
npm run android:build
```

### iPhone

On a Mac with full Xcode, CocoaPods, and Apple signing configured:

```bash
cd apps/desktop-tauri
npm install
npm run ios:init
npm run ios:build
```

Important:

- iPhone packaging requires the full Xcode app, not only Command Line Tools.
- Installing on real iPhones also requires Apple signing identities and provisioning.
- This repository is prepared for that flow, but the machine doing the final build must satisfy Apple tooling requirements.

### CI

There is a dedicated workflow for mobile bootstrap/build:

- `.github/workflows/mobile-build.yml`

It builds:

- Android `apk`
- Android `aab`
- iOS app shell on macOS runners

## Current product status

- The Rust protocol/storage/transport workspace is real and tested.
- The desktop client is a real runnable messenger-style app shell with local persistence and product flows.
- The project now includes an installable Tauri desktop target for macOS and Windows packaging.
- The codebase is prepared for Tauri mobile targets for Android and iPhone packaging.
- In the installed desktop app, profile state is persisted through the Tauri native layer into the app data directory instead of relying only on browser storage.
- SMTP/IMAP commands are implemented in the Rust core, but live mailbox execution is still only partially wired into the desktop shell.
- Before publishing for broad user install, the next milestone is full Tauri command binding for account/session operations and real mailbox execution end to end.

## GitHub distribution

Recommended public release flow:

1. Push the repository to GitHub.
2. Create a version tag like `v0.1.0`.
3. Run the bundled workflows:
   - desktop installers: `.github/workflows/desktop-release.yml`
   - mobile artifacts: `.github/workflows/mobile-build.yml`
4. Publish the generated artifacts to GitHub Releases.

Current artifact targets:

- macOS: `.app` and `.dmg`
- Windows: `nsis` and `msi`
- Android: `apk` and `aab` via CI
- iPhone: Xcode project and CI build shell on macOS runners
