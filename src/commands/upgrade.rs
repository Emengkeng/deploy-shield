use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    state::UpgradeableLoaderState,
    instruction as bpf_loader_upgradeable,
};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solana_system_interface::instruction as system_instruction;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use crate::config::Config;
use crate::utils::*;

const MIN_UPGRADE_BALANCE: u64 = 1_000_000_000; // 1 SOL minimum

pub async fn execute(program_path: Option<String>) -> Result<()> {
    print_header("Upgrade Program");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        anyhow::bail!(
            "No private deployer found.\n\
            Run `shield-deploy init` first."
        );
    }
    
    let deployer = config.load_deployer()?;
    let mut state = config.load_state()?;
    
    if state.deployed_programs.is_empty() {
        anyhow::bail!(
            "No programs deployed yet.\n\
            Run `shield-deploy deploy` first."
        );
    }
    
    let program_file = if let Some(path) = program_path {
        PathBuf::from(path)
    } else {
        detect_program_file()
            .ok_or_else(|| anyhow::anyhow!(
                "No program file found.\n\
                Build your program first or specify with --program"
            ))?
    };
    
    if !program_file.exists() {
        anyhow::bail!("Program file not found: {}", program_file.display());
    }
    
    println!("\nThis will:");
    println!("• Rebuild your program");
    println!("• Use the same private deployer");
    println!("• Preserve on-chain privacy\n");
    
    if !prompt_confirmation("Proceed?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    let rpc_url = get_rpc_url()?;
    let rpc_client = RpcClient::new_with_commitment(
        rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    
    let balance = rpc_client.get_balance(&deployer.pubkey())
        .context("Failed to get deployer balance")?;
    
    if balance < MIN_UPGRADE_BALANCE {
        anyhow::bail!(
            "Insufficient deployer balance.\n\
            Current: {}\n\
            Needed: ~1 SOL\n\
            Run `shield-deploy fund` to add more SOL.",
            format_sol(balance)
        );
    }
    
    println!("\n⬆ Upgrading program...");
    
    let program_data = fs::read(&program_file)
        .context("Failed to read program file")?;
    
    println!("  ↳ New program size: {} bytes", program_data.len());
    
    // Get the last deployed program
    let last_program = state.deployed_programs.last_mut()
        .ok_or_else(|| anyhow::anyhow!("No program found"))?;
    
    let program_id = Pubkey::from_str(&last_program.program_id)
        .context("Invalid program ID in state")?;
    
    println!("  ↳ Program ID: {}", program_id);
    
    upgrade_program_bpf_upgradeable(
        &rpc_client,
        &deployer,
        &program_id,
        &program_data,
    )
    .await
    .context("Failed to upgrade program")?;
    
    print_success("Program upgraded successfully");
    
    println!("\nUpgrade authority unchanged.");
    
    last_program.last_upgraded = Some(chrono::Utc::now().timestamp());
    state.last_balance = balance;
    config.save_state(&state)?;
    
    Ok(())
}

/// Upgrade a program using BPF Loader Upgradeable
/// 
/// This follows the official Solana upgrade process:
/// 1. Create a new buffer account
/// 2. Write new program data to buffer
/// 3. Upgrade program from buffer
/// 4. Buffer is automatically closed
async fn upgrade_program_bpf_upgradeable(
    rpc_client: &RpcClient,
    upgrade_authority: &Keypair,
    program_id: &Pubkey,
    new_program_data: &[u8],
) -> Result<()> {
    let authority_pubkey = upgrade_authority.pubkey();
    
    // Derive ProgramData address
    let (programdata_address, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &bpf_loader_upgradeable::id(),
    );
    
    println!("  ↳ ProgramData address: {}", programdata_address);
    
    // Verify upgrade authority
    verify_upgrade_authority(
        rpc_client,
        &programdata_address,
        &authority_pubkey,
    )
    .await
    .context("Authority verification failed")?;
    
    println!("\n Creating upgrade buffer...");
    
    let buffer_keypair = Keypair::new();
    let buffer_pubkey = buffer_keypair.pubkey();
    
    // Calculate required size for buffer
    let buffer_size = UpgradeableLoaderState::size_of_buffer(new_program_data.len());
    let buffer_lamports = rpc_client
        .get_minimum_balance_for_rent_exemption(buffer_size)
        .context("Failed to get rent exemption for buffer")?;
    
    // Create buffer account
    let create_buffer_ix = system_instruction::create_account(
        &authority_pubkey,
        &buffer_pubkey,
        buffer_lamports,
        buffer_size as u64,
        &bpf_loader_upgradeable::id(),
    );
    
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &[create_buffer_ix],
        Some(&authority_pubkey),
    );
    transaction.sign(&[upgrade_authority, &buffer_keypair], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to create buffer account")?;
    
    println!("  ✓ Buffer created: {}", signature);
    
    println!("\n Writing new program data...");
    
    write_program_data_to_buffer(
        rpc_client,
        upgrade_authority,
        &buffer_pubkey,
        new_program_data,
    )
    .await
    .context("Failed to write program data")?;
    
    println!("\n Upgrading program...");
    
    let upgrade_ix = bpf_loader_upgradeable::upgrade(
        program_id,
        &buffer_pubkey,
        &authority_pubkey,
        &authority_pubkey, // spill account (receives refund)
    );
    
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &[upgrade_ix],
        Some(&authority_pubkey),
    );
    transaction.sign(&[upgrade_authority], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .context("Failed to upgrade program")?;
    
    println!("  ✓ Program upgraded: {}", signature);
    
    Ok(())
}

/// Verify that the current authority matches expected authority
async fn verify_upgrade_authority(
    rpc_client: &RpcClient,
    programdata_address: &Pubkey,
    expected_authority: &Pubkey,
) -> Result<()> {
    let account = rpc_client
        .get_account(programdata_address)
        .context("ProgramData account not found")?;
    
    // Parse ProgramData account
    let programdata_state = bincode::deserialize::<UpgradeableLoaderState>(&account.data)
        .context("Failed to deserialize ProgramData")?;
    
    match programdata_state {
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address,
            slot: _,
        } => {
            if let Some(authority) = upgrade_authority_address {
                if authority == *expected_authority {
                    println!("  ✓ Upgrade authority verified");
                    Ok(())
                } else {
                    anyhow::bail!(
                        "Upgrade authority mismatch.\n\
                        Expected: {}\n\
                        Found: {}",
                        expected_authority,
                        authority
                    )
                }
            } else {
                anyhow::bail!("Program is not upgradeable (authority set to None)")
            }
        }
        _ => anyhow::bail!("Invalid ProgramData account state"),
    }
}

