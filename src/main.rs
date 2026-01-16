use clap::{Parser, Subcommand, arg, command};
use anyhow::Result;

mod commands;
mod config;
mod privacy;
mod utils;

use commands::*;

#[derive(Parser)]
#[command(name = "shield-deploy")]
#[command(about = "Privacy-preserving Solana program deployment", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a private deployer for this project
    Init,
    /// Fund the private deployer through ZK Compression
    Fund,
    /// Deploy a program using the private deployer
    Deploy {
        /// Path to the program .so file
        #[arg(short, long)]
        program: Option<String>,
    },
    /// Upgrade an existing program
    Upgrade {
        /// Path to the program .so file
        #[arg(short, long)]
        program: Option<String>,
    },
    /// Show deployer status and balance
    Status,
    /// Rotate to a new private deployer
    Rotate,
    /// Transfer upgrade authority to another address
    TransferAuthority {
        /// New authority public key
        new_authority: String,
    },
    /// Make a program immutable (cannot be upgraded by anyone)
    Finalize {
        /// Program ID to finalize
        program_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => init::execute().await,
        Commands::Fund => fund::execute().await,
        Commands::Deploy { program } => deploy::execute(program).await,
        Commands::Upgrade { program } => upgrade::execute(program).await,
        Commands::Status => status::execute().await,
        Commands::Rotate => rotate::execute().await,
        Commands::TransferAuthority { new_authority } => {
            transfer_authority::execute(new_authority).await
        },
        Commands::Finalize { program_id } => {
            finalize::execute(program_id).await
        }
    }
}