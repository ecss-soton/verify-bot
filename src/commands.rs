use std::env;

use anyhow::Context as ContextTrait;
use anyhow::Result;
use cached::Cached;
use futures::stream::FuturesUnordered;
use serenity::client::Context;
use serenity::futures::StreamExt;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::InteractionResponseType;
use serenity::model::prelude::{GuildId, UserId};

use crate::TASK_LIST;

mod api;

pub async fn verify(ctx: &Context, command: ApplicationCommandInteraction) -> Result<()> {
    let guild_id = command.guild_id.unwrap();
    match api::is_verified(command.user.id, guild_id).await {
        Ok(()) => match api::get_role_id(guild_id).await {
            Ok(role) => {
                match ctx
                    .http
                    .add_member_role(command.guild_id.unwrap().0, command.user.id.0, role.0, None)
                    .await
                {
                    Ok(_) => {
                        command
                            .create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|d| {
                                        d.ephemeral(true).content("You have now been verified!")
                                    })
                            })
                            .await?;
                        Ok(())
                    }
                    Err(e) => {
                        command
                            .create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|d| {
                                        d.content(concat!("I was unable to add the verified role, please make sure my role has higher permissions than the verified role."))
                                    })
                            })
                            .await?;
                        Err(e).context("Could not add verified role.")
                    }
                }
            }
            Err(e) => {
                command
                    .create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|d| {
                                d.content(concat!("It looks like your server doesn't support this bot, please contact the admins."))
                            })
                    })
                    .await?;
                Err(e)
            }
        },
        e => {
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
                .await?;
            e
        }
    }
}

/// Re-verifies an entire server (This only adds verified people), also invalidates guild role cache
pub async fn re_verify(ctx: &Context, command: ApplicationCommandInteraction) -> Result<()> {
    let guild_id = command.guild_id.unwrap();
    command.defer(ctx).await?;
    match api::get_role_id(guild_id).await {
        Ok(role) => {
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
                let mut cache = api::GET_ROLE_ID.lock().await;
                cache.cache_remove(&guild_id);
            }
            while (unordered.next().await).is_some() {}
            command
                .edit_original_interaction_response(ctx, |r| {
                    r.content(concat!("Successfully completed re-verifications."))
                })
                .await?;
            Ok(())
        }
        Err(e) => {
            command
                .edit_original_interaction_response(ctx, |r| {
                    r.content(concat!("It looks like your server doesn't support this bot, please contact the admins."))
                })
                .await?;
            Err(e)
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct IsVerified {
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub verified: bool,
}

/// Verifies multiple users, any errors are just printed.
pub async fn batch_verify(ctx: &Context, user_id: UserId, guild_id: GuildId) -> IsVerified {
    match api::is_verified(user_id, guild_id).await.context(format!(
        "Could not batch verify user with id {user_id} in the guild with id {guild_id}"
    )) {
        Ok(()) => {
            if let Ok(role) = api::get_role_id(guild_id).await {
                if let Err(e) = ctx
                    .http
                    .add_member_role(guild_id.0, user_id.0, role.0, None)
                    .await
                {
                    eprintln!("Could not add verified role. {e:?}");
                }
                return IsVerified {
                    guild_id,
                    user_id,
                    verified: true,
                };
            }
        }
        Err(e) => {
            eprintln!("{e:?}");
        }
    }
    IsVerified {
        guild_id,
        user_id,
        verified: false,
    }
}
