use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use solana_cli_config::{Config as SolanaConfig, CONFIG_FILE};
use solana_sdk::signature::{read_keypair_file, Keypair};
use std::path::PathBuf;

pub fn prompt_confirmation(message: &str) -> Result<bool> {
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(message)
        .default(true)
        .interact()
        .context("Failed to get user confirmation")
}

pub fn prompt_amount(message: &str) -> Result<f64> {
    let input: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(message)
        .validate_with(|input: &String| -> Result<(), &str> {
            match input.parse::<f64>() {
                Ok(val) if val > 0.0 => Ok(()),
                _ => Err("Please enter a positive number"),
            }
        })
        .interact_text()
        .context("Failed to get amount")?;
    
    input.parse::<f64>()
        .context("Failed to parse amount")
}

pub enum FundingWalletChoice {
    SolanaCli,
    KeypairFile(PathBuf),
}

pub fn prompt_funding_wallet() -> Result<FundingWalletChoice> {
    let choices = vec!["Use current Solana CLI wallet", "Use a keypair file", "Cancel"];
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Choose a funding wallet")
        .items(&choices)
        .default(0)
        .interact()
        .context("Failed to select wallet")?;
    
    match selection {
        0 => Ok(FundingWalletChoice::SolanaCli),
        1 => {
            let path: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Keypair file path")
                .interact_text()
                .context("Failed to get keypair path")?;
            Ok(FundingWalletChoice::KeypairFile(PathBuf::from(path)))
        }
        _ => anyhow::bail!("Funding cancelled by user"),
    }
}

pub fn load_funding_keypair(choice: FundingWalletChoice) -> Result<Keypair> {
    match choice {
        FundingWalletChoice::SolanaCli => {
            let config_file = CONFIG_FILE
                .as_ref()
                .context("Unable to determine Solana config file path")?;
            
            let config = SolanaConfig::load(config_file)
                .context("Failed to load Solana CLI config")?;
            
            read_keypair_file(&config.keypair_path)
                .map_err(|e| anyhow::anyhow!("Failed to read CLI wallet keypair: {}", e))
        }
        FundingWalletChoice::KeypairFile(path) => {
            read_keypair_file(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))
        }
    }
}

pub fn get_rpc_url() -> Result<String> {
    // Try to get from Solana CLI config
    if let Some(config_file) = CONFIG_FILE.as_ref() {
        if let Ok(config) = SolanaConfig::load(config_file) {
            return Ok(config.json_rpc_url);
        }
    }
    
    // Default to devnet for hackathon
    Ok("https://api.devnet.solana.com".to_string())
}

pub fn get_network_name() -> String {
    get_rpc_url()
        .ok()
        .and_then(|url| {
            if url.contains("devnet") {
                Some("devnet")
            } else if url.contains("mainnet") {
                Some("mainnet-beta")
            } else if url.contains("testnet") {
                Some("testnet")
            } else {
                Some("localhost")
            }
            .map(String::from)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn detect_program_file() -> Option<PathBuf> {
    // Common Anchor project structure
    let anchor_path = PathBuf::from("target/deploy");
    if !anchor_path.exists() {
        return None;
    }
    
    std::fs::read_dir(&anchor_path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.path().extension().and_then(|s| s.to_str()) == Some("so")
        })
        .map(|entry| entry.path())
}

pub fn format_sol(lamports: u64) -> String {
    format!("{:.2} SOL", lamports as f64 / 1_000_000_000.0)
}

pub fn print_header(title: &str) {
    println!("\n{}", title);
    println!("{}", "─".repeat(title.len()));
}

pub fn print_success(message: &str) {
    println!("\n {}", message);
}

pub fn print_warning(message: &str) {
    println!("\n {}", message);
}

pub fn print_error(message: &str) {
    eprintln!("\n {}", message);
}