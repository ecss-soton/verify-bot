use std::env;
use std::time::Duration;

use anyhow::Result;
use anyhow::{anyhow, bail, ensure, Context as ContextTrait};
use cached::Cached;
use futures::join;
use futures::stream::FuturesUnordered;
use log::warn;
use reqwest::Url;
use serenity::client::Context;
use serenity::collector::{ModalInteractionCollector, ModalInteractionCollectorBuilder};
use serenity::futures::StreamExt;
use serenity::model::application::component::ActionRowComponent::InputText;
use serenity::model::application::component::InputTextStyle;
use serenity::model::application::interaction::application_command::{
    ApplicationCommandInteraction, CommandDataOptionValue,
};
use serenity::model::application::interaction::InteractionResponseType;
use serenity::model::guild::{PartialGuild, Role};
use serenity::model::prelude::interaction::modal::ModalSubmitInteraction;
use serenity::model::prelude::{GuildId, UserId};

use crate::commands::api::{register_guild, RegisterParams};
use crate::TASK_LIST;

mod api;

pub async fn verify(ctx: &Context, command: ApplicationCommandInteraction) -> Result<()> {
    let guild_id = command.guild_id.unwrap();
    match api::is_verified(command.user.id, guild_id)
        .await
        .context(concat!(file!(), ":", line!()))
    {
        Ok(()) => match api::get_role_id(guild_id)
            .await
            .context(concat!(file!(), ":", line!()))
        {
            Ok(role) => {
                match ctx
                    .http
                    .add_member_role(command.guild_id.unwrap().0, command.user.id.0, role.0, None)
                    .await
                    .context(concat!(file!(), ":", line!()))
                {
                    Ok(_) => {
                        command
                            .create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|d| {
                                        d.ephemeral(true).content("You have now been verified!")
                                    })
                            })
                            .await
                            .context(concat!(file!(), ":", line!()))?;
                        Ok(())
                    }
                    Err(e) => {
                        command
                            .create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|d| {
                                        d.content("I was unable to add the verified role, please make sure my role has higher permissions than the verified role.")
                                    })
                            })
                            .await.context(concat!(file!(), ":", line!()))?;
                        Err(e).context("Could not add verified role.")
                    }
                }
            }
            Err(e) => {
                command
                    .create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|d| {
                                d.content("It looks like your server doesn't support this bot, please contact the admins.")
                            })
                    })
                    .await.context(concat!(file!(), ":", line!()))?;
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
                .await
                .context(concat!(file!(), ":", line!()))?;
            e
        }
    }
}

/// Re-verifies an entire server (This only adds verified people), also invalidates guild role cache
pub async fn re_verify(ctx: &Context, command: ApplicationCommandInteraction) -> Result<()> {
    let guild_id = command.guild_id.unwrap();
    let (defer, role_id) = join!(command.defer(ctx), api::get_role_id(guild_id));
    defer.context(concat!(file!(), ":", line!()))?;
    match role_id.context(concat!(file!(), ":", line!())) {
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
                    r.content("Successfully completed re-verifications.")
                })
                .await
                .context(concat!(file!(), ":", line!()))?;
            Ok(())
        }
        Err(e) => {
            command
                .edit_original_interaction_response(ctx, |r| {
                    r.content("It looks like your server doesn't support this bot, please contact the admins.")
                })
                .await.context(concat!(file!(), ":", line!()))?;
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
    match api::is_verified(user_id, guild_id)
        .await
        .context(concat!(file!(), ":", line!()))
        .context(format!(
            "Could not batch verify user with id {user_id} in the guild with id {guild_id}"
        )) {
        Ok(()) => {
            if let Ok(role) = api::get_role_id(guild_id).await {
                if let Err(e) = ctx
                    .http
                    .add_member_role(guild_id.0, user_id.0, role.0, None)
                    .await
                    .context(concat!(file!(), ":", line!()))
                {
                    warn!("Could not add verified role. {e:?}");
                }
                return IsVerified {
                    guild_id,
                    user_id,
                    verified: true,
                };
            }
        }
        Err(e) => {
            warn!("{e:?}");
        }
    }
    IsVerified {
        guild_id,
        user_id,
        verified: false,
    }
}

async fn create_modal(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    partial_guild: &PartialGuild,
) -> Result<ModalInteractionCollector> {
    command
        .create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::Modal)
                .interaction_response_data(|m| {
                    m.components(|c| {
                        c.create_action_row(|a| {
                            a.create_input_text(|t| {
                                t.required(true)
                                    .label("Server Name")
                                    .value(&*partial_guild.name)
                                    .style(InputTextStyle::Short)
                                    .custom_id("name")
                            })
                        })
                        .create_action_row(|a| {
                            a.create_input_text(|t| {
                                t.required(true)
                                    .label("Invite Link")
                                    .placeholder("https://discord.com/invite/9SYG22wR4V")
                                    .style(InputTextStyle::Short)
                                    .custom_id("invite")
                            })
                        })
                        .create_action_row(|a| {
                            a.create_input_text(|t| {
                                t.required(false)
                                    .label("SUSU Link")
                                    .placeholder("https://www.susu.org/groups/ecss")
                                    .style(InputTextStyle::Short)
                                    .custom_id("susu")
                            })
                        })
                    })
                    .custom_id("setup-modal")
                    .title("Setup Your Server")
                })
        })
        .await
        .context(concat!(file!(), ":", line!()))?;

    Ok(ModalInteractionCollectorBuilder::new(ctx)
        .guild_id(command.guild_id.unwrap())
        .author_id(command.user.id)
        .collect_limit(1)
        .timeout(Duration::from_secs(60 * 15))
        .filter(|modal| modal.data.custom_id == "setup-modal")
        .build())
}

