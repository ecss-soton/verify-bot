# University of Southampton verification Discord bot

A bot to interact with the [global verification API](https://github.com/ecss-soton/verify) to verify users in University
of Southampton related discord servers.

## How to set up the bot

1) [Invite](https://society.ecs.soton.ac.uk/verify-invite) the bot
2) Run /setup specifying your verified role (you can learn more about creating a verified role [here](setup.md))
3) Enjoy easy verification of your members!

### How it all works

With this service users can verify themselves once on the website and then never have to verify again! We store who is
verified and who isn't in our database and then whenever you run /verify or join a server we will give you that server's
verified role!

## Commands

### /verify

Verifies you or tells you how to verify.

### /verify-all

Will batch verify everyone on the server. **Admin only**

### /setup

Sets up the bot. **Admin only**

## Run Locally

Make sure you have [rust installed](https://www.rust-lang.org/tools/install). You can check this with `cargo -V`

Clone the project

```bash
  git clone https://github.com/ecss-soton/verify-bot.git
```

Go to the project directory

```bash
  cd verify-bot
```

Configure the environment variables. See [Environment Variables](#Environment-Variables)

Start the bot

```bash
  cargo run --release
```

### Environment Variables

Create a .env file at the project root and fill it with the following variables

A list of these can also be seen in [.env.example](./.env.example)

```bash
DISCORD_TOKEN="Your Discord Token"
# This is only necessary if you want to only update the guild commands and not the global ones.
TEST_GUILD_ID="Guild Id"
# Contact the ECSS web officer to get access to an API Key
API_KEY="The API key for the Soton verify service"
API_URL="The URL to that API"
DISPLAY_URL="The URL to display to users for verification"
```

### Docker image

TODO Add docker image link with dockerfile