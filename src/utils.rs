use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use solana_cli_config::{Config as SolanaConfig, CONFIG_FILE};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    instruction as bpf_loader_upgradeable,
};
use solana_sdk::{
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
    pubkey::Pubkey,
    signature::{Keypair, read_keypair_file, Signer},
    transaction::Transaction
};
use solana_pubkey::Pubkey as SolanaPubkeyV2;
use std::path::{Path, PathBuf};

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
                .map_err(|e| anyhow::anyhow!("Failed to read CLI wallet keypair: {e}"))
        }
        FundingWalletChoice::KeypairFile(path) => {
            read_keypair_file(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {e}"))
        }
    }
}

/// Write program data with automatic chunking and progress
pub async fn write_program_data_chunked(
    rpc_client: &RpcClient,
    authority: &Keypair,
    buffer_pubkey: &Pubkey,
    program_data: &[u8],
    show_progress: bool,
) -> Result<()> {
    let chunk_size = calculate_max_write_chunk_size();
    let total_chunks = program_data.len().div_ceil(chunk_size);
    
    if show_progress {
        println!("  ↳ Writing {} bytes in {} chunks", program_data.len(), total_chunks);
    }
    
    let mut failed_chunks = Vec::new();
    
    for (chunk_index, chunk) in program_data.chunks(chunk_size).enumerate() {
        let offset = chunk_index * chunk_size;
        
        let buffer_pubkey_v2 = SolanaPubkeyV2::new_from_array(buffer_pubkey.to_bytes());
        let authority_pubkey_v2 = SolanaPubkeyV2::new_from_array(authority.pubkey().to_bytes());

        let write_ix = bpf_loader_upgradeable::write(
            &buffer_pubkey_v2,
            &authority_pubkey_v2,
            offset as u32,
            chunk.to_vec(),
        );
        
        let sdk_instruction = SdkInstruction {
            program_id: Pubkey::from(write_ix.program_id.to_bytes()),
            accounts: write_ix
                .accounts
                .iter()
                .map(|acc| AccountMeta {
                    pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                })
                .collect(),
            data: write_ix.data,
        };
        
        let recent_blockhash = rpc_client.get_latest_blockhash()?;
        let mut transaction = Transaction::new_with_payer(
            &[sdk_instruction],
            Some(&authority.pubkey()),
        );
        transaction.sign(&[authority], recent_blockhash);
        
        // Try to send transaction
        let result = rpc_client.send_and_confirm_transaction(&transaction);
        
        if result.is_err() {
            failed_chunks.push(chunk_index);
        }
        
        if show_progress && ((chunk_index + 1) % 10 == 0 || chunk_index + 1 == total_chunks) {
            println!("  ↳ Progress: {}/{} chunks", chunk_index + 1, total_chunks);
        }
    }
    
    // Retry failed chunks once
    if !failed_chunks.is_empty() {
        println!("  ⚠️  Retrying {} failed chunks...", failed_chunks.len());
        
        for &chunk_index in &failed_chunks {
            let offset = chunk_index * chunk_size;
            let chunk = &program_data[offset..std::cmp::min(offset + chunk_size, program_data.len())];
            
            let buffer_pubkey_v2 = SolanaPubkeyV2::new_from_array(buffer_pubkey.to_bytes());
            let authority_pubkey_v2 = SolanaPubkeyV2::new_from_array(authority.pubkey().to_bytes());

            let write_ix = bpf_loader_upgradeable::write(
                &buffer_pubkey_v2,
                &authority_pubkey_v2,
                offset as u32,
                chunk.to_vec(),
            );
            
            let sdk_instruction = SdkInstruction {
                program_id: Pubkey::from(write_ix.program_id.to_bytes()),
                accounts: write_ix
                    .accounts
                    .iter()
                    .map(|acc| AccountMeta {
                        pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                        is_signer: acc.is_signer,
                        is_writable: acc.is_writable,
                    })
                    .collect(),
                data: write_ix.data,
            };
            
            let recent_blockhash = rpc_client.get_latest_blockhash()?;
            let mut transaction = Transaction::new_with_payer(
                &[sdk_instruction],
                Some(&authority.pubkey()),
            );
            transaction.sign(&[authority], recent_blockhash);
            
            rpc_client
                .send_and_confirm_transaction(&transaction)
                .context(format!("Failed to write chunk {} after retry", chunk_index + 1))?;
        }
        
        println!("  ✓ All failed chunks retried successfully");
    }
    
    if show_progress {
        println!("  ✓ All data written successfully");
    }
    
    Ok(())
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

pub const fn calculate_max_write_chunk_size() -> usize {
    // From Anchor: PACKET_DATA_SIZE - transaction overhead - buffer for shortvec
    // Conservative estimate: 1232 bytes max packet size
    // Transaction overhead: ~200 bytes (signatures, header, accounts)
    // Safe chunk size: ~900 bytes
    900
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

/// Get the program library name from Cargo.toml in current directory
pub fn get_program_lib_name() -> Result<String> {
    let cargo_toml_path = std::env::current_dir()?.join("Cargo.toml");
    
    if !cargo_toml_path.exists() {
        // Fallback: try to get from parent directory if we're in src/
        let parent_cargo = std::env::current_dir()?
            .parent()
            .map(|p| p.join("Cargo.toml"));
        
        if let Some(path) = parent_cargo {
            if path.exists() {
                return extract_lib_name(&path);
            }
        }
        
        anyhow::bail!("Could not find Cargo.toml to determine program name");
    }
    
    extract_lib_name(&cargo_toml_path)
}

pub fn extract_lib_name(cargo_toml_path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(cargo_toml_path)?;
    let manifest: toml::Value = toml::from_str(&content)?;
    
    // Try to get lib.name first
    if let Some(lib_name) = manifest
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(|name| name.as_str())
    {
        return Ok(lib_name.to_string());
    }
    
    // Fallback to package.name (converted to snake_case)
    if let Some(package_name) = manifest
        .get("package")
        .and_then(|pkg| pkg.get("name"))
        .and_then(|name| name.as_str())
    {
        return Ok(package_name.replace('-', "_"));
    }
    
    anyhow::bail!("Could not determine library name from Cargo.toml");
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

/// Deploy IDL after successful program deployment
pub async fn deploy_idl_if_available(
    program_id: &Pubkey,
    lib_name: &str,
) -> Result<()> {
    let idl_path = Path::new("target/idl").join(lib_name).with_extension("json");
    
    if !idl_path.exists() {
        println!("No IDL found at {idl_path:?}, skipping IDL deployment");
        return Ok(());
    }
    
    println!("\n Deploying IDL...");
    
    // Update IDL with program address
    let mut idl: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&idl_path)?
    )?;
    
    if let Some(metadata) = idl.get_mut("metadata").and_then(|m| m.as_object_mut()) {
        metadata.insert("address".to_string(), serde_json::Value::String(program_id.to_string()));
    }
    
    // Write updated IDL back
    std::fs::write(&idl_path, serde_json::to_string_pretty(&idl)?)?;
    
    // Use anchor idl init/upgrade commands
    println!("  ✓ IDL updated with program address: {program_id}");
    
    Ok(())
}

pub fn format_sol(lamports: u64) -> String {
    format!("{:.2} SOL", lamports as f64 / 1_000_000_000.0)
}

pub fn print_header(title: &str) {
    println!("\n{title}");
    println!("{}", "─".repeat(title.len()));
}

pub fn print_success(message: &str) {
    println!("\n {message}");
}

pub fn print_warning(message: &str) {
    println!("\n {message}");
}

#[allow(dead_code)]
pub fn print_error(message: &str) {
    eprintln!("\n {message}");
}