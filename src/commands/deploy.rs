use anyhow::{Context, Result, anyhow};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    state::UpgradeableLoaderState,
    instruction as bpf_loader_upgradeable,
};
use solana_sdk_ids::bpf_loader_upgradeable::ID as LOADER_ID;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
    commitment_config::CommitmentConfig
};
use solana_system_interface::instruction as system_instruction;
use solana_pubkey::Pubkey as SolanaPubkeyV2;
use solana_address::Address;
use std::{fs};
use std::path::PathBuf;
use crate::config::{Config, DeployedProgram};
use crate::utils::*;
use crate::commands::upgrade::upgrade_program_bpf_upgradeable;

const MIN_DEPLOY_BALANCE: u64 = 2_000_000_000; // 2 SOL minimum
// const MAX_PERMITTED_DATA_INCREASE: usize = 10 * 1024; // 10KB per transaction

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
    
    println!("  ↳ Program ID: {program_id}");
    
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
    
    println!("\nProgram ID:        {program_id}");
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

/// Verify that a program can be deployed (doesn't exist or is upgradeable)
fn verify_can_deploy(
    rpc_client: &RpcClient,
    program_id: &Pubkey,
    upgrade_authority: &Keypair,
) -> Result<bool> {
    match rpc_client.get_account(program_id) {
        Ok(account) => {
            let loader_id_sdk = Pubkey::new_from_array(LOADER_ID.to_bytes());
            // Program exists - verify it's upgradeable
            if account.owner != loader_id_sdk {
                return Err(anyhow!(
                    "Program {program_id} exists but is not an upgradeable program"
                ));
            }

            // Check if this is a valid program
            match bincode::deserialize::<UpgradeableLoaderState>(&account.data) {
                Ok(UpgradeableLoaderState::Program {
                    programdata_address,
                }) => {
                    // Get ProgramData to check authority
                    let programdata_address_pk = solana_sdk::pubkey::Pubkey::new_from_array(programdata_address.to_bytes());
                    let programdata = rpc_client.get_account(&programdata_address_pk)?;
                    match bincode::deserialize::<UpgradeableLoaderState>(&programdata.data)? {
                        UpgradeableLoaderState::ProgramData {
                            upgrade_authority_address,
                            ..
                        } => {
                            if upgrade_authority_address.is_none() {
                                return Err(anyhow!(
                                    "Program {program_id} is immutable and cannot be upgraded"
                                ));
                            }
                            let expected_authority_v2 = SolanaPubkeyV2::new_from_array(upgrade_authority.pubkey().to_bytes());
                            if upgrade_authority_address != Some(expected_authority_v2) {
                                return Err(anyhow!(
                                    "Authority mismatch. Expected {}, found {:?}",
                                    upgrade_authority.pubkey(),
                                    upgrade_authority_address
                                ));
                            }
                            Ok(true) // Exists and can upgrade
                        }
                        _ => Err(anyhow!("Invalid ProgramData state")),
                    }
                }
                _ => Err(anyhow!("Invalid program account")),
            }
        }
        Err(_) => Ok(false), // Program doesn't exist - can deploy
    }
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

    println!("\n🔍 Verifying deployment prerequisites...");
    
    // Check if program already exists and validate upgrade authority
    let program_exists = verify_can_deploy(rpc_client, &program_id, deployer)?;
    
    if program_exists {
        println!("  ⚠️  Program already exists - this will be an upgrade");
        return upgrade_program_bpf_upgradeable(
            rpc_client,
            deployer,
            &program_id,
            program_data,
        )
        .await;
    }
    
    println!("  ✓ Program does not exist - proceeding with fresh deployment");
    
    println!("\n Creating program buffer...");
    
    let buffer_keypair = Keypair::new();
    let buffer_pubkey = buffer_keypair.pubkey();
    
    // Calculate required size for buffer
    let buffer_size = UpgradeableLoaderState::size_of_buffer(program_data.len());
    let buffer_lamports = rpc_client
        .get_minimum_balance_for_rent_exemption(buffer_size)
        .context("Failed to get rent exemption for buffer")?;
    
    let deployer_addr = Address::from(deployer_pubkey.to_bytes());
    let buffer_addr = Address::from(buffer_pubkey.to_bytes());
    let loader_addr = Address::from(LOADER_ID.to_bytes());

    // Create buffer account
    let create_buffer_ix = system_instruction::create_account(
        &deployer_addr,
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
        Some(&deployer_pubkey),
    );
    transaction.sign(&[deployer, &buffer_keypair], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to create buffer account")?;
    
    println!("  ✓ Buffer created: {signature}");
    println!("  ↳ Buffer address: {buffer_pubkey}");
    
    println!("\n Writing program data to buffer...");
    
    write_program_data_chunked(
        rpc_client,
        deployer,
        &buffer_pubkey,
        program_data,
        true,
    )
        .await
        .context("Failed to write program data")?;
    
    println!("\n Deploying program from buffer...");
   
    
    // Calculate program account size
    let program_data_len = program_data.len();
    let max_data_len = program_data_len * 3;
    let programdata_size = UpgradeableLoaderState::size_of_programdata(max_data_len);
    let programdata_lamports = rpc_client
        .get_minimum_balance_for_rent_exemption(programdata_size)
        .context("Failed to get rent exemption for program data")?;
    
    // Derive ProgramData address
    let loader_id_sdk = Pubkey::new_from_array(LOADER_ID.to_bytes());
    let (programdata_address, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &loader_id_sdk,
    );
    
    let deployer_v2 = SolanaPubkeyV2::new_from_array(deployer_pubkey.to_bytes());
    let programdata_v2 = SolanaPubkeyV2::new_from_array(programdata_address.to_bytes());
    let buffer_v2 = SolanaPubkeyV2::new_from_array(buffer_pubkey.to_bytes());
    let program_v2 = SolanaPubkeyV2::new_from_array(program_id.to_bytes());


    // Deploy with upgradeable loader
    let deploy_instructions = bpf_loader_upgradeable::deploy_with_max_program_len(
        &deployer_v2,
        &programdata_v2,
        &buffer_v2,
        &program_v2,
        programdata_lamports,
        program_data_len * 2,
    )?;
    
    // Convert to solana_sdk::Instruction
    let sdk_instructions: Vec<SdkInstruction> = deploy_instructions
        .into_iter()
        .map(|ix| SdkInstruction {
            program_id: Pubkey::from(ix.program_id.to_bytes()),
            accounts: ix
                .accounts
                .iter()
                .map(|acc| AccountMeta {
                    pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                })
                .collect(),
            data: ix.data,
        })
        .collect();

    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &sdk_instructions,
        Some(&deployer_pubkey),
    );
    transaction.sign(&[deployer, program_keypair], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .context("Failed to deploy program")?;
    
    println!("  Program deployed: {signature}");
    println!("  ↳ ProgramData address: {programdata_address}");

    // Get program name from the current directory or Cargo.toml
    let lib_name = get_program_lib_name()?;
    
    // Deploy IDL if available
    deploy_idl_if_available(&program_id, &lib_name).await?;
    
    Ok(())
}


/// Write program data to buffer account in chunks
/// 
/// Large programs can't be written in a single transaction due to transaction size limits.
/// This function writes data in chunks using bpf_loader_upgradeable::write instruction.
#[deprecated]
#[allow(dead_code)]
async fn write_program_data_to_buffer(
    rpc_client: &RpcClient,
    deployer: &Keypair,
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
        let deployer_v2 = SolanaPubkeyV2::new_from_array(deployer.pubkey().to_bytes());
        
        let write_ix = bpf_loader_upgradeable::write(
            &buffer_v2,
            &deployer_v2,
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