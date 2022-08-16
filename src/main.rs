use std::collections::HashMap;
use std::env;
use std::time::Duration;

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use once_cell::sync::OnceCell;
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
use serenity::{async_trait, Error};
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

        println!("I now have the following slash commands: {:#?}", commands);

        let (send, recv) = unbounded_channel();
        TASK_LIST.set(send).expect("OnceCell has not yet been set.");
        tokio::task::spawn(channel(ctx, recv));
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            if let Err(why) = dispatch_commands(&ctx, command).await {
                eprintln!("Cannot respond to slash command: {:?}", why);
            }
        }
    }
}

async fn dispatch_commands(
    ctx: &Context,
    command: ApplicationCommandInteraction,
) -> Result<(), Error> {
    match command.data.name.as_str() {
        "verify" => {
            verify(ctx, command).await?;
        }
        "re-verify" => {
            re_verify(ctx, command).await?;
        }
        command => eprintln!("{command} command is not implemented."),
    };
    Ok(())
}

static TASK_LIST: OnceCell<UnboundedSender<(UserId, GuildId)>> = OnceCell::new();

async fn channel(ctx: Context, mut rec: UnboundedReceiver<(UserId, GuildId)>) -> ! {
    let ctx = &ctx;
    let mut tries = HashMap::new();
    let mut scheduled_tasks: Vec<(UserId, GuildId)> = vec![];
    let mut task_list = FuturesUnordered::new();
    const TRIES: i32 = 20;
    const TIMEOUT: Duration = Duration::from_secs(15);
    loop {
        while let Ok(new_task) = rec.try_recv() {
            if let Some(0) | None = tries.get(&new_task.0) {
                // Only add a task if one doesn't already exist.
                task_list.push(batch_verify(ctx, new_task.0, new_task.1))
            }
            tries.insert(new_task.0, TRIES);
        }
        for task in scheduled_tasks.drain(..) {
            if let Some(0) | None = tries.get(&task.0) {
                task_list.push(batch_verify(ctx, task.0, task.1))
            }
            tries.insert(task.0, TRIES);
        }
        while let Some(task) = task_list.next().await {
            match tries.get_mut(&task.user_id).map(|t| {
                *t -= 1;
                *t
            }) {
                Some(0) | None => {}
                Some(_) => {
                    if !task.verified {
                        scheduled_tasks.push((task.user_id, task.guild_id));
                    }
                }
            }
        }
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
