use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    instruction as bpf_loader_upgradeable,
    state::UpgradeableLoaderState,
};
use solana_sdk_ids::bpf_loader_upgradeable::ID as LOADER_ID;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signer,
    transaction::Transaction,
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
    commitment_config::CommitmentConfig
};
use solana_pubkey::Pubkey as SolanaPubkeyV2;
use std::str::FromStr;
use crate::config::Config;
use crate::utils::*;

pub async fn execute(program_id_str: String) -> Result<()> {
    print_header("Finalize Program (Make Immutable)");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        anyhow::bail!(
            "No private deployer found.\n\
            Run `shield-deploy init` first."
        );
    }
    
    let deployer = config.load_deployer()?;
    let state = config.load_state()?;
    
    let program_id = Pubkey::from_str(&program_id_str)
        .context("Invalid program ID")?;
    
    // Check if this is one of our deployed programs
    let program_info = state.deployed_programs
        .iter()
        .find(|p| p.program_id == program_id_str);
    
    if program_info.is_none() {
        print_warning("This program was not deployed by Shield-Deploy");
        println!("  You can still finalize it if you control the authority.");
        println!();
    }
    
    println!("\n⚠️  ⚠️  ⚠️  CRITICAL WARNING ⚠️  ⚠️  ⚠️\n");
    println!("This will make the program PERMANENTLY IMMUTABLE.");
    println!();
    println!("After this operation:");
    println!("  • NO ONE can upgrade this program (including you)");
    println!("  • NO ONE can fix bugs in the code");
    println!("  • NO ONE can add new features");
    println!("  • This operation CANNOT BE UNDONE");
    println!();
    println!("Program ID: {program_id}");
    println!();
    println!("Only proceed if:");
    println!("  ✓ The program has been thoroughly audited");
    println!("  ✓ All tests pass");
    println!("  ✓ The code is production-ready");
    println!("  ✓ You understand the consequences");
    println!("  ✓ There are NO bugs or security issues");
    println!();
    
    if !prompt_confirmation("I understand this is PERMANENT and IRREVERSIBLE")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    println!();
    print_warning("FINAL CONFIRMATION");
    println!("Type the program ID to confirm finalization:");
    println!("{program_id}");
    println!();
    
    let confirmation: String = dialoguer::Input::new()
        .with_prompt("Program ID")
        .interact_text()?;
    
    if confirmation.trim() != program_id_str {
        anyhow::bail!("Program ID mismatch. Finalization cancelled.");
    }
    
    let rpc_url = get_rpc_url()?;
    let rpc_client = RpcClient::new_with_commitment(
        rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    
    println!("\n Finalizing program (making immutable)...");
    
    finalize_program(
        &rpc_client,
        &deployer,
        &program_id,
    )
    .await
    .context("Failed to finalize program")?;
    
    print_success("Program is now IMMUTABLE");
    
    println!("\nProgram ID: {program_id}");
    println!("Upgrade authority: None (immutable)");
    println!();
    println!("⚠️  Important:");
    println!("  • This program can NEVER be upgraded");
    println!("  • If bugs are found, you must deploy a NEW program");
    println!("  • Users must migrate to the new program");
    println!("  • Keep the source code as the only way to verify behavior");
    println!();
    println!("✓ The program is now trustless and verifiable");
    
    Ok(())
}

/// Make a program immutable by setting upgrade authority to None
/// 
/// This is IRREVERSIBLE. After this, NO ONE can upgrade the program.
async fn finalize_program(
    rpc_client: &RpcClient,
    current_authority: &solana_sdk::signature::Keypair,
    program_id: &Pubkey,
) -> Result<()> {
    // Derive ProgramData address
    let loader_id_sdk = Pubkey::new_from_array(LOADER_ID.to_bytes());
    let (programdata_address, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &loader_id_sdk,
    );
    
    println!("  ↳ ProgramData: {programdata_address}");

    // Verify we currently control this program
    verify_current_authority(rpc_client, &programdata_address, current_authority)
        .await?;
    
    // Create set_upgrade_authority instruction with None
    // This is THE KEY DIFFERENCE - None instead of Some(pubkey)
    let programdata_v2 = SolanaPubkeyV2::new_from_array(programdata_address.to_bytes());
    let current_authority_v2 = SolanaPubkeyV2::new_from_array(current_authority.pubkey().to_bytes());

    let set_authority_ix = bpf_loader_upgradeable::set_upgrade_authority(
        &programdata_v2,
        &current_authority_v2,
        None,
    );

    let sdk_instruction = SdkInstruction {
        program_id: Pubkey::from(set_authority_ix.program_id.to_bytes()),
        accounts: set_authority_ix
            .accounts
            .iter()
            .map(|acc| AccountMeta {
                pubkey: Pubkey::from(acc.pubkey.to_bytes()),
                is_signer: acc.is_signer,
                is_writable: acc.is_writable,
            })
            .collect(),
        data: set_authority_ix.data,
    };
    
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let mut transaction = Transaction::new_with_payer(
        &[sdk_instruction],
        Some(&current_authority.pubkey()),
    );
    transaction.sign(&[current_authority], recent_blockhash);
    
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to finalize program")?;
    
    println!("  ✓ Transaction confirmed: {signature}");
    
    verify_immutable(rpc_client, &programdata_address).await?;
    
    Ok(())
}

/// Verify we control the program before finalizing
async fn verify_current_authority(
    rpc_client: &RpcClient,
    programdata_address: &Pubkey,
    expected_authority: &solana_sdk::signature::Keypair,
) -> Result<()> {
    let account = rpc_client
        .get_account(programdata_address)
        .context("ProgramData account not found")?;
    
    let programdata_state: UpgradeableLoaderState = bincode::deserialize(&account.data)
        .context("Failed to deserialize ProgramData")?;
    
    match programdata_state {
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address,
            slot: _,
        } => {
            if let Some(authority) = upgrade_authority_address {
                let expected_pubkey = expected_authority.pubkey();
                
                if authority.to_bytes() == expected_pubkey.to_bytes() {
                    println!("  ✓ Authority verified: you control this program");
                    Ok(())
                } else {
                    anyhow::bail!(
                        "Authority mismatch.\n\
                        Expected: {expected_pubkey}\n\
                        Found: {authority}\n\
                        You do not control this program."
                    )
                }
            } else {
                anyhow::bail!("Program is already immutable")
            }
        }
        _ => anyhow::bail!("Invalid ProgramData account state"),
    }
}

/// Verify the program is now immutable
async fn verify_immutable(
    rpc_client: &RpcClient,
    programdata_address: &Pubkey,
) -> Result<()> {
    let account = rpc_client
        .get_account(programdata_address)
        .context("ProgramData account not found")?;
    
    let programdata_state: UpgradeableLoaderState = bincode::deserialize(&account.data)
        .context("Failed to deserialize ProgramData")?;
    
    match programdata_state {
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address,
            slot: _,
        } => {
            if upgrade_authority_address.is_none() {
                println!("  ✓ Verified: Program is now immutable");
                Ok(())
            } else {
                anyhow::bail!(
                    "Finalization failed: authority is still set to {upgrade_authority_address:?}"
                )
            }
        }
        _ => anyhow::bail!("Invalid ProgramData account state"),
    }
}