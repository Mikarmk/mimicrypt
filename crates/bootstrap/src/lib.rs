use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use mimicrypt_crypto::{
    export_public_bundle, fingerprint_ed25519, maybe_validate_signed_prekey_age, sign_bytes,
    verify_signed_prekey, DeviceKeys, PrekeyBundlePublic,
};
use mimicrypt_spec_types::{ContactIdentity, InvitePayload, TrustState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInvite {
    pub payload: InvitePayload,
    pub bundle_hint_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInviteResponse {
    pub invite_id: Uuid,
    pub responder_email: String,
    pub responder_device_id: Uuid,
    pub bundle: PrekeyBundlePublic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustDecision {
    pub pinned_fingerprint: String,
    pub trust_state: TrustState,
    pub changed: bool,
}

pub fn create_contact_invite(
    inviter_email: &str,
    inviter_device_id: Uuid,
    inviter_keys: &DeviceKeys,
    now: OffsetDateTime,
) -> ContactInvite {
    let mut token = [0_u8; 32];
    OsRng.fill_bytes(&mut token);

    ContactInvite {
        payload: InvitePayload {
            invite_id: Uuid::new_v4(),
            inviter_email: inviter_email.to_owned(),
            inviter_device_id,
            issued_at: now,
            expires_at: now + Duration::days(3),
            opaque_token_b64: STANDARD_NO_PAD.encode(token),
        },
        bundle_hint_fingerprint: fingerprint_ed25519(
            &inviter_keys.identity.verifying_key.to_bytes(),
        ),
    }
}

pub fn validate_contact_invite(invite: &ContactInvite, now: OffsetDateTime) -> Result<()> {
    if invite.payload.expires_at < now {
        bail!("invite expired");
    }
    Ok(())
}

pub fn create_invite_response(
    invite: &ContactInvite,
    responder_email: &str,
    responder_device_id: Uuid,
    responder_keys: &DeviceKeys,
) -> ContactInviteResponse {
    ContactInviteResponse {
        invite_id: invite.payload.invite_id,
        responder_email: responder_email.to_owned(),
        responder_device_id,
        bundle: export_public_bundle(responder_keys),
    }
}

pub fn verify_invite_response(
    invite: &ContactInvite,
    response: &ContactInviteResponse,
    now: OffsetDateTime,
) -> Result<ContactIdentity> {
    if response.invite_id != invite.payload.invite_id {
        bail!("invite id mismatch");
    }

    verify_signed_prekey(&response.bundle)?;
    maybe_validate_signed_prekey_age(&response.bundle, now.unix_timestamp())?;

    Ok(ContactIdentity {
        email: response.responder_email.clone(),
        device_id: response.responder_device_id,
        identity_fingerprint: fingerprint_ed25519(&response.bundle.identity_ed25519_public),
        trust_state: TrustState::Unverified,
        verified_at: None,
    })
}

pub fn tofu_pin(
    existing: Option<&ContactIdentity>,
    bundle: &PrekeyBundlePublic,
) -> Result<TrustDecision> {
    let fingerprint = fingerprint_ed25519(&bundle.identity_ed25519_public);
    match existing {
        None => Ok(TrustDecision {
            pinned_fingerprint: fingerprint,
            trust_state: TrustState::Unverified,
            changed: false,
        }),
        Some(existing) if existing.identity_fingerprint == fingerprint => Ok(TrustDecision {
            pinned_fingerprint: fingerprint,
            trust_state: existing.trust_state,
            changed: false,
        }),
        Some(_) => Ok(TrustDecision {
            pinned_fingerprint: fingerprint,
            trust_state: TrustState::BlockedIdentityChange,
            changed: true,
        }),
    }
}

pub fn safety_number(local_fingerprint: &str, remote_fingerprint: &str) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(local_fingerprint.as_bytes());
    bytes.extend_from_slice(remote_fingerprint.as_bytes());
    let digest = Sha256::digest(&bytes);
    digest[..15]
        .chunks(5)
        .map(|part| {
            let number = u32::from_be_bytes([0, part[0], part[1], part[2]]);
            format!("{number:05}")
        })
        .collect::<Vec<_>>()
        .join("-")
}

pub fn encoded_bundle_payload(bundle: &PrekeyBundlePublic) -> Result<String> {
    let json = serde_json::to_vec(bundle)?;
    Ok(STANDARD_NO_PAD.encode(json))
}

pub fn decoded_bundle_payload(encoded: &str) -> Result<PrekeyBundlePublic> {
    let bytes = STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|_| anyhow!("invalid bundle base64 payload"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn signed_bundle_blob(keys: &DeviceKeys) -> Result<Vec<u8>> {
    let bundle = export_public_bundle(keys);
    let bytes = serde_json::to_vec(&bundle)?;
    let signed = sign_bytes(&keys.identity, &bytes);
    Ok(serde_json::to_vec(&(bundle, signed))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimicrypt_crypto::generate_device_keys;
    use mimicrypt_spec_types::DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS;

    #[test]
    fn invite_roundtrip_establishes_unverified_contact() {
        let now = OffsetDateTime::now_utc();
        let inviter =
            generate_device_keys(now.unix_timestamp(), DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS, 8);
        let responder =
            generate_device_keys(now.unix_timestamp(), DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS, 8);
        let invite = create_contact_invite("alice@example.com", Uuid::new_v4(), &inviter, now);
        let response =
            create_invite_response(&invite, "bob@example.com", Uuid::new_v4(), &responder);
        let contact = verify_invite_response(&invite, &response, now).unwrap();
        assert_eq!(contact.trust_state, TrustState::Unverified);
    }
}
