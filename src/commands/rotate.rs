use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_loader_v3_interface::instruction as bpf_loader_upgradeable;
use solana_sdk_ids::bpf_loader_upgradeable::ID as LOADER_ID;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::{
    pubkey::Pubkey, 
    signature::{Keypair, Signer}, 
    transaction::Transaction,
    instruction::Instruction as SdkInstruction,
    instruction::AccountMeta,
};
use solana_pubkey::Pubkey as SolanaPubkeyV2;
use std::str::FromStr;
use crate::config::Config;
use crate::utils::*;

pub async fn execute() -> Result<()> {
    print_header("Rotate Deployer");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        anyhow::bail!(
            "No private deployer found.\n\
            Run `shield-deploy init` first."
        );
    }
    
    let old_deployer = config.load_deployer()?;
    let state = config.load_state()?;
    
    println!("\nThis will:");
    println!("• Create a new private deployer");
    println!("• Transfer upgrade authority");
    println!("• Retire the old deployer");
    
    println!("\nRecommended only if:");
    println!("• You suspect key exposure");
    println!("• You are handing off control\n");
    
    if !prompt_confirmation("Proceed?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    let new_deployer = Keypair::new();
    
    println!("\n✓ New deployer generated");
    println!("  ↳ New deployer pubkey: {}", new_deployer.pubkey());
    
    // Check if any programs need authority transfer
    if !state.deployed_programs.is_empty() {
        println!("\n⏳ Transferring upgrade authority...");
        
        let rpc_url = get_rpc_url()?;
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        
        for program in &state.deployed_programs {
            let program_id = Pubkey::from_str(&program.program_id)
                .context("Invalid program ID")?;
            
            transfer_upgrade_authority(
                &rpc_client,
                &old_deployer,
                &program_id,
                &new_deployer.pubkey(),
            )
            .await
            .context(format!("Failed to transfer authority for {program_id}"))?;
            
            println!("  ✓ Authority transferred for {program_id}");
        }
    } else {
        println!("\n  ↳ No deployed programs, skipping authority transfer");
    }
    
    config.save_deployer(&new_deployer)
        .context("Failed to save new deployer")?;
    
    print_success("Deployer rotated");
    
    println!("\nNew deployer is now active.");
    println!("Old deployer can be safely discarded.");
    
    println!("\n⚠️  Important:");
    println!("  • Back up the new deployer key");
    println!("  • Update any external references");
    println!("  • Securely delete old deployer backups");
    
    Ok(())
}

/// Transfer upgrade authority from old deployer to new deployer
async fn transfer_upgrade_authority(
    rpc_client: &RpcClient,
    current_authority: &Keypair,
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
    
    // Create set_upgrade_authority instruction (returns privacy_cash::Instruction)
    let set_authority_ix = bpf_loader_upgradeable::set_upgrade_authority(
        &programdata_v2,
        &current_authority_v2,
        Some(&new_authority_v2),
    );
    
    // Convert to solana_sdk::Instruction for the transaction
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