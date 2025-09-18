use anyhow::{Context, Result};
use lettre::{
    message::{Mailbox, Message},
    transport::smtp::authentication::Credentials,
    transport::smtp::client::{Tls, TlsParameters},
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};
use std::{net::{ToSocketAddrs, SocketAddr}, time::Duration};

use crate::config::Config;

pub async fn send_email(cfg: &Config, plain_text: &str) -> Result<()> {
    let Some(recipients) = &cfg.emails else { return Ok(()); };
    if recipients.is_empty() { return Ok(()); }
    
    let Some(email_username) = &cfg.email_username else { return Ok(()); };
    let Some(email_app_password) = &cfg.email_app_password else { return Ok(()); };

    // Resolve first IPv4 for smtp.mail.me.com:587
    let ipv4 = ("smtp.mail.me.com", 587)
        .to_socket_addrs()
        .context("DNS lookup failed")?
        .find(|a| a.is_ipv4())
        .context("No IPv4 address found")?;

    let host_ip = match ipv4 {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        _ => unreachable!(),
    };

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host_ip)
        .port(587) // STARTTLS port
        .tls(Tls::Required(TlsParameters::new("smtp.mail.me.com".into())?))
        .credentials(Credentials::new(
            email_username.clone(),
            email_app_password.clone(),
        ))
        .timeout(Some(Duration::from_secs(20)))
        .build();

    for rcpt in recipients {
        let email = Message::builder()
            .from(email_username.parse::<Mailbox>()?)
            .to(rcpt.parse::<Mailbox>().context("Invalid recipient email")?)
            .subject("LFC news summary")
            .body(plain_text.to_owned())?;

        mailer.send(email).await?;
    }

    Ok(())
}