async fn get_verified_role(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    partial_guild: &PartialGuild,
) -> Result<Role> {
    let guild_id = command.guild_id.unwrap();
    let (bot, permissions) = join!(
        guild_id.member(ctx, ctx.cache.current_user_id()),
        partial_guild.member_permissions(ctx, ctx.cache.current_user_id())
    );
    let bot = bot?;

    if permissions.map_or(false, |p| !p.manage_roles()) {
        command
            .create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|d| {
                        d.content("Please make sure I have the permissions to manage roles.")
                    })
            })
            .await
            .context(concat!(file!(), ":", line!()))?;
        bail!("Not given permission to manage roles.")
    }
    let role;
    let bot_position = bot
        .roles
        .iter()
        .filter_map(|r| partial_guild.roles.get(r).map(|r| r.position))
        .max();

    match command.data.options.get(0).and_then(|o| o.resolved.clone()) {
        Some(CommandDataOptionValue::Role(r)) => {
            role = r;
            if let Some(position) = bot_position {
                if role.position > position {
                    command.create_interaction_response(ctx,
                        |r| {
                            r.kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|d| {
                                    d.content("Unable to use the verified role, please make sure my role has higher permissions than the verified role.")
                                })
                        })
                        .await.context(concat!(file!(), ":", line!()))?;
                    bail!("verified role ({role}) has higher position than bot role.")
                }
            }
            if role.id.0 == guild_id.0 {
                command.create_interaction_response(ctx,
                    |r| {
                        r.kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|d| {
                                d.content("Unable to use the verified role, please stop trying to crash this bot by using @everyone.")
                            })
                    })
                    .await.context(concat!(file!(), ":", line!()))?;
                bail!("verified role ({role}) is @everyone.")
            }
        }
        _ => bail!("Unable to get option info."),
    }

    Ok(role)
}

pub async fn setup(ctx: &Context, command: ApplicationCommandInteraction) -> Result<()> {
    let partial_guild = command.guild_id.unwrap().to_partial_guild(ctx).await?;
    let verified = get_verified_role(ctx, &command, &partial_guild)
        .await
        .context(concat!(file!(), ":", line!()))
        .context("Tried getting verified role.")?;

    let command = create_modal(ctx, &command, &partial_guild)
        .await
        .context(concat!(file!(), ":", line!()))
        .context("creating modal")?
        .next()
        .await
        .ok_or_else(|| anyhow!("Did not receive response"))?;

    match modal_response(&command, verified, partial_guild).await {
        Ok(c) => {
            command
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|d| d.content(c))
                })
                .await
                .context(concat!(file!(), ":", line!()))?;
            Ok(())
        }
        Err(e) => {
            command
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|d| d.content(format!("{e}")))
                })
                .await
                .context(concat!(file!(), ":", line!()))?;
            Err(e).context("Error when responding to modal.")
        }
    }
}

async fn modal_response(
    command: &ModalSubmitInteraction,
    verified: Role,
    partial_guild: PartialGuild,
) -> Result<&'static str> {
    let (mut name, mut susu, mut invite) = (None, None, None);
    for t in command
        .data
        .components
        .iter()
        .filter_map(|a| a.components.get(0))
    {
        match t {
            InputText(t) if t.custom_id == "name" => name = Some(t.value.clone()),
            InputText(t) if t.custom_id == "susu" => susu = Some(t.value.clone()),
            InputText(t) if t.custom_id == "invite" => invite = Some(t.value.clone()),
            ar => {
                return Err(anyhow!(
                    "Received unrecognized id {ar:?} from modal component."
                ))
                .context("Error incorrect modal response.");
            }
        }
    }
    let name = name.ok_or_else(|| anyhow!("name was not sent."))?;
    let susu_link = match susu
        .filter(|s| !s.trim().is_empty())
        .map(|s| Url::parse(&*s))
    {
        Some(Err(e)) => {
            return Err(e).context("Unable to parse susu link, please make sure it is a url.");
        }
        Some(Ok(l)) => Some(l),
        None => None,
    };
    let invite_link = Url::parse(&*invite.ok_or_else(|| anyhow!("invite was not sent."))?)
        .context("Unable to parse invite link, please make sure it is a url.")?;

    let resp = register_guild(RegisterParams {
        guild_id: partial_guild.id,
        name,
        icon: partial_guild.icon,
        created_at: partial_guild.id.created_at(),
        owner_id: partial_guild.owner_id,
        susu_link,
        invite_link,
        role_id: verified.id,
        role_name: verified.name,
        role_colour: verified.colour,
    })
    .await
    .context(concat!(file!(), ":", line!()))
    .context("Could not register guild, are you sure you haven't already registered?")?;

    // bail if registered is not true
    ensure!(resp.registered, "Error guild info was not saved to the db");
    // If approved is true
    Ok(if resp.approved {
        "Successfully set the server up!"
    } else {
        "Successfully set the server up! Please contact the ECSS web officer to get your server approved."
    })
}
