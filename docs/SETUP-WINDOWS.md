# Setting up SUL on Windows

Windows 10/11 already ships WebView2, which Tauri uses, so no extra webview install is needed.

1. Install **Rust** from https://rustup.rs (choose the default MSVC toolchain).
2. Install **Visual Studio Build Tools** with the "Desktop development with C++" workload.
3. Install **Node.js 22 LTS**.
4. In the repo root, run the core tests (no UI needed):
   ```
   cargo test --workspace
   ```
5. Build and test the UI:
   ```
   cd ui
   npm install
   npm test
   npm run build
   ```
6. Run the desktop app (first time only, generate icons from any square PNG, 1024x1024 recommended):
   ```
   cd ui
   npx tauri icon path\to\logo.png --output ..\src-tauri\icons
   npx tauri dev
   ```

## Status of these steps
Steps 4-6 have **not** been run by the author yet; the first verified run should be recorded in
`PROJECT_STATUS.md`. The browser-only UI (`ui/dist/index.html`) was built and tested in a Linux container.
