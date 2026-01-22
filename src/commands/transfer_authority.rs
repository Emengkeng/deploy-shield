use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::{
    instruction as bpf_loader_upgradeable,
};
use solana_sdk_ids::bpf_loader_upgradeable::ID as LOADER_ID;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signer,
    transaction::Transaction,
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
};
use solana_pubkey::Pubkey as SolanaPubkeyV2;
use std::str::FromStr;
use crate::config::Config;
use crate::utils::*;

pub async fn execute(new_authority: String) -> Result<()> {
    print_header("Transfer Upgrade Authority");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        anyhow::bail!(
            "No private deployer found.\n\
            Run `shield-deploy init` first."
        );
    }
    
    let deployer = config.load_deployer()?;
    let state = config.load_state()?;
    
    let new_authority_pubkey = Pubkey::from_str(&new_authority)
        .context("Invalid public key for new authority")?;
    
    // Check if any programs deployed
    if state.deployed_programs.is_empty() {
        anyhow::bail!(
            "No programs deployed yet.\n\
            Run `shield-deploy deploy` first."
        );
    }
    
    println!("\nThis will transfer upgrade authority to:");
    println!("  {new_authority_pubkey}");
    
    println!("\nFor programs:");
    for (i, program) in state.deployed_programs.iter().enumerate() {
        println!("  {}. {}", i + 1, program.program_id);
    }
    
    println!("\n⚠️  Warning:");
    println!("• After transfer, the private deployer cannot upgrade these programs");
    println!("• This operation cannot be undone");
    println!("• Make sure you control the new authority\n");
    
    if !prompt_confirmation("Proceed?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    let rpc_url = get_rpc_url()?;
    let rpc_client = RpcClient::new_with_commitment(
        rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    
    println!("\n Transferring authority...");
    
    for program in &state.deployed_programs {
        let program_id = Pubkey::from_str(&program.program_id)
            .context("Invalid program ID")?;
        
        transfer_upgrade_authority(
            &rpc_client,
            &deployer,
            &program_id,
            &new_authority_pubkey,
        )
        .await
        .context(format!("Failed to transfer authority for {program_id}"))?;
        
        println!("  ✓ Authority transferred for {program_id}");
    }
    
    print_success("Authority transfer complete");
    
    println!("\nNew upgrade authority: {new_authority_pubkey}");
    println!("The private deployer no longer controls these programs.");
    
    println!("\n💡 Tip:");
    println!("  You can still use the deployer for new deployments.");
    println!("  Run `shield-deploy deploy` to deploy new programs.");
    
    Ok(())
}

/// Transfer upgrade authority to a new address
/// 
/// This uses bpf_loader_upgradeable::set_upgrade_authority to transfer control.
/// After this, only the new authority can upgrade the program.
async fn transfer_upgrade_authority(
    rpc_client: &RpcClient,
    current_authority: &solana_sdk::signature::Keypair,
    program_id: &Pubkey,
    new_authority: &Pubkey,
) -> Result<()> {
    // Derive ProgramData address
    let loader_id_sdk = Pubkey::new_from_array(LOADER_ID.to_bytes());
    let (programdata_address, _) = Pubkey::find_program_address(
        &[program_id.as_ref()],
        &loader_id_sdk,
    );
    
    // Convert to privacy_cash::Pubkey for the instruction builder
    let programdata_v2 = SolanaPubkeyV2::new_from_array(programdata_address.to_bytes());
    let current_authority_v2 = SolanaPubkeyV2::new_from_array(current_authority.pubkey().to_bytes());
    let new_authority_v2 = SolanaPubkeyV2::new_from_array(new_authority.to_bytes());
        
    // Create set_upgrade_authority instruction
    // Some(new_authority) = transfer to new_authority
    // None = make program immutable (not upgradeable)
    let set_authority_ix = bpf_loader_upgradeable::set_upgrade_authority(
        &programdata_v2,
        &current_authority_v2,
        Some(&new_authority_v2),
    );
    
    // Convert to solana_sdk::Instruction
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
        .context("Failed to transfer authority")?;
    
    println!("    ↳ Transaction: {signature}");
    
    Ok(())
}