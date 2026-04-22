use anyhow::{anyhow, Context, Result};
use imap::Session;
use lettre::{
    message::{header::ContentType, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use native_tls::TlsConnector;
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use time::Duration;

use mimicrypt_spec_types::{EnvelopeMetadata, FolderName};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailAccount {
    pub email: String,
    pub display_name: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
    pub inbox_folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollConfig {
    pub idle_supported: bool,
    pub foreground_interval_seconds: i64,
    pub background_interval_seconds: i64,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            idle_supported: true,
            foreground_interval_seconds: 30,
            background_interval_seconds: 300,
        }
    }
}

pub fn send_ciphertext(
    account: &MailAccount,
    recipient: &str,
    subject: &str,
    payload_b64: &str,
) -> Result<()> {
    let email = Message::builder()
        .from(format!("{} <{}>", account.display_name, account.email).parse()?)
        .to(recipient.parse()?)
        .subject(subject)
        .singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(payload_b64.to_owned()),
        )?;

    let transport = SmtpTransport::relay(&account.smtp_host)?
        .port(account.smtp_port)
        .credentials(Credentials::new(
            account.username.clone(),
            account.password.clone(),
        ))
        .build();

    transport.send(&email).context("SMTP send failed")?;
    Ok(())
}

pub fn connect_imap(account: &MailAccount) -> Result<Session<native_tls::TlsStream<TcpStream>>> {
    let tls = TlsConnector::builder().build()?;
    let client = imap::connect(
        (account.imap_host.as_str(), account.imap_port),
        &account.imap_host,
        &tls,
    )
    .context("IMAP connect failed")?;
    let session = client
        .login(&account.username, &account.password)
        .map_err(|err| anyhow!("IMAP login failed: {}", err.0))?;
    Ok(session)
}

pub fn fetch_since_uid(
    session: &mut Session<native_tls::TlsStream<TcpStream>>,
    folder: &str,
    last_uid: u32,
) -> Result<Vec<(EnvelopeMetadata, String)>> {
    session.select(folder)?;
    let range = format!("{}:*", last_uid.saturating_add(1));
    let uids = session.uid_search(range)?;
    let mut out = Vec::new();

    for uid in uids.iter().copied() {
        let fetches = session.uid_fetch(uid.to_string(), "RFC822 UID")?;
        for fetch in fetches.iter() {
            let body = fetch.body().ok_or_else(|| anyhow!("missing RFC822 body"))?;
            let content = String::from_utf8_lossy(body).into_owned();
            out.push((
                EnvelopeMetadata {
                    transport_message_id: format!("uid-{uid}"),
                    transport_subject: String::new(),
                    mailbox_uid: uid,
                    folder: FolderName::Inbox,
                },
                content,
            ));
        }
    }

    Ok(out)
}

pub fn idle_or_poll_hint(config: &PollConfig, foreground: bool) -> Duration {
    if foreground {
        Duration::seconds(config.foreground_interval_seconds)
    } else {
        Duration::seconds(config.background_interval_seconds)
    }
}
