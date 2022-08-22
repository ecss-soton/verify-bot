use cached::proc_macro::cached;

use once_cell::sync::Lazy;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::{Client, ClientBuilder, Url};
use serde::{Deserialize, Serialize};

use anyhow::Result;
use anyhow::{anyhow, ensure};
use serenity::model::prelude::{GuildId, RoleId, UserId};
use serenity::model::Timestamp;

use log::warn;
use serenity::utils::Colour;
use std::env;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Error, Debug)]
enum ParamError {
    #[error("Incorrect Authorization header.")]
    IncorrectAuth,
    #[error("Incorrect parameters: {0:?}.")]
    InvalidParams(reqwest::Result<String>),
}

static CLIENT: Lazy<Client> = Lazy::new(|| {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(
            &*env::var("API_KEY").expect("API_KEY environment var has not been set."),
        )
        .unwrap(),
    );
    ClientBuilder::new()
        .default_headers(headers)
        .build()
        .unwrap()
});

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
struct Verified {
    pub verified: bool,
    #[serde(rename = "roleId")]
    pub role_id: RoleId,
    #[serde(rename = "sotonLinkedDate")]
    pub soton_linked_date: Timestamp,
    #[serde(rename = "discordLinkedDate")]
    pub discord_linked_date: Timestamp,
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug)]
struct VerifiedParams {
    #[serde(rename = "userId")]
    pub user_id: UserId,
    #[serde(rename = "guildId")]
    pub guild_id: GuildId,
}

#[cached(key = "UserId", result = true, convert = r##"{user_id}"##)]
pub async fn is_verified(user_id: UserId, guild_id: GuildId) -> Result<()> {
    let elapsed = Instant::now();
    let params = VerifiedParams { user_id, guild_id };
    let resp = CLIENT
        .get(
            &*(env::var("API_URL").expect("API_URL environment var has not been set.")
                + "/api/v1/verified"),
        )
        .json(&params)
        .send()
        .await?;
    let elapsed = elapsed.elapsed();
    if elapsed > Duration::from_millis(400) {
        warn!("Took {elapsed:?} to check if user is verified.");
    }

    match resp.status().into() {
        200 => {
            let resp = resp.json::<Verified>().await?;
            ensure!(resp.verified, "User ({params:?}) is not verified.");
            {
                let mut cache = GET_ROLE_ID.lock().await;
                cache.cache_set(guild_id, resp.role_id);
            }
            Ok(())
        }
        404 => Err(anyhow!(
            "User ({params:?}) does not exist or is not verified."
        )),
        401 => Err(ParamError::IncorrectAuth.into()),
        400 => Err(ParamError::InvalidParams(resp.text().await).into()),
        _ => Err(anyhow!("Unknown error: {resp:?}")),
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Guild {
    #[serde(rename = "roleId")]
    pub role_id: RoleId,
    pub approved: bool,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
struct GuildParams {
    #[serde(rename = "guildId")]
    pub guild_id: GuildId,
}

#[cached(result = true)]
pub async fn get_role_id(guild_id: GuildId) -> Result<RoleId> {
    let elapsed = Instant::now();
    let resp = CLIENT
        .get(
            env::var("API_URL").expect("API_URL environment var has not been set.")
                + &*format!("/api/v1/guild/{guild_id}"),
        )
        .json(&GuildParams { guild_id })
        .send()
        .await?;

    let elapsed = elapsed.elapsed();
    if elapsed > Duration::from_millis(400) {
        warn!("Took {elapsed:?} to get role id.");
    }
    match resp.status().into() {
        200 => {
            let resp = resp.json::<Guild>().await?;
            Ok(resp.role_id)
        }
        404 => Err(anyhow!("Guild with id of {guild_id} does not exist.")),
        401 => Err(ParamError::IncorrectAuth.into()),
        400 => Err(ParamError::InvalidParams(resp.text().await).into()),
        _ => Err(anyhow!("Unknown error: {resp:?}")),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegisterParams {
    #[serde(rename = "guildId")]
    pub guild_id: GuildId,
    pub name: String,
    pub icon: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "ownerId")]
    pub owner_id: UserId,
    #[serde(rename = "susuLink")]
    pub susu_link: Option<Url>,
    #[serde(rename = "inviteLink")]
    pub invite_link: Url,
    #[serde(rename = "roleId")]
    pub role_id: RoleId,
    #[serde(rename = "roleName")]
    pub role_name: String,
    #[serde(rename = "roleColour")]
    pub role_colour: Colour,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Register {
    pub registered: bool,
    pub approved: bool,
}

pub async fn register_guild(info: RegisterParams) -> Result<Register> {
    let elapsed = Instant::now();
    let resp = CLIENT
        .post(
            env::var("API_URL").expect("API_URL environment var has not been set.")
                + "/api/v1/guild/register",
        )
        .json(&info)
        .send()
        .await?;
    let elapsed = elapsed.elapsed();
    if elapsed > Duration::from_millis(400) {
        warn!("Took {elapsed:?} to register guild.");
    }

    match resp.status().into() {
        200 => Ok(resp.json::<Register>().await?),
        409 => Err(anyhow!(
            "Guild with id of {} has already been registered.",
            info.guild_id
        )),
        401 => Err(ParamError::IncorrectAuth.into()),
        400 => Err(ParamError::InvalidParams(resp.text().await).into()),
        _ => Err(anyhow!("Unknown error: {resp:?}")),
    }
}
