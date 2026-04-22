# Mimicrypt Protocol Specification v1

## Status

Draft for implementation and audit.

## Goals

- End-to-end confidentiality over SMTP/IMAP transport
- Forward secrecy and post-compromise security
- Peer-to-peer bootstrap without any directory service
- Single-device source of truth
- Idempotent processing over unreliable email delivery

## Cryptographic Suite

- Key agreement bootstrap: `X25519`
- Identity signatures: `Ed25519`
- KDF: `HKDF-SHA256`
- Session evolution: `Double Ratchet`
- AEAD: `ChaCha20-Poly1305`

## Bootstrap

1. Initiator sends `contact_invite` with single-use opaque token.
2. Recipient replies with signed prekey bundle:
   - `identity_ed25519_public`
   - `identity_x25519_public`
   - `signed_prekey`
   - `signed_prekey_signature`
   - `one_time_prekeys`
3. Initiator verifies the signed prekey signature using the recipient identity key.
4. Initiator derives root key using X25519 exchanges and creates the first Double Ratchet session.
5. Identity is pinned with TOFU.
6. Users must complete out-of-band verification via QR, numeric code, or fingerprint comparison.

## Identity and Trust

- First valid identity key is pinned locally.
- Any future identity-key change forces `BlockedIdentityChange`.
- Signed-prekey rotation is allowed as long as the pinned identity key remains stable and signatures validate.

## Envelope

1. Chat message is serialized as canonical JSON.
2. Plaintext is padded to a bucket size.
3. Session encrypts with Double Ratchet.
4. Result is wrapped in a binary envelope with:
   - protocol header
   - ratchet header
   - ciphertext
   - signature block
5. Binary envelope is base64-encoded for transport.

## Metadata Constraints

- Email provider can still observe sender, recipient, timing, and approximate volume.
- v1 mitigates message-length leakage via padding.
- v1 may batch multiple logical messages into one transport email.
- Dummy traffic is optional and feature-flagged.

## Reliability Rules

- IMAP sync uses folder-local UID cursors.
- Reprocessing the same email must be safe.
- Dedupe key includes transport `message_id`, session id, and replay tuple.
- Delivery receipts are encrypted protocol messages sent via email.

## Replay and Poisoning Defenses

- Verify signature before mutating local state.
- Reject duplicate replay tuples.
- Ignore or quarantine malformed protocol-looking emails.
- Enforce skipped-key and replay-cache bounds.
- Track one-time-prekey depletion and trigger replenishment.

## Explicit Product Limits

- No hard realtime guarantees
- Provider delay and silent drop are possible
- SMTP acceptance does not imply recipient delivery
- Device loss without export means loss of access
