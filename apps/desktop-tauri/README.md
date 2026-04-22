# Desktop Tauri Shell

This directory contains the first desktop-client scaffold for Mimicrypt.

## Scope

- chat-like UI shell
- onboarding copy that explicitly states asynchronous delivery constraints
- verification-first UX
- delivery status labels bound to the local protocol model

## Next implementation steps

1. Add Tauri dependencies and bootstrap the frontend build.
2. Wire `mimicrypt-app-services` into Tauri commands.
3. Connect account setup, invite flow, verification screen, and chat thread.
4. Run browser-level UI checks with Playwright once the dev server exists.
