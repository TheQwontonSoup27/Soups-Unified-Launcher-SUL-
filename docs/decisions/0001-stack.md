# 0001 — Stack: Tauri 2 + Rust core + React/TypeScript UI, Windows first
Date: 2026-10-22 · Status: accepted by owner ("go" after plan)

- Rust `sul-core` holds all file-safety logic (manifests, hashing, protected roots, journal, logging).
- React UI with one self-registering folder/entry per module; esbuild bundler (no Vite needed).
- Windows first; core stays portable. License still undecided.
- Tauri shell is in `src-tauri/` and excluded from the Cargo workspace so core tests run without webview libs.
