use anyhow::{anyhow, Context as ContextTrait, Result};
use std::collections::HashMap;
use std::time::Duration;
use std::{env, mem};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use log::{info, warn};
use once_cell::sync::OnceCell;
use serenity::all::{Command, CommandInteraction, CommandOptionType, CreateCommand, Interaction};
use serenity::async_trait;
use serenity::builder::CreateCommandOption;
use serenity::model::gateway::Ready;
use serenity::model::guild::Member;
use serenity::model::id::GuildId;
use serenity::model::prelude::UserId;
use serenity::model::Permissions;
use serenity::prelude::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::commands::{setup, silent_verify, verify, verify_all};

mod commands;

fn create_commands() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("verify")
            .description("Verifies you and gives you a nice role!")
            .dm_permission(false),
        CreateCommand::new("verify-all")
            .description("Verifies everyone on the server.")
            .dm_permission(false)
            .default_member_permissions(Permissions::MANAGE_ROLES),
        CreateCommand::new("setup")
            .description("Sets your server up so that users can be verified.")
            .dm_permission(false)
            .default_member_permissions(Permissions::ADMINISTRATOR)
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::Role,
                    "role",
                    "The role you will be using to mark people as verified.",
                )
                .required(true),
            ),
    ]
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn guild_member_addition(&self, ctx: Context, new_member: Member) {
        let (guild_id, user_id) = (new_member.guild_id, new_member.user.id);
        silent_verify(&ctx, user_id, guild_id).await;
        TASK_LIST
            .get()
            .expect("OnceCell should be instantiated")
            .send((user_id, guild_id))
            .ok();
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        let commands = if let Some(guild_id) = env::var("TEST_GUILD_ID")
            .ok()
            .map(|f| GuildId::new(f.parse().expect("TEST_GUILD_ID must be an integer")))
        {
            GuildId::set_commands(guild_id, &ctx.http, create_commands()).await
        } else {
            Command::set_global_commands(&ctx, create_commands()).await
        };

        match commands.context("Unable to create commands.") {
            Ok(commands) => {
                info!("I now have the following slash commands: {commands:#?}")
            }
            Err(e) => {
                warn!("{e:?}")
            }
        }

        let (send, recv) = unbounded_channel();
        TASK_LIST.set(send).expect("OnceCell has not yet been set");
        tokio::task::spawn(check_for_verify(ctx, recv));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let guild = command.guild_id.unwrap();
            let user = command.user.id;
            if let Err(why) = dispatch_commands(&ctx, command).await {
                warn!("Command failure in guild with id {guild} from user with id {user}: {why:?}");
            }
        }
    }
}

async fn dispatch_commands(ctx: &Context, command: CommandInteraction) -> Result<()> {
    match command.data.name.as_str() {
        "verify" => verify(ctx, command)
            .await
            .context("Failed to run verify command."),
        "verify-all" => verify_all(ctx, command)
            .await
            .context("Ran verify-all command."),
        "setup" => setup(ctx, command)
            .await
            .context("Failed to run setup command"),
        "setup-modal" => Ok(()),
        command => Err(anyhow!("{command} command is not implemented.")),
    }
}

static TASK_LIST: OnceCell<UnboundedSender<(UserId, GuildId)>> = OnceCell::new();

async fn check_for_verify(ctx: Context, mut rec: UnboundedReceiver<(UserId, GuildId)>) -> ! {
    let ctx = &ctx;
    let mut tries = HashMap::new();
    let mut task_list_a = FuturesUnordered::new();
    let mut task_list_b = FuturesUnordered::new();
    const TRIES: i32 = 60;
    const TIMEOUT: Duration = Duration::from_secs(3);
    loop {
        while let Ok(new_task) = rec.try_recv() {
            if let Some(0) | None = tries.get(&new_task.0) {
                // Only add a task if one doesn't already exist.
                task_list_a.push(silent_verify(ctx, new_task.0, new_task.1))
            }
            tries.insert(new_task.0, TRIES);
        }

        while let Some(task) = task_list_a.next().await {
            let new_tries = tries.get_mut(&task.user_id).map(|t| {
                *t -= 1;
                *t
            });
            match new_tries {
                Some(0) | None => {
                    tries.remove(&task.user_id);
                }
                Some(_) => {
                    if !task.verified {
                        task_list_b.push(silent_verify(ctx, task.user_id, task.guild_id));
                    }
                }
            }
        }
        mem::swap(&mut task_list_a, &mut task_list_b);
        // b will now drained and a will contain the scheduled futures
        tokio::time::sleep(TIMEOUT).await;
    }
}

#[tokio::main]
async fn main() {
    let config_str = include_str!("./../log4rs.yml");
    let config = serde_yaml::from_str(config_str).unwrap();
    log4rs::init_raw_config(config).unwrap();

    dotenv::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN environment var has not been set");

    let mut client = Client::builder(token, GatewayIntents::GUILD_MEMBERS)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    info!("Client successfully created!");

    if let Err(why) = client.start().await {
        warn!("Client error: {:?}", why);
    }
}
