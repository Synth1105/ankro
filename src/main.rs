use ankro::args::{Args, Commands};
use ankro::serve;
use clap::Parser;

fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Serve {
            port,
            target,
            ban_threshold,
        } => {
            if let Err(err) = serve(port, target, ban_threshold) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}
