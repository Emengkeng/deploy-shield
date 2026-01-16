use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    state::UpgradeableLoaderState,
    instruction as bpf_loader_upgradeable,
};
use solana_sdk_ids::bpf_loader_upgradeable::ID as LOADER_ID;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
};
use solana_system_interface::instruction as system_instruction;
use std::fs;
use std::path::PathBuf;
use crate::config::{Config, DeployedProgram};
use crate::utils::*;

const MIN_DEPLOY_BALANCE: u64 = 5_000_000_000; // 5 SOL minimum
const MAX_PERMITTED_DATA_INCREASE: usize = 10 * 1024; // 10KB per transaction

pub async fn execute(program_path: Option<String>) -> Result<()> {
    print_header("Deploy Program");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        anyhow::bail!(
            "No private deployer found.\n\
            Run `shield-deploy init` first."
        );
    }
    
    let deployer = config.load_deployer()?;
    
    // Detect or use provided program
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
    
    println!("\nBuild artifact detected:");
    println!("• {}\n", program_file.display());
    
    println!("This deployment will:");
    println!("• Use the private deployer");
    println!("• Hide your funding wallet on-chain");
    println!("• Set upgrade authority to the deployer\n");
    
    if !prompt_confirmation("Proceed?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    // Check deployer balance
    let rpc_url = get_rpc_url()?;
    let rpc_client = RpcClient::new_with_commitment(
        rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    
    let balance = rpc_client.get_balance(&deployer.pubkey())
        .context("Failed to get deployer balance")?;
    
    if balance < MIN_DEPLOY_BALANCE {
        anyhow::bail!(
            "Insufficient deployer balance.\n\
            Current: {}\n\
            Needed: ~5 SOL\n\
            Run `shield-deploy fund` to add more SOL.",
            format_sol(balance)
        );
    }
    
    println!("\n Deploying program...");
    
    let program_data = fs::read(&program_file)
        .context("Failed to read program file")?;
    
    println!("  ↳ Program size: {} bytes", program_data.len());
    
    // Generate program keypair
    let program_keypair = Keypair::new();
    let program_id = program_keypair.pubkey();
    
    println!("  ↳ Program ID: {}", program_id);
    
    // Deploy program using BPF Loader Upgradeable
    deploy_program_bpf_upgradeable(
        &rpc_client,
        &deployer,
        &program_keypair,
        &program_data,
    )
    .await
    .context("Failed to deploy program")?;
    
    print_success("Program deployed");
    
    println!("\nProgram ID:        {}", program_id);
    println!("Upgrade authority: private deployer");
    
    let mut state = config.load_state()?;
    state.deployed_programs.push(DeployedProgram {
        program_id: program_id.to_string(),
        deployed_at: chrono::Utc::now().timestamp(),
        last_upgraded: None,
    });
    state.last_balance = balance;
    config.save_state(&state)?;
    
    println!("\nNext steps:");
    println!("→ Upgrade later with `shield-deploy upgrade`");
    println!("→ Transfer authority if desired");
    
    Ok(())
}

/// Deploy a program using BPF Loader Upgradeable
/// 
/// This follows the official Solana deployment process:
/// 1. Create buffer account with program data
/// 2. Write program data to buffer (in chunks)
/// 3. Deploy from buffer to program account
/// 4. Set deployer as upgrade authority
async fn deploy_program_bpf_upgradeable(
    rpc_client: &RpcClient,
    deployer: &Keypair,
    program_keypair: &Keypair,
    program_data: &[u8],
) -> Result<()> {
    let program_id = program_keypair.pubkey();
    let deployer_pubkey = deployer.pubkey();
    
    println!("\n Creating program buffer...");
    
    let buffer_keypair = Keypair::new();
    let buffer_pubkey = buffer_keypair.pubkey();
    
    // Calculate required size for buffer
    let buffer_size = UpgradeableLoaderState::size_of_buffer(program_data.len());
    let buffer_lamports = rpc_client
        .get_minimum_balance_for_rent_exemption(buffer_size)
        .context("Failed to get rent exemption for buffer")?;
    
    let deployer_pubkey_pc = privacy_cash::Pubkey::from(deployer_pubkey.to_bytes());
    let buffer_pubkey_pc = privacy_cash::Pubkey::from(buffer_pubkey.to_bytes());
    let loader_id_pc = privacy_cash::Pubkey::from(bpf_loader_upgradeable::id().to_bytes());

    // Create buffer account
    let create_buffer_ix = system_instruction::create_account(
        &deployer_pubkey_pc,
        &buffer_pubkey_pc,
        buffer_lamports,
        buffer_size as u64,
        &loader_id_pc,
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
        Some(&deployer_pubkey),
    );
    transaction.sign(&[deployer, &buffer_keypair], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to create buffer account")?;
    
    println!("  ✓ Buffer created: {}", signature);
    println!("  ↳ Buffer address: {}", buffer_pubkey);
    
    println!("\n Writing program data to buffer...");
    
    write_program_data_to_buffer(
        rpc_client,
        deployer,
        &buffer_pubkey,
        program_data,
    )
    .await
    .context("Failed to write program data")?;
    
    println!("\n Deploying program from buffer...");
    
    // Calculate program account size
    let program_data_len = program_data.len();
    let programdata_size = UpgradeableLoaderState::size_of_programdata(program_data_len);
    let programdata_lamports = rpc_client
        .get_minimum_balance_for_rent_exemption(programdata_size)
        .context("Failed to get rent exemption for program data")?;
    
    // Derive ProgramData address
    let (programdata_address, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &LOADER_ID,
    );
    
    let deployer_pubkey_pc = privacy_cash::Pubkey::from(deployer_pubkey.to_bytes());
    let programdata_address_pc = privacy_cash::Pubkey::from(programdata_address.to_bytes());
    let buffer_pubkey_pc = privacy_cash::Pubkey::from(buffer_pubkey.to_bytes());
    let program_id_pc = privacy_cash::Pubkey::from(program_id.to_bytes());


    // Deploy with upgradeable loader
    let deploy_ix = bpf_loader_upgradeable::deploy_with_max_program_len(
        &deployer_pubkey_pc,
        &programdata_address_pc,
        &buffer_pubkey_pc,
        &program_id_pc,
        programdata_lamports,
        program_data_len * 2, // max_data_len
    )
    .context("Failed to create deploy instruction")?;
    
    // Convert to solana_sdk::Instruction
    let sdk_instruction = SdkInstruction {
        program_id: Pubkey::from(deploy_ix.program_id.to_bytes()),
        accounts: deploy_ix
            .accounts
            .iter()
            .map(|acc| AccountMeta {
                pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                is_signer: acc.is_signer,
                is_writable: acc.is_writable,
            })
            .collect(),
        data: deploy_ix.data,
    };

    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &[sdk_instruction],
        Some(&deployer_pubkey),
    );
    transaction.sign(&[deployer, program_keypair], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .context("Failed to deploy program")?;
    
    println!("  Program deployed: {}", signature);
    println!("  ↳ ProgramData address: {}", programdata_address);
    
    Ok(())
}

/// Write program data to buffer account in chunks
/// 
/// Large programs can't be written in a single transaction due to transaction size limits.
/// This function writes data in chunks using bpf_loader_upgradeable::write instruction.
async fn write_program_data_to_buffer(
    rpc_client: &RpcClient,
    deployer: &Keypair,
    buffer_pubkey: &Pubkey,
    program_data: &[u8],
) -> Result<()> {
    let chunk_size = 900;
    let total_chunks = (program_data.len() + chunk_size - 1) / chunk_size;
    
    println!("  ↳ Writing {} bytes in {} chunks", program_data.len(), total_chunks);
    
    for (chunk_index, chunk) in program_data.chunks(chunk_size).enumerate() {
        let offset = chunk_index * chunk_size;
        
        // Convert to privacy_cash::Pubkey
        let buffer_pubkey_pc = privacy_cash::Pubkey::from(buffer_pubkey.to_bytes());
        let deployer_pubkey_pc = privacy_cash::Pubkey::from(deployer.pubkey().to_bytes());
        
        let write_ix = bpf_loader_upgradeable::write(
            &buffer_pubkey_pc,
            &deployer_pubkey_pc,
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
            Some(&deployer.pubkey()),
        );
        transaction.sign(&[deployer], recent_blockhash);
        
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