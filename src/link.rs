use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::Serialize;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// How long a `/verify` link remains usable. The web app only checks the `exp`
/// in the token, so keep this in sync with the message users see.
const DEFAULT_TTL_SECONDS: u64 = 15 * 60;

#[derive(Serialize)]
struct LinkTokenPayload<'a> {
    #[serde(rename = "discordId")]
    discord_id: String,
    #[serde(rename = "discordTag")]
    discord_tag: &'a str,
    #[serde(rename = "guildId")]
    guild_id: String,
    /// Unix timestamp in seconds.
    exp: u64,
    nonce: String,
}

pub fn ttl_seconds() -> u64 {
    env::var("LINK_TOKEN_TTL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TTL_SECONDS)
}

/// Builds the one-time verification URL handed to the user by `/verify`.
///
/// The token is a base64url JSON payload plus an HMAC-SHA256 signature. The
/// backend verifies it with the same `LINK_TOKEN_SECRET`, which is what removes
/// the need for a Discord OAuth round trip (and its state cookie).
pub fn verification_url(discord_id: u64, discord_tag: &str, guild_id: u64) -> Result<String> {
    let secret = env::var("LINK_TOKEN_SECRET")
        .context("LINK_TOKEN_SECRET environment var has not been set")?;
    if secret.is_empty() {
        return Err(anyhow!("LINK_TOKEN_SECRET environment var is empty"));
    }

    let display_url = env::var("DISPLAY_URL")
        .context("DISPLAY_URL environment var has not been set")?;
    if display_url.trim().is_empty() {
        return Err(anyhow!("DISPLAY_URL environment var is empty"));
    }

    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock is before the Unix epoch")?
        .as_secs()
        + ttl_seconds();

    let mut nonce_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);

    let payload = LinkTokenPayload {
        discord_id: discord_id.to_string(),
        discord_tag,
        guild_id: guild_id.to_string(),
        exp,
        nonce,
    };

    let json = serde_json::to_vec(&payload)
        .context("Could not serialise the verification token")?;
    let body = URL_SAFE_NO_PAD.encode(json);

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| anyhow!("LINK_TOKEN_SECRET has an invalid length"))?;
    mac.update(body.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    Ok(format!(
        "{}/link?t={}.{}",
        display_url.trim_end_matches('/'),
        body,
        signature
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_has_two_base64url_parts() {
        std::env::set_var("LINK_TOKEN_SECRET", "test-secret");
        std::env::set_var("DISPLAY_URL", "https://sotonverify.link/");

        let url = verification_url(123456789012345678, "alice#1234", 987654321098765432)
            .expect("token generation should succeed");
        let token = url
            .strip_prefix("https://sotonverify.link/link?t=")
            .expect("url should contain the token");

        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
        assert!(parts.iter().all(|part| part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')));
    }
}
