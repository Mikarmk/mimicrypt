use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::Zeroize;

use mimicrypt_spec_types::CIPHER_SUITE_V1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityKeypair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPrekey {
    pub key_id: u32,
    pub public_key: [u8; 32],
    pub signature: Vec<u8>,
    pub created_unix: i64,
    pub expires_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneTimePrekey {
    pub key_id: u32,
    pub public_key: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeys {
    pub identity: IdentityKeypair,
    pub x25519_identity_private: [u8; 32],
    pub x25519_identity_public: [u8; 32],
    pub signed_prekey_private: [u8; 32],
    pub signed_prekey: SignedPrekey,
    pub one_time_prekeys_private: Vec<[u8; 32]>,
    pub one_time_prekeys_public: Vec<OneTimePrekey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrekeyBundlePublic {
    pub cipher_suite_id: String,
    pub identity_ed25519_public: [u8; 32],
    pub identity_x25519_public: [u8; 32],
    pub signed_prekey: SignedPrekey,
    pub one_time_prekeys: Vec<OneTimePrekey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    pub signer_public_key: [u8; 32],
    pub signature: Vec<u8>,
}

pub fn generate_device_keys(
    now_unix: i64,
    signed_prekey_lifetime_days: i64,
    one_time_count: usize,
) -> DeviceKeys {
    let identity_signing = SigningKey::generate(&mut OsRng);
    let identity_verifying = identity_signing.verifying_key();

    let x25519_identity_private = StaticSecret::random_from_rng(OsRng);
    let x25519_identity_public = X25519PublicKey::from(&x25519_identity_private);

    let signed_prekey_private = StaticSecret::random_from_rng(OsRng);
    let signed_prekey_public = X25519PublicKey::from(&signed_prekey_private);
    let signed_prekey_signature = identity_signing.sign(signed_prekey_public.as_bytes());

    let mut one_time_prekeys_private = Vec::with_capacity(one_time_count);
    let mut one_time_prekeys_public = Vec::with_capacity(one_time_count);

    for key_id in 0..one_time_count as u32 {
        let private = StaticSecret::random_from_rng(OsRng);
        let public = X25519PublicKey::from(&private);
        one_time_prekeys_private.push(private.to_bytes());
        one_time_prekeys_public.push(OneTimePrekey {
            key_id,
            public_key: public.to_bytes(),
        });
    }

    DeviceKeys {
        identity: IdentityKeypair {
            signing_key: identity_signing,
            verifying_key: identity_verifying,
        },
        x25519_identity_private: x25519_identity_private.to_bytes(),
        x25519_identity_public: x25519_identity_public.to_bytes(),
        signed_prekey_private: signed_prekey_private.to_bytes(),
        signed_prekey: SignedPrekey {
            key_id: 1,
            public_key: signed_prekey_public.to_bytes(),
            signature: signed_prekey_signature.to_bytes().to_vec(),
            created_unix: now_unix,
            expires_unix: now_unix + signed_prekey_lifetime_days * 24 * 60 * 60,
        },
        one_time_prekeys_private,
        one_time_prekeys_public,
    }
}

pub fn export_public_bundle(keys: &DeviceKeys) -> PrekeyBundlePublic {
    PrekeyBundlePublic {
        cipher_suite_id: CIPHER_SUITE_V1.to_owned(),
        identity_ed25519_public: keys.identity.verifying_key.to_bytes(),
        identity_x25519_public: keys.x25519_identity_public,
        signed_prekey: keys.signed_prekey.clone(),
        one_time_prekeys: keys.one_time_prekeys_public.clone(),
    }
}

pub fn verify_signed_prekey(bundle: &PrekeyBundlePublic) -> Result<()> {
    let verifying_key = VerifyingKey::from_bytes(&bundle.identity_ed25519_public)?;
    let signature_bytes: [u8; 64] = bundle
        .signed_prekey
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("invalid signed prekey signature length"))?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(&bundle.signed_prekey.public_key, &signature)
        .context("signed prekey signature verification failed")?;
    Ok(())
}

pub fn sign_bytes(identity: &IdentityKeypair, bytes: &[u8]) -> SignedEnvelope {
    let signature = identity.signing_key.sign(bytes);
    SignedEnvelope {
        signer_public_key: identity.verifying_key.to_bytes(),
        signature: signature.to_bytes().to_vec(),
    }
}

pub fn verify_signature(bytes: &[u8], envelope: &SignedEnvelope) -> Result<()> {
    let verifying = VerifyingKey::from_bytes(&envelope.signer_public_key)?;
    let signature_bytes: [u8; 64] = envelope
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("invalid envelope signature length"))?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying.verify(bytes, &signature)?;
    Ok(())
}

pub fn fingerprint_ed25519(public_key: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key);
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|chunk| chunk.join(""))
        .collect::<Vec<_>>()
        .join("-")
}

