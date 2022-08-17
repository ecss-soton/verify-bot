use anyhow::{anyhow, Context as ContextTrait, Result};
use std::collections::HashMap;
use std::time::Duration;
use std::{env, mem};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use once_cell::sync::OnceCell;
use serenity::async_trait;
use serenity::builder::CreateApplicationCommands;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::Interaction;
use serenity::model::gateway::Ready;
use serenity::model::guild::Member;
use serenity::model::id::GuildId;
use serenity::model::prelude::command::Command;
use serenity::model::prelude::UserId;
use serenity::model::Permissions;
use serenity::prelude::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::commands::{batch_verify, re_verify, verify};

mod commands;

fn create_commands(commands: &mut CreateApplicationCommands) -> &mut CreateApplicationCommands {
    commands
        .create_application_command(|command| {
            command
                .name("verify")
                .description("Verifies you and gives you a nice role!")
                .dm_permission(false)
        })
        .create_application_command(|command| {
            command
                .name("re-verify")
                .description("Re-verifies everyone on the server.")
                .dm_permission(false)
                .default_member_permissions(Permissions::MANAGE_ROLES)
        })
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn guild_member_addition(&self, ctx: Context, new_member: Member) {
        let (guild_id, user_id) = (new_member.guild_id, new_member.user.id);
        batch_verify(&ctx, user_id, guild_id).await;
        TASK_LIST
            .get()
            .expect("OnceCell should be instantiated")
            .send((user_id, guild_id))
            .ok();
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let commands = if let Some(guild_id) = env::var("TEST_GUILD_ID")
            .ok()
            .map(|f| GuildId(f.parse().expect("TEST_GUILD_ID must be an integer.")))
        {
            GuildId::set_application_commands(&guild_id, &ctx.http, create_commands).await
        } else {
            Command::set_global_application_commands(&ctx, create_commands).await
        };

        match commands.context("Unable to create commands.") {
            Ok(commands) => {
                println!("I now have the following slash commands: {commands:#?}")
            }
            Err(e) => {
                eprintln!("{e:?}")
            }
        }

        let (send, recv) = unbounded_channel();
        TASK_LIST.set(send).expect("OnceCell has not yet been set.");
        tokio::task::spawn(check_for_verify(ctx, recv));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            if let Err(why) = dispatch_commands(&ctx, command).await {
                eprintln!("{why:?}");
            }
        }
    }
}

async fn dispatch_commands(ctx: &Context, command: ApplicationCommandInteraction) -> Result<()> {
    match command.data.name.as_str() {
        "verify" => verify(ctx, command).await.context("Ran verify command."),
        "re-verify" => re_verify(ctx, command)
            .await
            .context("Ran re-verify command."),
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
                task_list_a.push(batch_verify(ctx, new_task.0, new_task.1))
            }
            tries.insert(new_task.0, TRIES);
        }

        while let Some(task) = task_list_a.next().await {
            match tries.get_mut(&task.user_id).map(|t| {
                *t -= 1;
                *t
            }) {
                Some(0) | None => {
                    tries.remove(&task.user_id);
                }
                Some(_) => {
                    if !task.verified {
                        task_list_b.push(batch_verify(ctx, task.user_id, task.guild_id));
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
    dotenv::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN environment var has not been set.");

    let mut client = Client::builder(token, GatewayIntents::GUILD_MEMBERS)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }
}
