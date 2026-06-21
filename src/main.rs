use ankro::args::{Args, Commands};
use ankro::serve;
use clap::Parser;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Serve {
            port,
            target,
            ban_threshold,
        } => {
            if let Err(err) = serve(port, target, ban_threshold).await {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}
