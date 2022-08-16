use std::env;

use cached::proc_macro::cached;
use cached::Cached;
use futures::stream::FuturesUnordered;
use once_cell::sync::Lazy;
use reqwest::{header, Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use serenity::client::Context;
use serenity::futures::StreamExt;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::InteractionResponseType;
use serenity::model::prelude::{GuildId, RoleId, UserId};
use serenity::model::Timestamp;
use serenity::Error;

use crate::TASK_LIST;

static CLIENT: Lazy<Client> = Lazy::new(|| {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "Authorization",
        header::HeaderValue::from_str(
            &*env::var("API_KEY").expect("API_KEY environment var has not been set."),
        )
        .unwrap(),
    );
    ClientBuilder::new()
        .default_headers(headers)
        .build()
        .unwrap()
});

pub async fn verify(ctx: &Context, command: ApplicationCommandInteraction) -> Result<(), Error> {
    let guild_id = command.guild_id.unwrap();
    if is_verified(command.user.id, guild_id).await.is_some() {
        if let Some(role) = get_role_id(guild_id).await {
            ctx.http
                .add_member_role(command.guild_id.unwrap().0, command.user.id.0, role.0, None)
                .await?;

            command
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|d| {
                            d.ephemeral(true).content("You have now been verified!")
                        })
                })
                .await
        } else {
            command
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|d| {
                            d.content(concat!("It looks like your server doesn't support this bot, please contact the admins."))
                        })
                })
                .await
        }
    } else {
        TASK_LIST
            .get()
            .expect("OnceCell should be instantiated")
            .send((command.user.id, guild_id))
            .ok();
        command
            .create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|d| {
                        d.ephemeral(true).content(format!(
                            "Please verify yourself by going to {}",
                            env::var("DISPLAY_URL")
                                .expect("DISPLAY_URL environment var has not been set.")
                        ))
                    })
            })
            .await
    }
}

/// Re-verifies an entire server (This only adds verified people), also invalidates guild role cache
pub async fn re_verify(ctx: &Context, command: ApplicationCommandInteraction) -> Result<(), Error> {
    let guild_id = command.guild_id.unwrap();
    command.defer(ctx).await?;
    if let Some(role) = get_role_id(guild_id).await {
        let mut members = guild_id
            .members_iter(ctx)
            .filter_map(move |r| async { r.ok() })
            .boxed();
        let mut unordered = FuturesUnordered::new();
        while let Some(member) = members.next().await {
            // Filter all the members that have the verified role or are a bot.
            if !member.user.bot && !member.roles.iter().any(|r| r == &role) {
                let (guild_id, user_id) = (member.guild_id, member.user.id);
                unordered.push(batch_verify(ctx, user_id, guild_id));
            }
        }
        {
            let mut cache = GET_ROLE_ID.lock().await;
            cache.cache_remove(&guild_id);
        }
        while (unordered.next().await).is_some() {}
        command
            .edit_original_interaction_response(ctx, |r| {
                r.content(concat!("Successfully completed re-verifications."))
            })
            .await?;
    } else {
        command
            .edit_original_interaction_response(ctx, |r| {
                r.content(concat!("It looks like your server doesn't support this bot, please contact the admins."))
            })
            .await?;
    }
    Ok(())
}

#[derive(Copy, Clone, Debug)]
pub struct IsVerified {
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub verified: bool,
}

/// Verifies multiple users, any errors are just printed.
pub async fn batch_verify(ctx: &Context, user_id: UserId, guild_id: GuildId) -> IsVerified {
    if is_verified(user_id, guild_id).await.is_some() {
        if let Some(role) = get_role_id(guild_id).await {
            if let Err(e) = ctx
                .http
                .add_member_role(guild_id.0, user_id.0, role.0, None)
                .await
            {
                eprintln!("{e}");
            }
            return IsVerified {
                guild_id,
                user_id,
                verified: true,
            };
        }
    }
    IsVerified {
        guild_id,
        user_id,
        verified: false,
    }
}

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

#[derive(Copy, Clone, Serialize, Deserialize)]
struct VerifiedReq {
    #[serde(rename = "userId")]
    pub user_id: UserId,
    #[serde(rename = "guildId")]
    pub guild_id: GuildId,
}

#[cached(key = "UserId", option = true, convert = r##"{user_id}"##)]
async fn is_verified(user_id: UserId, guild_id: GuildId) -> Option<()> {
    let resp = CLIENT
        .get(
            &*(env::var("API_URL").expect("API_URL environment var has not been set.")
                + "/api/v1/verified"),
        )
        .json(&VerifiedReq { user_id, guild_id })
        .send()
        .await;

    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(verified) = resp.json::<Verified>().await {
                if verified.verified {
                    {
                        let mut cache = GET_ROLE_ID.lock().await;
                        cache.cache_set(guild_id, verified.role_id);
                    }
                    return Some(());
                }
            }
        }
    }
    None
}

/// This is non-exhaustive.
#[derive(Serialize, Deserialize, Copy, Clone)]
struct ServerInfo {
    #[serde(rename = "roleId")]
    pub role_id: RoleId,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
struct ServerReq {
    #[serde(rename = "guildId")]
    pub guild_id: GuildId,
}

#[cached(option = true)]
async fn get_role_id(guild_id: GuildId) -> Option<RoleId> {
    let resp = CLIENT
        .get(
            env::var("API_URL").expect("API_URL environment var has not been set.")
                + &*format!("/api/v1/guild/{guild_id}"),
        )
        .json(&ServerReq { guild_id })
        .send()
        .await;

    if let Ok(resp) = resp {
        if resp.status().is_success() {
            return Some(resp.json::<ServerInfo>().await.ok()?.role_id);
        }
    }
    None
}
