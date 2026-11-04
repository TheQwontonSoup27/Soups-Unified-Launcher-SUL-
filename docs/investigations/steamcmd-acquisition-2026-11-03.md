# SteamCMD and acquiring builds (spec 1.2): a first look, one page

Date: 2026-11-03.

## The question

Spec 1.2 asks SUL to "investigate SteamCMD/depot/manifest workflows where applicable; respect account permissions, access
controls, and distribution terms", to acquire historical builds only "through authorized tools/access and where terms permit",
and never to store passwords or session secrets in plaintext.

## How far this goes

- One search, and **one page read** with the fetch tool: Valve's developer wiki page `https://developer.valvesoftware.com/wiki/SteamCMD`.
  The fetch tool gives back an answer written by a small summarising model, not the page itself; the quotes below come from that
  summary.
- **Nothing was downloaded, installed or run.** SUL has no code that starts SteamCMD.
- The search also listed a SteamDB post, "manifest request codes" (`https://steamdb.info/blog/manifest-request-codes`). It was **not read**.
  It may bear on downloading a specific manifest; this note says nothing about it.

## What the page said

| Point | What was found |
|---|---|
| What it is | "a command-line version of the Steam Client". Its "primary use is to install and update various dedicated servers available on Steam", and it "works with games that use the SteamPipe content system" |
| Login | "To download most game servers, you can login anonymously using login anonymous." "Some servers require you to login with a Steam Account"; a "No subscription" error means the title "either requires a login or that you have purchased the game". The page does not list which titles or depots need an owning account |
| Updating | `app_update <app_id> [-beta <betaname>] [-betapassword <password>] [validate]`. A beta branch is chosen with `-beta`; "some beta branches are protected by a password". The ordinary branch is named "public" |
| Older builds | **The page does not mention `download_depot`, manifest ids, or any way to fetch a specific older build** |
| Credentials | Examples put the account name and password on the command line (`steamcmd +login <username> <password>`) and in a script (`login <username> <password>`). The page gives no warning about that. Steam Guard needs a code the first time. It mentions files where SteamCMD stores login information without saying where or how they are protected. It advises a separate Steam account for servers |
| Licence or terms | **Not stated.** No licence, no terms of use, nothing about redistributing SteamCMD |

## What is not established

- Whether SteamCMD can download a game client (not a server) such as Helldivers 2, and under which account and ownership rules.
- Whether and how a specific older build or depot manifest can be fetched today, and what that needs (the page is silent; the
  SteamDB post was not read).
- Valve's or Steam's terms for downloading or keeping older builds, and for using SteamCMD from another program.
- How SteamCMD protects the login information it stores.

## What this means for SUL

- Acquisition stays **Not investigated**. One page is not an assessment, and no test exists.
- **Passwords:** the page's own examples pass a password on the command line. SUL's rule (decision 0003) is that it never stores or
  handles a Steam password. So SUL could not offer to "log in for you"; at most it could start SteamCMD and leave the login to the
  owner, and whether that is workable is unknown.
- **Terms:** nothing read here says an older build may be downloaded or kept. Until that is checked, SUL only imports an install the
  owner already has (`BuildRegistry::import_install`), which is what is built.
- Anything in this area needs the owner's decision before code, because it touches accounts, credentials and a third party's terms
  (spec section 7: do not silently finalise).
