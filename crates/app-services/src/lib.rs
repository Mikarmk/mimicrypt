use anyhow::{bail, Result};
use time::OffsetDateTime;
use uuid::Uuid;

use mimicrypt_bootstrap::{
    create_contact_invite, create_invite_response, tofu_pin, validate_contact_invite,
    verify_invite_response, ContactInvite, ContactInviteResponse,
};
use mimicrypt_envelope::open_message;
use mimicrypt_reliability::{delivery_receipt, transition_outbox, RetryPolicy};
use mimicrypt_spec_types::{AppMessage, ContactIdentity, DeliveryState, OutboxItem, TrustState};
use mimicrypt_storage::AppDatabase;

#[cfg(not(target_os = "android"))]
use mimicrypt_mail_transport::{send_ciphertext, MailAccount};

pub struct AppService {
    pub db: AppDatabase,
    pub retry_policy: RetryPolicy,
}

impl AppService {
    pub fn new(db: AppDatabase) -> Self {
        Self {
            db,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn issue_contact_invite(
        &self,
        inviter_email: &str,
        inviter_device_id: Uuid,
        inviter_keys: &mimicrypt_crypto::DeviceKeys,
    ) -> ContactInvite {
        create_contact_invite(
            inviter_email,
            inviter_device_id,
            inviter_keys,
            OffsetDateTime::now_utc(),
        )
    }

    pub fn accept_contact_invite(
        &self,
        invite: &ContactInvite,
        responder_email: &str,
        responder_device_id: Uuid,
        responder_keys: &mimicrypt_crypto::DeviceKeys,
    ) -> Result<ContactInviteResponse> {
        validate_contact_invite(invite, OffsetDateTime::now_utc())?;
        Ok(create_invite_response(
            invite,
            responder_email,
            responder_device_id,
            responder_keys,
        ))
    }

    pub fn register_contact_response(
        &self,
        invite: &ContactInvite,
        response: &ContactInviteResponse,
    ) -> Result<ContactIdentity> {
        let verified = verify_invite_response(invite, response, OffsetDateTime::now_utc())?;
        let decision = tofu_pin(self.db.contact(&verified.email)?.as_ref(), &response.bundle)?;
        let mut contact = verified;
        contact.trust_state = decision.trust_state;
        self.db.save_contact(&contact)?;
        self.db.store_invite_response(response)?;
        Ok(contact)
    }

    pub fn mark_contact_verified(&self, email: &str) -> Result<ContactIdentity> {
        let mut contact = self
            .db
            .contact(email)?
            .ok_or_else(|| anyhow::anyhow!("contact not found"))?;
        contact.trust_state = TrustState::Verified;
        contact.verified_at = Some(OffsetDateTime::now_utc());
        self.db.save_contact(&contact)?;
        Ok(contact)
    }

    pub fn queue_outbound_message(
        &self,
        contact: &ContactIdentity,
        message: AppMessage,
        payload_b64: String,
    ) -> Result<OutboxItem> {
        if contact.trust_state == TrustState::BlockedIdentityChange {
            bail!("identity changed; re-verification required");
        }
        let item = OutboxItem {
            message_id: message.message_id,
            session_id: message.session_id,
            recipient_email: contact.email.clone(),
            delivery_state: DeliveryState::Queued,
            attempt_count: 0,
            next_attempt_at: OffsetDateTime::now_utc(),
            payload_b64,
            transport_subject: "notes".into(),
        };
        self.db.enqueue_outbox(&item)?;
        Ok(item)
    }

    #[cfg(not(target_os = "android"))]
    pub fn send_outbox_item(&self, account: &MailAccount, item: &OutboxItem) -> Result<OutboxItem> {
        send_ciphertext(
            account,
            &item.recipient_email,
            &item.transport_subject,
            &item.payload_b64,
        )?;
        let updated = transition_outbox(
            item.clone(),
            DeliveryState::SmtpAccepted,
            &self.retry_policy,
            OffsetDateTime::now_utc(),
        );
        self.db.enqueue_outbox(&updated)?;
        Ok(updated)
    }

    #[cfg(target_os = "android")]
    pub fn send_outbox_item(&self, item: &OutboxItem) -> Result<OutboxItem> {
        Ok(item.clone())
    }

    pub fn open_ciphertext(
        &self,
        encoded: &str,
        sender_device_id: Uuid,
        session: &mut mimicrypt_ratchet::SessionState,
    ) -> Result<AppMessage> {
        open_message(encoded, sender_device_id, session)
    }

    pub fn make_delivery_receipt(
        &self,
        original_message_id: Uuid,
        state: DeliveryState,
    ) -> mimicrypt_spec_types::DeliveryReceiptPayload {
        delivery_receipt(original_message_id, state, OffsetDateTime::now_utc())
    }
}
