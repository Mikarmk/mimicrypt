use anyhow::{bail, Result};
use bincode::{deserialize, serialize};
use double_ratchet_2::{header::Header, ratchet::Ratchet, PublicKey, StaticSecret};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use uuid::Uuid;

use mimicrypt_spec_types::{ReplayKey, MAX_REPLAY_CACHE_PER_SESSION, MAX_SKIPPED_MESSAGE_KEYS};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetHeaderWire {
    pub serialized_header: Vec<u8>,
    pub public_key: Vec<u8>,
    pub message_number: u32,
    pub previous_chain_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetCiphertext {
    pub header: RatchetHeaderWire,
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

#[derive(Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: Uuid,
    #[serde(with = "serde_ratchet")]
    pub ratchet: Ratchet<StaticSecret>,
    pub replay_cache: VecDeque<ReplayKey>,
}

impl SessionState {
    pub fn init_bob(session_id: Uuid, root_key: [u8; 32]) -> (Self, [u8; 32]) {
        let (ratchet, public_key) = Ratchet::<StaticSecret>::init_bob(root_key);
        (
            Self {
                session_id,
                ratchet,
                replay_cache: VecDeque::new(),
            },
            public_key.as_bytes().to_owned(),
        )
    }

    pub fn init_alice(session_id: Uuid, root_key: [u8; 32], bob_ratchet_public: [u8; 32]) -> Self {
        let public = PublicKey::from(bob_ratchet_public);
        Self {
            session_id,
            ratchet: Ratchet::<StaticSecret>::init_alice(root_key, public),
            replay_cache: VecDeque::new(),
        }
    }

    pub fn encrypt(&mut self, plaintext: &[u8], aad: &[u8]) -> RatchetCiphertext {
        let (header, ciphertext, nonce) = self.ratchet.ratchet_encrypt(plaintext, aad);
        RatchetCiphertext {
            header: RatchetHeaderWire {
                serialized_header: header.concat(b""),
                public_key: header.public_key.clone(),
                message_number: header.n as u32,
                previous_chain_length: header.pn as u32,
            },
            ciphertext,
            nonce,
        }
    }

    pub fn decrypt(
        &mut self,
        sender_device_id: Uuid,
        payload: &RatchetCiphertext,
        aad: &[u8],
    ) -> Result<Vec<u8>> {
        self.ensure_replay_not_seen(sender_device_id, payload)?;
        if self.replay_cache.len() > MAX_SKIPPED_MESSAGE_KEYS {
            bail!("skipped key bound exceeded");
        }

        let header = Header::<PublicKey>::from(payload.header.serialized_header.as_slice());

        let plaintext =
            self.ratchet
                .ratchet_decrypt(&header, &payload.ciphertext, &payload.nonce, aad);

        self.remember_replay(sender_device_id, payload);
        Ok(plaintext)
    }

    pub fn export(&self) -> Result<Vec<u8>> {
        Ok(serialize(self)?)
    }

    pub fn import(bytes: &[u8]) -> Result<Self> {
        Ok(deserialize(bytes)?)
    }

    fn ensure_replay_not_seen(
        &self,
        sender_device_id: Uuid,
        payload: &RatchetCiphertext,
    ) -> Result<()> {
        let replay = ReplayKey {
            session_id: self.session_id,
            sender_device_id,
            ratchet_public_key: payload.header.public_key.clone(),
            message_number: payload.header.message_number,
            ciphertext_sha256: Sha256::digest(&payload.ciphertext).to_vec(),
        };

        if self.replay_cache.iter().any(|entry| entry == &replay) {
            bail!("replayed message detected");
        }
        Ok(())
    }

    fn remember_replay(&mut self, sender_device_id: Uuid, payload: &RatchetCiphertext) {
        if self.replay_cache.len() == MAX_REPLAY_CACHE_PER_SESSION {
            self.replay_cache.pop_front();
        }
        self.replay_cache.push_back(ReplayKey {
            session_id: self.session_id,
            sender_device_id,
            ratchet_public_key: payload.header.public_key.clone(),
            message_number: payload.header.message_number,
            ciphertext_sha256: Sha256::digest(&payload.ciphertext).to_vec(),
        });
    }
}

mod serde_ratchet {
    use super::*;
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(ratchet: &Ratchet<StaticSecret>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = bincode::serialize(ratchet).map_err(serde::ser::Error::custom)?;
        serializer.serialize_bytes(&bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Ratchet<StaticSecret>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Vec::<u8>::deserialize(deserializer)?;
        bincode::deserialize(&bytes).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratchet_roundtrip_and_replay_guard() {
        let root = [7_u8; 32];
        let session_id = Uuid::new_v4();
        let sender_device_id = Uuid::new_v4();
        let (mut bob, bob_pub) = SessionState::init_bob(session_id, root);
        let mut alice = SessionState::init_alice(session_id, root, bob_pub);
        let aad = b"aad";
        let payload = alice.encrypt(b"hello", aad);
        let plaintext = bob.decrypt(sender_device_id, &payload, aad).unwrap();
        assert_eq!(plaintext, b"hello");
        assert!(bob.decrypt(sender_device_id, &payload, aad).is_err());
    }
}
