use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    state::UpgradeableLoaderState,
    instruction as bpf_loader_upgradeable,
};
use solana_sdk_ids::bpf_loader_upgradeable::ID as LOADER_ID;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::{
    pubkey::Pubkey, 
    signature::{Keypair, Signer}, 
    transaction::Transaction,
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
};
use solana_address::Address;
use solana_pubkey::Pubkey as SolanaPubkeyV2;
use solana_system_interface::instruction as system_instruction;
use std::fs;
use std::str::FromStr;
use crate::config::Config;
use crate::utils::*;

const MIN_UPGRADE_BALANCE: u64 = 1_000_000_000; // 1 SOL minimum

pub async fn execute(program_id_str: String) -> Result<()> {
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

    let program_id = Pubkey::from_str(&program_id_str)
        .context("Invalid program ID")?;

    // Verify program ownership BEFORE expensive operations
    println!("\n Verifying upgrade authority...");
    verify_upgrade_authority_early(
        &get_rpc_url()?,
        &program_id,
        &deployer,
    ).await?;
    
    if state.deployed_programs.is_empty() {
        anyhow::bail!(
            "No programs deployed yet.\n\
            Run `shield-deploy deploy` first."
        );
    }
    
    let program_file = detect_program_file()
        .ok_or_else(|| anyhow::anyhow!(
            "No program file found.\n\
            Build your program first or specify with --program"
        ))?;
    
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
    
    println!("  ↳ Program ID: {program_id}");
    
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

/// Early verification to avoid wasting time on buffer writes
async fn verify_upgrade_authority_early(
    rpc_url: &str,
    program_id: &Pubkey,
    expected_authority: &Keypair,
) -> Result<()> {
    let rpc_client = RpcClient::new_with_commitment(
        rpc_url.to_string(),
        CommitmentConfig::confirmed(),
    );
    
    let program_account = rpc_client
        .get_account(program_id)
        .context("Failed to fetch program account - it may not exist")?;
    
    let loader_id_sdk = Pubkey::new_from_array(LOADER_ID.to_bytes());
    if program_account.owner != loader_id_sdk {
        anyhow::bail!("Program is not an upgradeable program");
    }
    
    let programdata_address = match bincode::deserialize::<UpgradeableLoaderState>(
        &program_account.data
    )? {
        UpgradeableLoaderState::Program { programdata_address } => programdata_address,
        _ => anyhow::bail!("Invalid program account"),
    };
    
    let programdata_address_sdk = Pubkey::from(programdata_address.to_bytes());
    let programdata = rpc_client.get_account(&programdata_address_sdk)
        .context("ProgramData account not found - program may be closed")?;
    
    match bincode::deserialize::<UpgradeableLoaderState>(&programdata.data)? {
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address,
            ..
        } => {
            match upgrade_authority_address {
                None => anyhow::bail!(
                    "Program is immutable (upgrade authority is None)"
                ),
                Some(authority) => {
                    if authority.to_bytes() != expected_authority.pubkey().to_bytes() {
                        anyhow::bail!(
                            "Authority mismatch.\n\
                            Expected: {}\n\
                            Found: {}",
                            expected_authority.pubkey(),
                            authority
                        );
                    }
                }
            }
        }
        _ => anyhow::bail!("Invalid ProgramData account"),
    }
    
    println!("  ✓ Upgrade authority verified");
    Ok(())
}

