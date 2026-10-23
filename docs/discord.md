# Discord Rich Presence

Status: **Experimental**. The IPC protocol code is unit-tested against recorded frames; it has not yet been run against a live Discord client.

- Uses Discord's documented local IPC (the same protocol the official discord-rpc library speaks). No Discord login, token or password is handled by SUL.
- **Off by default.** The switch is in the title bar. Turning it off clears the status Discord is showing.
- Games whose capability status is Unsupported or Not investigated never publish.

## One-time setup (free, about 2 minutes)
1. Open https://discord.com/developers/applications and click **New Application**. The application name is what Discord shows as "Playing ...".
2. Copy the **Application ID** (a number, 17-19 digits). It is not a secret.
3. In PowerShell run (replace the number): `setx SUL_DISCORD_CLIENT_ID 123456789012345678`
4. Close SUL completely, start it again, keep the Discord desktop app running, then switch the toggle on.

If it fails, the message next to the switch says why (Discord not running, ID missing, ID rejected).
