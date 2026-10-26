# Discord Rich Presence

Status: **Experimental**. The IPC protocol code is unit-tested against recorded frames; it has not yet been run against a live Discord client.

- Uses Discord's documented local IPC (the same protocol the official discord-rpc library speaks). No Discord login, token or password is handled by SUL.
- **Off by default.** The switch (`RPC`) is in the top row of the sidebar, and Settings → Discord has the same switch plus options. Turning it off clears the status Discord is showing.
- Games whose capability status is Unsupported or Not investigated never publish.

## One-time setup (free, about 2 minutes)
1. Open https://discord.com/developers/applications and click **New Application**. The application name is what Discord shows as "Playing ...".
2. Copy the **Application ID** (a number, 17-19 digits). It is not a secret.
3. In PowerShell run (replace the number): `setx SUL_DISCORD_CLIENT_ID 123456789012345678`
4. Close SUL completely, start it again, keep the Discord desktop app running, then switch the toggle on.

If it fails, the message next to the switch says why (Discord not running, ID missing, ID rejected).

## Small corner image (optional)
Discord can show a small image in the corner of the activity's large image. It is chosen by an **asset key**:
1. In the developer portal open your application, then **Rich Presence → Art Assets**, and upload the image (for example the Team27 logo).
2. Note the asset's key (the name shown beside the upload).
3. In SUL open **Settings → Discord**, type the key into "Small image asset key" and press **Save**. Leave it empty for no corner image.

Limits: SUL cannot change how large Discord draws the corner image. As far as is known the presence interface has no size field, so a
"scale to 10 %" control cannot be offered. An unusable key (empty, control characters, over 256 characters) is ignored rather than sent.
Hover text for an image is only sent together with that image.
