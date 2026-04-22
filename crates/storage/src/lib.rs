use anyhow::{anyhow, Context, Result};
use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use uuid::Uuid;

use mimicrypt_bootstrap::ContactInviteResponse;
use mimicrypt_ratchet::SessionState;
use mimicrypt_reliability::dedupe_key;
use mimicrypt_spec_types::{ContactIdentity, DeliveryState, MailboxCursor, OutboxItem};

pub struct AppDatabase {
    conn: Connection,
}

impl AppDatabase {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS contacts (
              email TEXT PRIMARY KEY,
              json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
              session_id TEXT PRIMARY KEY,
              json BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cursors (
              folder TEXT PRIMARY KEY,
              last_uid INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outbox (
              message_id TEXT PRIMARY KEY,
              dedupe_key TEXT NOT NULL UNIQUE,
              json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS invite_responses (
              invite_id TEXT PRIMARY KEY,
              json TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn save_contact(&self, contact: &ContactIdentity) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO contacts (email, json) VALUES (?1, ?2)",
            params![contact.email, serde_json::to_string(contact)?],
        )?;
        Ok(())
    }

    pub fn contact(&self, email: &str) -> Result<Option<ContactIdentity>> {
        fetch_json(
            &self.conn,
            "SELECT json FROM contacts WHERE email = ?1",
            params![email],
        )
    }

    pub fn save_session(&self, session: &SessionState) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (session_id, json) VALUES (?1, ?2)",
            params![session.session_id.to_string(), session.export()?],
        )?;
        Ok(())
    }

    pub fn session(&self, session_id: Uuid) -> Result<Option<SessionState>> {
        let blob: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT json FROM sessions WHERE session_id = ?1",
                params![session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        blob.map(|bytes| SessionState::import(&bytes)).transpose()
    }

    pub fn save_cursor(&self, cursor: &MailboxCursor) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO cursors (folder, last_uid) VALUES (?1, ?2)",
            params![cursor.folder.to_string(), cursor.last_uid],
        )?;
        Ok(())
    }

    pub fn cursor(&self, folder: &str) -> Result<u32> {
        let uid = self
            .conn
            .query_row(
                "SELECT last_uid FROM cursors WHERE folder = ?1",
                params![folder],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(uid)
    }

    pub fn enqueue_outbox(&self, item: &OutboxItem) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO outbox (message_id, dedupe_key, json) VALUES (?1, ?2, ?3)",
            params![
                item.message_id.to_string(),
                dedupe_key(item.session_id, item.message_id),
                serde_json::to_string(item)?
            ],
        )?;
        Ok(())
    }

    pub fn pending_outbox(&self, now: OffsetDateTime) -> Result<Vec<OutboxItem>> {
        let mut stmt = self.conn.prepare("SELECT json FROM outbox")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut items = Vec::new();
        for row in rows {
            let item: OutboxItem = serde_json::from_str(&row?)?;
            if matches!(
                item.delivery_state,
                DeliveryState::Queued | DeliveryState::FailedRetryable
            ) && item.next_attempt_at <= now
            {
                items.push(item);
            }
        }
        Ok(items)
    }

    pub fn store_invite_response(&self, response: &ContactInviteResponse) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO invite_responses (invite_id, json) VALUES (?1, ?2)",
            params![
                response.invite_id.to_string(),
                serde_json::to_string(response)?
            ],
        )?;
        Ok(())
    }
}

pub fn store_secret(service: &str, account: &str, secret: &str) -> Result<()> {
    let entry = Entry::new(service, account)?;
    entry.set_password(secret)?;
    Ok(())
}

pub fn load_secret(service: &str, account: &str) -> Result<String> {
    let entry = Entry::new(service, account)?;
    entry.get_password().map_err(|err| anyhow!(err.to_string()))
}

fn fetch_json<T: DeserializeOwned>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Option<T>> {
    let raw: Option<String> = conn.query_row(sql, params, |row| row.get(0)).optional()?;
    raw.map(|json| serde_json::from_str(&json).context("failed to decode JSON"))
        .transpose()
}
