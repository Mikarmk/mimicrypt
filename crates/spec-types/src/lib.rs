use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use time::OffsetDateTime;
use uuid::Uuid;

pub const PROTOCOL_VERSION_V1: u16 = 1;
pub const CIPHER_SUITE_V1: &str = "X25519+Ed25519+HKDF-SHA256+DR+ChaCha20Poly1305";
pub const DEFAULT_SIGNED_PREKEY_LIFETIME_DAYS: i64 = 14;
pub const DEFAULT_ONE_TIME_PREKEY_STOCK: usize = 100;
pub const MAX_SKIPPED_MESSAGE_KEYS: usize = 1000;
pub const MAX_REPLAY_CACHE_PER_SESSION: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    ContactInvite,
    PrekeyBundle,
    Ciphertext,
    DeliveryReceipt,
    ReadReceipt,
    KeyRotation,
    IdentityChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustState {
    Unverified,
    Verified,
    BlockedIdentityChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryState {
    Draft,
    Encrypted,
    Queued,
    SmtpInFlight,
    SmtpAccepted,
    FetchedByPeer,
    DecryptedByPeer,
    ReadByPeer,
    FailedRetryable,
    FailedTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderName {
    Inbox,
    Archive,
    Spam,
}

impl Display for FolderName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inbox => write!(f, "INBOX"),
            Self::Archive => write!(f, "Archive"),
            Self::Spam => write!(f, "Spam"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolHeader {
    pub protocol_version: u16,
    pub message_type: MessageType,
    pub cipher_suite_id: String,
    pub session_id: Uuid,
    pub sender_device_id: Uuid,
    pub sender_fingerprint: String,
    pub timestamp: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatchetHeaderData {
    pub ratchet_public_key: Vec<u8>,
    pub message_number: u32,
    pub previous_chain_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentManifest {
    pub attachment_id: Uuid,
    pub mime_type: String,
    pub file_extension: Option<String>,
    pub plaintext_len: u64,
    pub padded_len: u64,
    pub chunk_size: u32,
    pub chunk_count: u32,
    pub sha256: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayKey {
    pub session_id: Uuid,
    pub sender_device_id: Uuid,
    pub ratchet_public_key: Vec<u8>,
    pub message_number: u32,
    pub ciphertext_sha256: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactIdentity {
    pub email: String,
    pub device_id: Uuid,
    pub identity_fingerprint: String,
    pub trust_state: TrustState,
    pub verified_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxCursor {
    pub folder: FolderName,
    pub last_uid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxItem {
    pub message_id: Uuid,
    pub session_id: Uuid,
    pub recipient_email: String,
    pub delivery_state: DeliveryState,
    pub attempt_count: u32,
    pub next_attempt_at: OffsetDateTime,
    pub payload_b64: String,
    pub transport_subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMessage {
    pub message_id: Uuid,
    pub session_id: Uuid,
    pub sender_email: String,
    pub sender_device_id: Uuid,
    pub body: String,
    pub created_at: OffsetDateTime,
    pub attachments: Vec<AttachmentManifest>,
    pub delivery_state: DeliveryState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryReceiptPayload {
    pub original_message_id: Uuid,
    pub state: DeliveryState,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitePayload {
    pub invite_id: Uuid,
    pub inviter_email: String,
    pub inviter_device_id: Uuid,
    pub issued_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub opaque_token_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeMetadata {
    pub transport_message_id: String,
    pub transport_subject: String,
    pub mailbox_uid: u32,
    pub folder: FolderName,
}

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported cipher suite {0}")]
    UnsupportedCipherSuite(String),
}

pub fn ensure_v1(protocol_version: u16, cipher_suite_id: &str) -> Result<(), SpecError> {
    if protocol_version != PROTOCOL_VERSION_V1 {
        return Err(SpecError::UnsupportedVersion(protocol_version));
    }
    if cipher_suite_id != CIPHER_SUITE_V1 {
        return Err(SpecError::UnsupportedCipherSuite(
            cipher_suite_id.to_owned(),
        ));
    }
    Ok(())
}
