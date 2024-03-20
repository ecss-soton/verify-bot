use std::env;
use std::time::Duration;

use anyhow::Result;
use anyhow::{anyhow, bail, ensure, Context as ContextTrait};
use cached::Cached;
use futures::stream::FuturesUnordered;
use futures::{join, Stream};
use log::warn;
use reqwest::Url;
use serenity::all::ActionRowComponent::InputText;
use serenity::all::{
    CommandDataOptionValue, CommandInteraction, CreateActionRow, CreateInteractionResponse,
    EditInteractionResponse, InputTextStyle, ModalInteraction,
};
use serenity::builder::{
    CreateInputText, CreateInteractionResponseFollowup, CreateInteractionResponseMessage,
    CreateModal,
};
use serenity::client::Context;
use serenity::collector::ModalInteractionCollector;
use serenity::futures::StreamExt;

use serenity::model::guild::{PartialGuild, Role};
use serenity::model::prelude::{GuildId, UserId};

use crate::commands::api::{register_guild, RegisterParams};
use crate::TASK_LIST;

mod api;

pub async fn verify(ctx: &Context, command: CommandInteraction) -> Result<()> {
    let guild_id = command.guild_id.unwrap();
    match api::get_role_id(guild_id)
        .await
        .context(concat!(file!(), ":", line!()))
    {
        Ok(role) => {
            if let Err(e) = api::is_verified(command.user.id, guild_id)
                .await
                .context(concat!(file!(), ":", line!()))
            {
                TASK_LIST
                    .get()
                    .expect("OnceCell should be instantiated")
                    .send((command.user.id, guild_id))
                    .ok();

                command
                        .create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(format!(
                            "Please verify yourself by going to {} and then run this command again.",
                            env::var("DISPLAY_URL")
                                .expect("DISPLAY_URL environment var has not been set")
                        )).ephemeral(true)))
                        .await
                        .context(concat!(file!(), ":", line!()))?;
                return Err(e);
            }

            match ctx
                .http
                .add_member_role(command.guild_id.unwrap(), command.user.id, role, None)
                .await
                .context(concat!(file!(), ":", line!()))
            {
                Ok(_) => {
                    command
                        .create_response(
                            ctx,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("You have now been verified!")
                                    .ephemeral(true),
                            ),
                        )
                        .await
                        .context(concat!(file!(), ":", line!()))?;
                    Ok(())
                }
                Err(e) => {
                    command
                        .create_response(ctx,  CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("I was unable to add the verified role, please make sure my role has higher permissions than the verified role.")))
                        .await.context(concat!(file!(), ":", line!()))?;
                    Err(e).context("Could not add verified role.")
                }
            }
        }
        Err(e) => {
            command
                .create_response(ctx,  CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("It looks like your server doesn't support this bot, please contact the admins so they can run /setup."))
                )
                .await.context(concat!(file!(), ":", line!()))?;
            Err(e)
        }
    }
}

/// Re-verifies an entire server (This only adds verified people), also invalidates guild role cache
pub async fn verify_all(ctx: &Context, command: CommandInteraction) -> Result<()> {
    let guild_id = command.guild_id.unwrap();
    {
        let mut cache = api::GET_ROLE_ID.lock().await;
        cache.cache_remove(&guild_id);
    }
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
                    unordered.push(silent_verify(ctx, user_id, guild_id));
                }
            }
            let mut num_verified = 0;
            while let Some(verified) = unordered.next().await {
                if verified.verified {
                    num_verified += 1;
                }
            }
            let members = match num_verified {
                1 => "member",
                _ => "members",
            };
            command
                .edit_response(ctx, EditInteractionResponse::new().content(format!("Successfully completed re-verifications. Was able to verify {num_verified} {members}.")))
                .await
                .context(concat!(file!(), ":", line!()))?;
            Ok(())
        }
        Err(e) => {
            command
                .edit_response(ctx, EditInteractionResponse::new().content("It looks like your server doesn't support this bot, please contact the admins."))
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
pub async fn silent_verify(ctx: &Context, user_id: UserId, guild_id: GuildId) -> IsVerified {
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
                    .add_member_role(guild_id, user_id, role, None)
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
    command: &CommandInteraction,
    partial_guild: &PartialGuild,
) -> Result<impl Stream<Item = ModalInteraction>> {
    command
        .create_response(
            ctx,
            CreateInteractionResponse::Modal(
                CreateModal::new("setup-modal", "Setup Your Server").components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "Server Name", "name")
                            .value(&*partial_guild.name),
                    ),
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "Invite Link", "invite")
                            .placeholder("https://discord.gg/9SYG22wR4V"),
                    ),
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "SUSU Link", "susu")
                            .required(false)
                            .placeholder("https://www.susu.org/groups/ecss"),
                    ),
                ]),
            ),
        )
        .await
        .context(concat!(file!(), ":", line!()))?;

    Ok(ModalInteractionCollector::new(ctx)
        .guild_id(command.guild_id.unwrap())
        .author_id(command.user.id)
        .timeout(Duration::from_secs(60 * 15))
        .filter(|modal| modal.data.custom_id == "setup-modal")
        .stream())
}

