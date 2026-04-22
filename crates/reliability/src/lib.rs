use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use mimicrypt_spec_types::{DeliveryReceiptPayload, DeliveryState, OutboxItem};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_seconds: i64,
    pub max_delay_seconds: i64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            base_delay_seconds: 15,
            max_delay_seconds: 60 * 30,
        }
    }
}

pub fn next_attempt(
    now: OffsetDateTime,
    policy: &RetryPolicy,
    attempt_count: u32,
) -> OffsetDateTime {
    let exp = 2_i64.saturating_pow(attempt_count.min(10));
    let delay = (policy.base_delay_seconds * exp).min(policy.max_delay_seconds);
    now + Duration::seconds(delay)
}

pub fn transition_outbox(
    mut item: OutboxItem,
    state: DeliveryState,
    policy: &RetryPolicy,
    now: OffsetDateTime,
) -> OutboxItem {
    item.delivery_state = state;
    match state {
        DeliveryState::FailedRetryable => {
            item.attempt_count += 1;
            item.next_attempt_at = next_attempt(now, policy, item.attempt_count);
        }
        DeliveryState::SmtpAccepted
        | DeliveryState::FetchedByPeer
        | DeliveryState::DecryptedByPeer
        | DeliveryState::ReadByPeer => {
            item.next_attempt_at = now;
        }
        _ => {}
    }
    item
}

pub fn should_retry(item: &OutboxItem, policy: &RetryPolicy, now: OffsetDateTime) -> bool {
    item.delivery_state == DeliveryState::FailedRetryable
        && item.attempt_count < policy.max_attempts
        && item.next_attempt_at <= now
}

pub fn dedupe_key(session_id: Uuid, message_id: Uuid) -> String {
    format!("{session_id}:{message_id}")
}

pub fn delivery_receipt(
    original_message_id: Uuid,
    state: DeliveryState,
    now: OffsetDateTime,
) -> DeliveryReceiptPayload {
    DeliveryReceiptPayload {
        original_message_id,
        state,
        observed_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimicrypt_spec_types::DeliveryState;

    #[test]
    fn retry_backoff_moves_time_forward() {
        let now = OffsetDateTime::now_utc();
        let policy = RetryPolicy::default();
        let item = OutboxItem {
            message_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            recipient_email: "bob@example.com".into(),
            delivery_state: DeliveryState::Queued,
            attempt_count: 0,
            next_attempt_at: now,
            payload_b64: "payload".into(),
            transport_subject: "hello".into(),
        };
        let failed = transition_outbox(item, DeliveryState::FailedRetryable, &policy, now);
        assert!(failed.next_attempt_at > now);
        assert!(should_retry(&failed, &policy, failed.next_attempt_at));
    }
}
