#[macro_use]
extern crate dotenv_codegen;

use crate::commands::{batch_verify, re_verify, verify};
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::Interaction;

use serenity::model::gateway::Ready;

use serenity::model::guild::Member;

use serenity::model::id::GuildId;

use serenity::model::Permissions;
use serenity::prelude::*;
use serenity::{async_trait, Error};

mod commands;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn guild_member_addition(&self, ctx: Context, new_member: Member) {
        batch_verify(&ctx, new_member).await
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let guild_id = GuildId(
            dotenv!("TEST_GUILD_ID")
                .parse()
                .expect("TEST_GUILD_ID must be an integer"),
        );

        let commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
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
                        .description("Re-verifies everyone on the server, you must have the permissions to manage roles to use this command.")
                        .dm_permission(false)
                        .default_member_permissions(Permissions::MANAGE_ROLES)
                })
        })
        .await;

        println!("I now have the following slash commands: {:#?}", commands);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            if let Err(why) = dispatch_commands(&ctx, command).await {
                eprintln!("Cannot respond to slash command: {}", why);
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

#[tokio::main]
async fn main() {
    let token = dotenv!("DISCORD_TOKEN");

    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }
}
