//! Spike binary entry point — skeleton (Task 1).
//!
//! Full `tokio::select!` integration wired in Task 7.

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "spike", about = "Listary-style global focus sync spike")]
struct Cli {
    #[arg(long, default_value_t = 37421)]
    port: u16,
    #[arg(long, default_value = "C:\\")]
    initial_path: String,
    #[arg(long, value_delimiter = ',')]
    app_whitelist: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing::info!(port = cli.port, "spike skeleton starting (Task 1)");
    Ok(())
}