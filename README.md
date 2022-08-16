# verify-bot

A bot to interact with the global verification API to verify users in University of Southampton related discord servers.

# Commands

## verify

Verifies you or tells you how to verify.

## re-verify

Will batch verify everyone on the server, for this command to work you must be an admin.

# Getting Started

To get started create a .env file at the project root with the following details:

```bash
DISCORD_TOKEN="Your Discord Token"
# This is only necessary if you want to only update the guild commands and not the global ones.
TEST_GUILD_ID="Guild Id"
API_KEY="The API key for the Soton verify service"
API_URL="The URL to that API"
DISPLAY_URL="The URL to display to users for verification"
```