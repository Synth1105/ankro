//! Command-line argument definitions for `ankro`.
//!
//! The binary currently exposes one subcommand, `serve`, which starts the TCP
//! bridge and connects it to a target executable.

use clap::{Parser, Subcommand};

/// Parsed top-level CLI arguments for `ankro`.
#[derive(Parser)]
pub struct Args {
    /// The selected subcommand.
    #[command(subcommand)]
    pub command: Commands,
}

/// Supported `ankro` subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Run the bridge server.
    Serve {
        /// TCP port used by the bridge server.
        #[arg(short, long, default_value_t = 1234)]
        port: u32,

        /// Path or command name of the target executable.
        #[arg(short, long)]
        target: String,

        /// Number of requests allowed per IP before banning.
        #[arg(long, default_value_t = 1000)]
        ban_threshold: usize,
    },
}