/// Write program data to buffer account in chunks
/// 
/// Same implementation as deploy, but extracted for reuse
async fn write_program_data_to_buffer(
    rpc_client: &RpcClient,
    authority: &Keypair,
    buffer_pubkey: &Pubkey,
    program_data: &[u8],
) -> Result<()> {
    let chunk_size = 900; // Safe size per transaction
    let total_chunks = (program_data.len() + chunk_size - 1) / chunk_size;
    
    println!("  ↳ Writing {} bytes in {} chunks", program_data.len(), total_chunks);
    
    for (chunk_index, chunk) in program_data.chunks(chunk_size).enumerate() {
        let offset = chunk_index * chunk_size;
        
        // Create write instruction
        let write_ix = bpf_loader_upgradeable::write(
            buffer_pubkey,
            &authority.pubkey(),
            offset as u32,
            chunk.to_vec(),
        );
        
        let recent_blockhash = rpc_client.get_latest_blockhash()?;
        let mut transaction = Transaction::new_with_payer(
            &[write_ix],
            Some(&authority.pubkey()),
        );
        transaction.sign(&[authority], recent_blockhash);
        
        rpc_client
            .send_and_confirm_transaction(&transaction)
            .context(format!("Failed to write chunk {} of {}", chunk_index + 1, total_chunks))?;
        
        // Progress indicator
        if (chunk_index + 1) % 10 == 0 || chunk_index + 1 == total_chunks {
            println!("  ↳ Progress: {}/{} chunks", chunk_index + 1, total_chunks);
        }
    }
    
    println!("  ✓ All data written successfully");
    
    Ok(())
}