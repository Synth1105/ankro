

use ankro::args::{Args, Commands};
use ankro::serve;
use clap::Parser;

fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Serve { port, target } => {
            if let Err(err) = serve(port, target) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}
