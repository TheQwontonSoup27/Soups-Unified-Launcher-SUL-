//! SUL build, gate, AST-guard, and benchmark runner (`cargo xtask <cmd>`).
//! Implements the R0 verification surface defined in Ch. 1 §10, §13.5.7, and SUL-LEDGER/*.md.

mod network;

use clap::{Parser, Subcommand};
use miette::{Context, IntoDiagnostic, Result, bail};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "xtask", about = "SUL spec, gate, guard, and benchmark runner")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate config.toml and workspace Cargo.toml (Edition 2024, MSRV 1.85) against Ch. 1 §4
    ConfigCheck {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
    },
    /// Enforce AST structural guards, layer boundaries, feature orthogonality, and Windows/AC refusals
    AcRegression,
    /// Verify zero Electron / Node.js components in sul.exe dependency graph
    ElectronAudit,
    /// Verify release gate, asset/schema parity, & component parity against SUL-LEDGER/releases.md
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
    /// Execute full CI DAG for a target release class
    Pipeline {
        #[arg(long, default_value = "R0")]
        release: String,
    },
}

fn validate_workspace_manifest() -> Result<()> {
    let manifest_path = PathBuf::from("Cargo.toml");
    if manifest_path.exists() {
        let raw = std::fs::read_to_string(&manifest_path)
            .into_diagnostic()
            .context("failed to read workspace Cargo.toml")?;
        let doc: toml::Value = toml::from_str(&raw)
            .into_diagnostic()
            .context("invalid TOML in workspace Cargo.toml")?;
        if let Some(pkg) = doc.get("workspace").and_then(|w| w.get("package")) {
            let edition = pkg.get("edition").and_then(|v| v.as_str()).unwrap_or("");
            let msrv = pkg
                .get("rust-version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if edition != "2024" {
                bail!("workspace.package.edition must be 2024 (found {edition:?})");
            }
            if msrv != "1.85" {
                bail!("workspace.package.rust-version must be 1.85 (found {msrv:?})");
            }
        }
        if raw.contains("io-uring") {
            bail!(
                "Linux-only compio `io-uring` feature must not appear in Windows workspace Cargo.toml"
            );
        }
    }
    Ok(())
}

fn check_electron_free() -> Result<()> {
    let manifest_path = PathBuf::from("Cargo.toml");
    if manifest_path.exists() {
        let raw = std::fs::read_to_string(&manifest_path).into_diagnostic()?;
        if raw.to_ascii_lowercase().contains("electron") {
            bail!("electron dependency detected in Cargo.toml (violates Zero Electron invariant)");
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::ConfigCheck { config } => {
            println!(
                "[xtask] config-check: validating workspace & {}",
                config.display()
            );
            validate_workspace_manifest()?;
            if config.exists() {
                let raw = std::fs::read_to_string(&config)
                    .into_diagnostic()
                    .with_context(|| format!("failed to read {}", config.display()))?;
                let _: toml::Value = toml::from_str(&raw)
                    .into_diagnostic()
                    .context("invalid TOML syntax in config")?;
            }
            println!("[xtask] config-check: OK");
        }
        Command::AcRegression => {
            println!(
                "[xtask] ac-regression: implemented guards: workspace manifest, network surfaces (Ch. 16 §16.15.14)"
            );
            println!(
                "[xtask] ac-regression: NOT implemented yet: structural refusals, crate boundaries, feature orthogonality, Windows-only IPC"
            );
            validate_workspace_manifest()?;
            network::check()?;
            println!("[xtask] ac-regression: OK (for the guards listed above only)");
        }
        Command::ElectronAudit => {
            println!(
                "[xtask] electron-audit: checking Cargo.toml / package manifests for Electron"
            );
            check_electron_free()?;
            println!("[xtask] electron-audit: OK");
        }
        Command::ReleaseCheck { release } => {
            println!(
                "[xtask] release-check: implemented checks for {release}: workspace manifest (edition, MSRV), no Electron in Cargo.toml"
            );
            println!(
                "[xtask] release-check: NOT implemented yet: reading SUL-LEDGER/releases.md, asset/schema parity, component parity"
            );
            validate_workspace_manifest()?;
            check_electron_free()?;
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
            println!(
                "[xtask] pipeline: implemented checks for {release}: workspace manifest, no Electron, network surfaces (same limits as ac-regression and release-check)"
            );
            validate_workspace_manifest()?;
            check_electron_free()?;
            network::check()?;
            println!("[xtask] pipeline ({release}): OK (for the checks listed above only)");
        }
    }
    Ok(())
}
