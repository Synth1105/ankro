

use clap::{Parser, Subcommand};

/// Gatekeeper For Your Web Application.
#[derive(Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

/// ankro's commands
#[derive(Subcommand)]
pub enum Commands {
    /// serve ankro at port.
    Serve {
        /// The port to serve on
        #[arg(short, long, default_value_t = 1234)]
        port: u32,

        /// The target address or host
        #[arg(short, long)]
        target: String,
    },
}
