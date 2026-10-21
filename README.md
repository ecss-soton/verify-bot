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
# Must match LINK_TOKEN_SECRET in the verify backend. Used to sign /verify links.
LINK_TOKEN_SECRET="A long random string shared with the verify backend"
# Optional. How long a /verify link stays usable, in seconds. Defaults to 900 (15 minutes).
LINK_TOKEN_TTL_SECONDS="900"
```

### How verification works

Running `/verify` asks the bot for a short-lived, signed link. The link contains the
Discord account and guild, so when the user opens it the website only has to ask them
to sign in with Southampton (in that same browser) and then writes the link to the
database. No Discord OAuth correlation cookie is involved, so the flow works from the
Discord mobile app, an in-app browser, or a completely different browser.

After linking, the bot's poll of the verification API notices the link and gives the
user the verified role. If the poll has already timed out, running `/verify` again is
enough.

Running `/setup` registers the server, but it still has to be approved by the Soton
Verify admins before `/verify` will work.

### Docker image

TODO Add docker image link with dockerfile