async fn get_verified_role(
    ctx: &Context,
    command: &CommandInteraction,
    partial_guild: &PartialGuild,
) -> Result<Role> {
    let guild_id = command.guild_id.unwrap();
    let current_user = ctx.cache.current_user().id;
    let bot = guild_id.member(ctx, current_user).await?;

    if bot.permissions.map_or(false, |p| !p.manage_roles()) {
        command
            .create_response(
                ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Please make sure I have the permissions to manage roles."),
                ),
            )
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

    match command.data.options.first().map(|o| o.value.clone()) {
        Some(CommandDataOptionValue::Role(r)) => {
            role = ctx
                .http
                .get_guild_roles(guild_id)
                .await?
                .into_iter()
                .find(|gr| gr.id == r)
                .unwrap();

            if let Some(position) = bot_position {
                if role.position > position {
                    command.create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().content("Unable to use the verified role, please make sure my role has higher permissions than the verified role.")))
                        .await.context(concat!(file!(), ":", line!()))?;

                    bail!(
                        "verified role {} ({}) has higher position than bot role.",
                        role.name,
                        role.id
                    )
                }
            }
            if role.id.get() == guild_id.get() {
                command.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("Unable to use the verified role, please stop trying to crash this bot by using @everyone.")))
                    .await.context(concat!(file!(), ":", line!()))?;
                bail!("verified role is @everyone.")
            }
        }
        _ => bail!("Unable to get option info."),
    }

    Ok(role)
}

pub async fn setup(ctx: &Context, command: CommandInteraction) -> Result<()> {
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

    match join!(
        modal_response(&command, verified, partial_guild),
        command.defer(ctx)
    ) {
        (Ok(c), _) => {
            command
                .create_followup(ctx, CreateInteractionResponseFollowup::new().content(c))
                .await
                .context(concat!(file!(), ":", line!()))?;
            {
                let mut cache = api::GET_ROLE_ID.lock().await;
                cache.cache_remove(&command.guild_id.unwrap());
            }
            Ok(())
        }
        (Err(e), _) => {
            command
                .create_followup(
                    ctx,
                    CreateInteractionResponseFollowup::new().content(format!("{e}")),
                )
                .await
                .context(concat!(file!(), ":", line!()))?;
            Err(e).context("Error when responding to modal.")
        }
    }
}

async fn modal_response(
    command: &ModalInteraction,
    verified: Role,
    partial_guild: PartialGuild,
) -> Result<&'static str> {
    let (mut name, mut susu, mut invite) = (None, None, None);
    for t in command
        .data
        .components
        .iter()
        .filter_map(|a| a.components.first())
    {
        match t {
            InputText(t) if t.custom_id == "name" => name = t.value.clone(),
            InputText(t) if t.custom_id == "susu" => susu = t.value.clone(),
            InputText(t) if t.custom_id == "invite" => invite = t.value.clone(),
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
        .map(|s| Url::parse(&s))
    {
        Some(Err(e)) => {
            return Err(e).context("Unable to parse susu link, please make sure it is a url.");
        }
        Some(Ok(l)) => Some(l),
        None => None,
    };
    let invite_link = Url::parse(&invite.ok_or_else(|| anyhow!("invite was not sent."))?)
        .context("Unable to parse invite link, please make sure it is a url.")?;

    let resp = register_guild(RegisterParams {
        guild_id: partial_guild.id,
        name,
        icon: partial_guild.icon.map(|i| i.to_string()),
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
