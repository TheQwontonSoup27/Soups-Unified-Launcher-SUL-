# 0001 — Stack: Tauri 2 + Rust core + React/TypeScript UI, Windows first
Date: 2026-10-22 · Status: accepted by owner ("go" after plan)

- Rust `sul-core` holds all file-safety logic (manifests, hashing, protected roots, journal, logging).
- React UI with one self-registering folder/entry per module; bundler: esbuild at the time; superseded by Vite in 0002.
- Windows first; core stays portable. License still undecided.
- Tauri shell is in `src-tauri/`. It was excluded from the Cargo workspace at the time; 0002 made it a member (use `cargo test -p sul-core` for core-only runs).