pub fn derive_bootstrap_root_key_initiator(
    local_identity_private: [u8; 32],
    local_ephemeral_private: [u8; 32],
    remote_identity_public: [u8; 32],
    remote_signed_prekey_public: [u8; 32],
    remote_one_time_prekey_public: Option<[u8; 32]>,
) -> Result<[u8; 32]> {
    let local_identity_private = StaticSecret::from(local_identity_private);
    let local_ephemeral_private = StaticSecret::from(local_ephemeral_private);
    let remote_identity_public = X25519PublicKey::from(remote_identity_public);
    let remote_signed_prekey_public = X25519PublicKey::from(remote_signed_prekey_public);

    let dh1 = local_identity_private.diffie_hellman(&remote_signed_prekey_public);
    let dh2 = local_ephemeral_private.diffie_hellman(&remote_identity_public);
    let dh3 = local_ephemeral_private.diffie_hellman(&remote_signed_prekey_public);

    let mut input = Vec::with_capacity(128);
    input.extend_from_slice(dh1.as_bytes());
    input.extend_from_slice(dh2.as_bytes());
    input.extend_from_slice(dh3.as_bytes());

    if let Some(remote_one_time) = remote_one_time_prekey_public {
        let remote_one_time = X25519PublicKey::from(remote_one_time);
        let dh4 = local_ephemeral_private.diffie_hellman(&remote_one_time);
        input.extend_from_slice(dh4.as_bytes());
    }

    let hk = Hkdf::<Sha256>::new(Some(b"mimicrypt/bootstrap/v1"), &input);
    let mut okm = [0_u8; 32];
    hk.expand(b"root-key", &mut okm)
        .map_err(|_| anyhow!("failed to derive bootstrap root key"))?;
    input.zeroize();
    Ok(okm)
}

pub fn derive_bootstrap_root_key_responder(
    local_identity_private: [u8; 32],
    local_signed_prekey_private: [u8; 32],
    local_one_time_prekey_private: Option<[u8; 32]>,
    remote_identity_public: [u8; 32],
    remote_ephemeral_public: [u8; 32],
) -> Result<[u8; 32]> {
    let local_identity_private = StaticSecret::from(local_identity_private);
    let local_signed_prekey_private = StaticSecret::from(local_signed_prekey_private);
    let remote_identity_public = X25519PublicKey::from(remote_identity_public);
    let remote_ephemeral_public = X25519PublicKey::from(remote_ephemeral_public);

    let dh1 = local_signed_prekey_private.diffie_hellman(&remote_identity_public);
    let dh2 = local_identity_private.diffie_hellman(&remote_ephemeral_public);
    let dh3 = local_signed_prekey_private.diffie_hellman(&remote_ephemeral_public);

    let mut input = Vec::with_capacity(128);
    input.extend_from_slice(dh1.as_bytes());
    input.extend_from_slice(dh2.as_bytes());
    input.extend_from_slice(dh3.as_bytes());

    if let Some(one_time_private) = local_one_time_prekey_private {
        let one_time_private = StaticSecret::from(one_time_private);
        let dh4 = one_time_private.diffie_hellman(&remote_ephemeral_public);
        input.extend_from_slice(dh4.as_bytes());
    }

    let hk = Hkdf::<Sha256>::new(Some(b"mimicrypt/bootstrap/v1"), &input);
    let mut okm = [0_u8; 32];
    hk.expand(b"root-key", &mut okm)
        .map_err(|_| anyhow!("failed to derive responder bootstrap root key"))?;
    input.zeroize();
    Ok(okm)
}

pub fn random_x25519_private() -> [u8; 32] {
    StaticSecret::random_from_rng(OsRng).to_bytes()
}

pub fn encrypt_chacha20(
    key_bytes: &[u8; 32],
    nonce_bytes: &[u8; 12],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    let ciphertext = cipher.encrypt(
        nonce,
        Payload {
            msg: plaintext,
            aad,
        },
    )?;
    Ok(ciphertext)
}

pub fn decrypt_chacha20(
    key_bytes: &[u8; 32],
    nonce_bytes: &[u8; 12],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(
        nonce,
        Payload {
            msg: ciphertext,
            aad,
        },
    )?;
    Ok(plaintext)
}

pub fn random_nonce_96() -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

pub fn sha256(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

pub fn consume_one_time_prekey(keys: &mut DeviceKeys, key_id: u32) -> Result<[u8; 32]> {
    let idx = keys
        .one_time_prekeys_public
        .iter()
        .position(|item| item.key_id == key_id)
        .ok_or_else(|| anyhow!("one-time prekey {key_id} not found"))?;
    keys.one_time_prekeys_public.remove(idx);
    Ok(keys.one_time_prekeys_private.remove(idx))
}

pub fn maybe_validate_signed_prekey_age(bundle: &PrekeyBundlePublic, now_unix: i64) -> Result<()> {
    if bundle.signed_prekey.expires_unix < now_unix {
        bail!("signed prekey expired");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimicrypt_spec_types::DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS;

    #[test]
    fn bundle_signature_verifies() {
        let keys = generate_device_keys(1_700_000_000, DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS, 4);
        let bundle = export_public_bundle(&keys);
        verify_signed_prekey(&bundle).unwrap();
    }

    #[test]
    fn shared_bootstrap_key_matches_for_both_parties() {
        let alice = generate_device_keys(1_700_000_000, 14, 4);
        let mut bob = generate_device_keys(1_700_000_000, 14, 4);
        let bob_bundle = export_public_bundle(&bob);
        let bob_otk = bob_bundle.one_time_prekeys[0].clone();

        let alice_ephemeral_private = random_x25519_private();
        let alice_root = derive_bootstrap_root_key_initiator(
            alice.x25519_identity_private,
            alice_ephemeral_private,
            bob_bundle.identity_x25519_public,
            bob_bundle.signed_prekey.public_key,
            Some(bob_otk.public_key),
        )
        .unwrap();

        let bob_one_time_private = consume_one_time_prekey(&mut bob, bob_otk.key_id).unwrap();
        let alice_ephemeral_secret = StaticSecret::from(alice_ephemeral_private);
        let alice_ephemeral_public = X25519PublicKey::from(&alice_ephemeral_secret).to_bytes();
        let bob_root = derive_bootstrap_root_key_responder(
            bob.x25519_identity_private,
            bob.signed_prekey_private,
            Some(bob_one_time_private),
            alice.x25519_identity_public,
            alice_ephemeral_public,
        )
        .unwrap();

        assert_eq!(alice_root, bob_root);
    }
}