/// Upgrade a program using BPF Loader Upgradeable
/// 
/// This follows the official Solana upgrade process:
/// 1. Create a new buffer account
/// 2. Write new program data to buffer
/// 3. Upgrade program from buffer
/// 4. Buffer is automatically closed
pub async fn upgrade_program_bpf_upgradeable(
    rpc_client: &RpcClient,
    upgrade_authority: &Keypair,
    program_id: &Pubkey,
    new_program_data: &[u8],
) -> Result<()> {
    let authority_pubkey = upgrade_authority.pubkey();
    
    // Derive ProgramData address
    let loader_id_sdk = Pubkey::new_from_array(LOADER_ID.to_bytes());
    let (programdata_address, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &loader_id_sdk,
    );
    
    println!("  ↳ ProgramData address: {programdata_address}");
    
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

    let authority_addr = Address::from(authority_pubkey.to_bytes());
    let buffer_addr = Address::from(buffer_pubkey.to_bytes());
    let loader_addr = Address::from(LOADER_ID.to_bytes());
    
    // Create buffer account
    let create_buffer_ix = system_instruction::create_account(
        &authority_addr,
        &buffer_addr,
        buffer_lamports,
        buffer_size as u64,
        &loader_addr,
    );

    let sdk_instruction = SdkInstruction {
        program_id: Pubkey::from(create_buffer_ix.program_id.to_bytes()),
        accounts: create_buffer_ix
            .accounts
            .iter()
            .map(|acc| AccountMeta {
                pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                is_signer: acc.is_signer,
                is_writable: acc.is_writable,
            })
            .collect(),
        data: create_buffer_ix.data,
    };
    
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &[sdk_instruction],
        Some(&authority_pubkey),
    );
    transaction.sign(&[upgrade_authority, &buffer_keypair], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to create buffer account")?;
    
    println!("  ✓ Buffer created: {signature}");
    
    println!("\n Writing new program data...");
    
    write_program_data_chunked(
        rpc_client,
        upgrade_authority,
        &buffer_pubkey,
        new_program_data,
        true,
    )
    .await
    .context("Failed to write program data")?;
    
    println!("\n Upgrading program...");
    
    // Convert to privacy_cash::Pubkey
    let program_v2 = SolanaPubkeyV2::new_from_array(program_id.to_bytes());
    let buffer_v2 = SolanaPubkeyV2::new_from_array(buffer_pubkey.to_bytes());
    let authority_v2 = SolanaPubkeyV2::new_from_array(authority_pubkey.to_bytes());


    let upgrade_ix = bpf_loader_upgradeable::upgrade(
        &program_v2,
        &buffer_v2,
        &authority_v2,
        &authority_v2,
    );

    let sdk_instruction = SdkInstruction {
        program_id: Pubkey::from(upgrade_ix.program_id.to_bytes()),
        accounts: upgrade_ix
            .accounts
            .iter()
            .map(|acc| AccountMeta {
                pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                is_signer: acc.is_signer,
                is_writable: acc.is_writable,
            })
            .collect(),
        data: upgrade_ix.data,
    };
        
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &[sdk_instruction],
        Some(&authority_pubkey),
    );
    transaction.sign(&[upgrade_authority], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .context("Failed to upgrade program")?;
    
    println!("  ✓ Program upgraded: {signature}");

    // Get program name
    let lib_name = get_program_lib_name()?;
    
    // Update IDL after successful upgrade
    deploy_idl_if_available(program_id, &lib_name).await?;
    
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
                if authority.to_bytes() == expected_authority.to_bytes() {
                    println!("  ✓ Upgrade authority verified");
                    Ok(())
                } else {
                    anyhow::bail!(
                        "Upgrade authority mismatch.\n\
                        Expected: {expected_authority}\n\
                        Found: {authority}"
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
#[deprecated]
#[allow(dead_code)]
async fn write_program_data_to_buffer(
    rpc_client: &RpcClient,
    authority: &Keypair,
    buffer_pubkey: &Pubkey,
    program_data: &[u8],
) -> Result<()> {
    let chunk_size = 900;
    let total_chunks = program_data.len().div_ceil(chunk_size);
    
    println!("  ↳ Writing {} bytes in {} chunks", program_data.len(), total_chunks);
    
    for (chunk_index, chunk) in program_data.chunks(chunk_size).enumerate() {
        let offset = chunk_index * chunk_size;
        
        // Convert to privacy_cash::Pubkey
        let buffer_v2 = SolanaPubkeyV2::new_from_array(buffer_pubkey.to_bytes());
        let authority_v2 = SolanaPubkeyV2::new_from_array(authority.pubkey().to_bytes());
        
        let write_ix = bpf_loader_upgradeable::write(
            &buffer_v2,
            &authority_v2,
            offset as u32,
            chunk.to_vec(),
        );
        
        // Convert to solana_sdk::Instruction
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
            .context(format!("Failed to write chunk {} of {}", chunk_index + 1, total_chunks))?;
        
        if (chunk_index + 1) % 10 == 0 || chunk_index + 1 == total_chunks {
            println!("  ↳ Progress: {}/{} chunks", chunk_index + 1, total_chunks);
        }
    }
    
    println!("  ✓ All data written successfully");
    
    Ok(())
}