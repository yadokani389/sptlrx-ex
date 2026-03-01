mod cli;
mod client;
mod model;
mod render;
mod server;

use anyhow::{Result, bail};
use clap::Parser;

use crate::cli::{CliArgs, RunRole};

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();
    if let Err(error) = run(args).await {
        eprintln!("[relay] {error}");
        std::process::exit(1);
    }
}

async fn run(args: CliArgs) -> Result<()> {
    match args.role {
        RunRole::Bridge => server::run(args).await,
        RunRole::Client => client::run(args).await,
        RunRole::Auto => run_auto(args).await,
    }
}

async fn run_auto(args: CliArgs) -> Result<()> {
    match server::bind_listener(&args.host, args.port).await {
        Ok(listener) => server::run_with_listener(args, listener).await,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let upstream = args.upstream_base_url();
            if client::looks_like_bridge(&upstream).await {
                eprintln!(
                    "[relay] Port {} is already in use on {}. Detected running bridge at {} and switching to client mode.",
                    args.port, args.host, upstream
                );
                return client::run(args).await;
            }

            bail!(
                "Port {} is already in use on {} and {} is not a compatible sptlrx-ex bridge.",
                args.port,
                args.host,
                upstream
            )
        }
        Err(error) => Err(error.into()),
    }
}
