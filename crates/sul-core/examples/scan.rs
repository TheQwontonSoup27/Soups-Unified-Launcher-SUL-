//! Run the real game scanner against this machine and print what it finds.
//!
//! ```text
//! cargo run -p sul-core --example scan
//! cargo run -p sul-core --example scan -- --matrix --verbose
//! ```
//!
//! Flags:
//!   --all           also list installed programs with no sign of being a game
//!   --keep-missing  keep entries whose install folder no longer exists
//!   --matrix        print each game's tick-and-compatibility rows
//!   --verbose       show debug logging (skipped keys, unreadable manifests)
//!
//! Registry scanning needs Windows; elsewhere this prints why it cannot run.

use std::process::ExitCode;

use sul_core::library::LibraryIndex;
use sul_core::registry::{ScanOptions, system_scanner};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);

    tracing_subscriber::fmt()
        .with_max_level(if flag("--verbose") {
            tracing::Level::DEBUG
        } else {
            tracing::Level::WARN
        })
        .init();

    let options = ScanOptions {
        include_unclassified: flag("--all"),
        keep_missing: flag("--keep-missing"),
    };

    let scanner = match system_scanner(options) {
        Ok(scanner) => scanner,
        Err(e) => {
            eprintln!("cannot scan: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut index = LibraryIndex::new();
    if let Err(e) = index.refresh(scanner.as_ref()) {
        eprintln!("scan failed: {e}");
        return ExitCode::FAILURE;
    }

    let mut entries: Vec<_> = index.iter().collect();
    entries.sort_by_cached_key(|e| e.game.name.to_lowercase());

    println!("{} games found\n", entries.len());
    for entry in entries {
        let game = &entry.game;
        println!("{} [{:?}]", game.name, game.source);
        println!("    id:      {}", game.id);
        println!("    folder:  {}", game.install_dir.display());
        if let Some(publisher) = &game.publisher {
            println!("    by:      {publisher}");
        }
        if flag("--matrix") {
            for (feature, state) in entry.matrix.iter() {
                let tick = if state.enabled { 'x' } else { ' ' };
                let note = state.note.as_deref().unwrap_or("");
                println!(
                    "    [{tick}] {:<24} {:?} {note}",
                    feature.label(),
                    state.compatibility
                );
            }
        }
    }
    ExitCode::SUCCESS
}
