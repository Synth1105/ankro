use ankro::args::{Args, Commands};
use ankro::serve;
use clap::Parser;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = Args::parse();
    
    tracing_subscriber::fmt::init();

    match args.command {
        Commands::Serve {
            port,
            target,
            ban_threshold,
        } => {
            tracing::info!("ankro server started.");
            if let Err(err) = serve(port, target, ban_threshold).await {
                tracing::error!("{err}");
                std::process::exit(1);
            }
        }
    }
}
