//! SUL build, gate, AST-guard, and benchmark runner (`cargo xtask <cmd>`).
//! Implements the R0 verification surface defined in Ch. 1 §10, §13.5.7, and SUL-LEDGER/*.md.
//!
//! Every gate is a module with a pure function over text (unit-tested with made-up input) and a `check()` that prints one
//! `[xtask] <gate>: ...` verdict and `PROBLEM <gate>: ...` lines. A gate that is only partly implemented says what it does
//! not check in its own output: a green gate never claims more than it looked at. `run_all` runs every gate of a command
//! even when an earlier one fails, so one run reports everything that is wrong.

mod boundaries;
mod common;
mod electron;
mod ipc;
mod network;
mod release;
mod tauri;
mod workspace;

use clap::{Parser, Subcommand};
use common::run_all;
use miette::{Context, IntoDiagnostic, Result};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "xtask", about = "SUL spec, gate, guard, and benchmark runner")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate the workspace Cargo.toml (Edition 2024, MSRV 1.85) and config.toml syntax (Ch. 1 §4; the compatibility rules are sul-compat)
    ConfigCheck {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
    },
    /// Structural guards from SUL-LEDGER/guards.md: workspace manifest, network surfaces, crate boundaries, Electron, Tauri capabilities, WebView runtime
    AcRegression,
    /// Verify zero Electron / Node.js in manifests, lockfiles, the Tauri bundle and source (guards.md, "Electron exclusion")
    ElectronAudit,
    /// Audit tauri.conf.json and capability files: csp, withGlobalTauri, wildcard scopes, shell execute (guards.md)
    TauriCapabilityAudit,
    /// Check the locked Tauri and plugin-updater versions against the ledger's floors (guards.md)
    WebviewRuntimeAudit,
    /// Run the golden-bytes test of sul-ipc-types: the wire format must not change without a reviewed golden-file update (Ch. 7 §2.3)
    IpcCheck,
    /// Check the release ladder and the Tier 1-core events in SUL-LEDGER/releases.md against gates.md, the workspace version and sul-ipc-types
    ReleaseCheck {
        #[arg(long, default_value = "R0")]
        release: String,
    },
    /// Run platform telemetry micro-benchmarks (Ch. 1 §10)
    BenchTelemetry,
    /// Dry-run VRAM, latency, and fallback simulation against baselines.md
    SimulateConfig {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
    },
    /// Emit dependency-aware task DAG for a release wave
    Plan {
        #[arg(long, default_value_t = 0)]
        wave: u8,
    },
    /// Run every gate for a target release class
    Pipeline {
        #[arg(long, default_value = "R0")]
        release: String,
    },
}

/// The shipped `config.toml`, if there is one. The repository has none yet, and the output says so.
fn config_toml(config: &Path) -> Result<()> {
    if !config.exists() {
        println!(
            "[xtask] config-toml: {} not found, so no shipped-config rule was checked (the Ch. 1 §4 compatibility rules are the sul-compat crate, R0 layer 3)",
            config.display()
        );
        return Ok(());
    }
    let raw = std::fs::read_to_string(config)
        .into_diagnostic()
        .with_context(|| format!("failed to read {}", config.display()))?;
    toml::from_str::<toml::Value>(&raw)
        .into_diagnostic()
        .with_context(|| format!("invalid TOML syntax in {}", config.display()))?;
    println!(
        "[xtask] config-toml: {} is valid TOML. NOT checked: the Ch. 1 §4 compatibility rules (sul-compat, R0 layer 3)",
        config.display()
    );
    Ok(())
}

fn say_what_ac_regression_covers() {
    println!(
        "[xtask] ac-regression: implemented guards: workspace manifest, network surfaces (Ch. 16 §16.15.14), crate boundaries (HTTP client, direct wasapi and process-memory rules are written; the crate-boundaries line below says which are armed), Electron exclusion, Tauri capability audit, WebView runtime audit (Tauri and updater floors)"
    );
    println!(
        "[xtask] ac-regression: NOT implemented yet: structural refusals (voice-clone, kernel-AC frame-gen and hook refusals: the crates and the shipped config.toml they guard do not exist), guards on a module or call order inside a crate (need syntax analysis), feature orthogonality and the [webview] config guards (need config.toml), the WebView2 runtime floor. No rule for \"Windows-only IPC\" was found in Ch. 1."
    );
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::ConfigCheck { config } => {
            run_all(&[
                ("workspace-manifest", &workspace::check),
                ("config-toml", &|| config_toml(&config)),
            ])?;
            println!("[xtask] config-check: OK (for the checks listed above only)");
        }
        Command::AcRegression => {
            say_what_ac_regression_covers();
            run_all(&[
                ("workspace-manifest", &workspace::check),
                ("network-surfaces", &network::check),
                ("crate-boundaries", &boundaries::check),
                ("electron-audit", &electron::check),
                ("tauri-capability-audit", &tauri::check_capabilities),
                ("webview-runtime-audit", &tauri::check_runtime),
            ])?;
            println!("[xtask] ac-regression: OK (for the guards listed above only)");
        }
        Command::ElectronAudit => electron::check()?,
        Command::TauriCapabilityAudit => tauri::check_capabilities()?,
        Command::WebviewRuntimeAudit => tauri::check_runtime()?,
        Command::IpcCheck => ipc::check()?,
        Command::ReleaseCheck { release } => {
            run_all(&[
                ("workspace-manifest", &workspace::check),
                ("electron-audit", &electron::check),
                ("release-ladder", &|| release::check(&release)),
            ])?;
            println!("[xtask] release-check ({release}): OK (for the checks listed above only)");
        }
        Command::BenchTelemetry => {
            println!("[xtask] bench-telemetry: NOT implemented yet (no benchmark was run)");
        }
        Command::SimulateConfig { config } => {
            println!(
                "[xtask] simulate-config: NOT implemented yet (nothing was simulated for {})",
                config.display()
            );
        }
        Command::Plan { wave } => {
            println!("[xtask] plan: NOT implemented yet (no task DAG was emitted for wave {wave})");
        }
        Command::Pipeline { release } => {
            say_what_ac_regression_covers();
            run_all(&[
                ("workspace-manifest", &workspace::check),
                ("config-toml", &|| config_toml(Path::new("config.toml"))),
                ("network-surfaces", &network::check),
                ("crate-boundaries", &boundaries::check),
                ("electron-audit", &electron::check),
                ("tauri-capability-audit", &tauri::check_capabilities),
                ("webview-runtime-audit", &tauri::check_runtime),
                ("release-ladder", &|| release::check(&release)),
                ("ipc-check", &ipc::check),
            ])?;
            println!("[xtask] pipeline ({release}): OK (for the checks listed above only)");
        }
    }
    Ok(())
}
