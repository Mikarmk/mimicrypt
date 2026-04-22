use anyhow::Result;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use mimicrypt_crypto::{sign_bytes, verify_signature, DeviceKeys, SignedEnvelope};
use mimicrypt_ratchet::{RatchetCiphertext, SessionState};
use mimicrypt_spec_types::{
    ensure_v1, AppMessage, AttachmentManifest, MessageType, ProtocolHeader, RatchetHeaderData,
    CIPHER_SUITE_V1, PROTOCOL_VERSION_V1,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AadDocument {
    pub protocol_version: u16,
    pub message_type: MessageType,
    pub cipher_suite_id: String,
    pub session_id: Uuid,
    pub sender_device_id: Uuid,
    pub sender_fingerprint: String,
    pub ratchet_public_key: Vec<u8>,
    pub message_number: u32,
    pub previous_chain_length: u32,
    pub timestamp_unix: i64,
    pub attachment_manifest_hash: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireEnvelope {
    pub header: ProtocolHeader,
    pub ratchet_header: RatchetHeaderData,
    pub ratchet_header_bytes: Vec<u8>,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub signature: SignedEnvelope,
}

pub fn canonical_message_json(message: &AppMessage) -> Result<Vec<u8>> {
    let mut attachments = message.attachments.clone();
    attachments.sort_by_key(|item| item.attachment_id);
    let value = json!({
        "message_id": message.message_id,
        "session_id": message.session_id,
        "sender_email": message.sender_email,
        "sender_device_id": message.sender_device_id,
        "body": message.body,
        "created_at": message.created_at.unix_timestamp(),
        "attachments": attachments,
    });
    Ok(serde_json::to_vec(&value)?)
}

pub fn padded_plaintext(plaintext: &[u8]) -> Vec<u8> {
    let buckets = [512_usize, 1024, 2048, 4096, 8192];
    let target = buckets
        .into_iter()
        .find(|bucket| *bucket >= plaintext.len())
        .unwrap_or_else(|| ((plaintext.len() / 16384) + 1) * 16384);
    let mut padded = plaintext.to_vec();
    padded.resize(target, 0);
    padded
}

pub fn build_aad(
    header: &ProtocolHeader,
    ratchet: &RatchetHeaderData,
    attachments: &[AttachmentManifest],
) -> Result<Vec<u8>> {
    let attachment_manifest_hash = serde_json::to_vec(attachments)?;
    let doc = AadDocument {
        protocol_version: header.protocol_version,
        message_type: header.message_type,
        cipher_suite_id: header.cipher_suite_id.clone(),
        session_id: header.session_id,
        sender_device_id: header.sender_device_id,
        sender_fingerprint: header.sender_fingerprint.clone(),
        ratchet_public_key: ratchet.ratchet_public_key.clone(),
        message_number: ratchet.message_number,
        previous_chain_length: ratchet.previous_chain_length,
        timestamp_unix: header.timestamp.unix_timestamp(),
        attachment_manifest_hash,
    };
    Ok(serde_json::to_vec(&doc)?)
}

pub fn seal_message(
    sender_keys: &DeviceKeys,
    sender_device_id: Uuid,
    sender_fingerprint: String,
    ratchet: &mut SessionState,
    message: &AppMessage,
) -> Result<String> {
    let plaintext = canonical_message_json(message)?;
    let padded = padded_plaintext(&plaintext);
    let ratchet_payload = ratchet.encrypt(&padded, b"");
    let header = ProtocolHeader {
        protocol_version: PROTOCOL_VERSION_V1,
        message_type: MessageType::Ciphertext,
        cipher_suite_id: CIPHER_SUITE_V1.to_owned(),
        session_id: message.session_id,
        sender_device_id,
        sender_fingerprint,
        timestamp: OffsetDateTime::now_utc(),
    };
    let ratchet_header = RatchetHeaderData {
        ratchet_public_key: ratchet_payload.header.public_key.clone(),
        message_number: ratchet_payload.header.message_number,
        previous_chain_length: ratchet_payload.header.previous_chain_length,
    };
    let signed_bytes = serialize(&(
        header.clone(),
        ratchet_header.clone(),
        &ratchet_payload.ciphertext,
    ))?;
    let wire = WireEnvelope {
        header,
        ratchet_header,
        ratchet_header_bytes: ratchet_payload.header.serialized_header.clone(),
        nonce: ratchet_payload.nonce,
        ciphertext: ratchet_payload.ciphertext,
        signature: sign_bytes(&sender_keys.identity, &signed_bytes),
    };
    Ok(STANDARD_NO_PAD.encode(serialize(&wire)?))
}

pub fn open_message(
    encoded: &str,
    sender_device_id: Uuid,
    ratchet: &mut SessionState,
) -> Result<AppMessage> {
    let bytes = STANDARD_NO_PAD.decode(encoded)?;
    let wire: WireEnvelope = deserialize(&bytes)?;
    ensure_v1(wire.header.protocol_version, &wire.header.cipher_suite_id)?;

    let signed_bytes = serialize(&(
        wire.header.clone(),
        wire.ratchet_header.clone(),
        &wire.ciphertext,
    ))?;
    verify_signature(&signed_bytes, &wire.signature)?;

    let plaintext = ratchet.decrypt(
        sender_device_id,
        &RatchetCiphertext {
            header: mimicrypt_ratchet::RatchetHeaderWire {
                serialized_header: wire.ratchet_header_bytes,
                public_key: wire.ratchet_header.ratchet_public_key.clone(),
                message_number: wire.ratchet_header.message_number,
                previous_chain_length: wire.ratchet_header.previous_chain_length,
            },
            ciphertext: wire.ciphertext,
            nonce: wire.nonce,
        },
        b"",
    )?;

    let trimmed = plaintext
        .into_iter()
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    let value: Value = serde_json::from_slice(&trimmed)?;
    let attachments = serde_json::from_value(value["attachments"].clone())?;
    let created_at =
        OffsetDateTime::from_unix_timestamp(value["created_at"].as_i64().unwrap_or_default())?;
    Ok(AppMessage {
        message_id: serde_json::from_value(value["message_id"].clone())?,
        session_id: serde_json::from_value(value["session_id"].clone())?,
        sender_email: value["sender_email"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        sender_device_id: serde_json::from_value(value["sender_device_id"].clone())?,
        body: value["body"].as_str().unwrap_or_default().to_owned(),
        created_at,
        attachments,
        delivery_state: mimicrypt_spec_types::DeliveryState::FetchedByPeer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimicrypt_crypto::{fingerprint_ed25519, generate_device_keys};
    use mimicrypt_spec_types::{DeliveryState, DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS};

    #[test]
    fn padded_message_roundtrip() {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let keys = generate_device_keys(now, DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS, 8);
        let session_id = Uuid::new_v4();
        let sender_device_id = Uuid::new_v4();
        let (mut bob, bob_pub) = SessionState::init_bob(session_id, [42_u8; 32]);
        let mut alice = SessionState::init_alice(session_id, [42_u8; 32], bob_pub);
        let encoded = seal_message(
            &keys,
            sender_device_id,
            fingerprint_ed25519(&keys.identity.verifying_key.to_bytes()),
            &mut alice,
            &AppMessage {
                message_id: Uuid::new_v4(),
                session_id,
                sender_email: "alice@example.com".into(),
                sender_device_id,
                body: "hello".into(),
                created_at: OffsetDateTime::now_utc(),
                attachments: vec![],
                delivery_state: DeliveryState::Queued,
            },
        )
        .unwrap();

        let opened = open_message(&encoded, sender_device_id, &mut bob).unwrap();
        assert_eq!(opened.body, "hello");
    }
}
