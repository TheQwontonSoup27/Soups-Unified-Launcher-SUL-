# Setting up SUL on Windows

Windows 10/11 already ships WebView2, which Tauri uses, so no extra webview install is needed.

1. Install **Rust** from https://rustup.rs (choose the default MSVC toolchain).
2. Install **Visual Studio Build Tools** with the "Desktop development with C++" workload.
3. Install **Node.js 22 LTS**.
4. In the repo root, install the JavaScript dependencies once:
   ```
   npm install
   ```
5. Run the core tests (no UI needed):
   ```
   cargo test -p sul-core
   ```
6. Check and test the UI:
   ```
   npm run ui:typecheck
   npm test --workspace ui
   npm run ui:build
   ```
7. Run the desktop app (icons are already in `src-tauri/icons`):
   ```
   npm run dev
   ```
   Optional, for Discord presence: set `SUL_DISCORD_CLIENT_ID` first (see `docs/discord.md`).

## Status of these steps
Step 5 has been run by the owner on Windows (before Step 1.5: 78 tests passed and 38 games mapped by the registry scanner).
Steps 4, 6 and 7 have **not** been run on Windows by the author; record the first verified run in `PROJECT_STATUS.md`.
`cargo test --workspace` also builds the Tauri shell, which needs WebView2 (present on current Windows).
