use anyhow::{Context, Result};
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use crate::config::Config;
use crate::privacy::PrivacyLayer;
use crate::utils::*;

pub async fn execute() -> Result<()> {
    print_header("Fund Private Deployer");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        anyhow::bail!(
            "No private deployer found.\n\
            Run `shield-deploy init` first."
        );
    }
    
    let deployer = config.load_deployer()?;
    
    println!();
    let amount_sol = prompt_amount("Amount to fund (SOL)")?;
    let amount_lamports = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;
    
    // Round to prevent correlation
    let rounded_lamports = PrivacyLayer::round_amount(amount_lamports);
    let rounded_sol = rounded_lamports as f64 / LAMPORTS_PER_SOL as f64;
    
    if rounded_lamports != amount_lamports {
        println!("\nAmount rounded to {} SOL for privacy", rounded_sol);
    }
    
    println!();
    let wallet_choice = prompt_funding_wallet()?;
    
    println!("\nFunding wallet:");
    println!("• This wallet will only sign the funding transaction");
    println!("• It will not be stored or linked to the project\n");
    
    if !prompt_confirmation("Continue?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    let funding_keypair = load_funding_keypair(wallet_choice)
        .context("Failed to load funding wallet")?;
    
    let rpc_url = get_rpc_url()?;
    let privacy = PrivacyLayer::new(&rpc_url);
    
    privacy.check_pool_anonymity_set()?;
    
    // Shield funds
    let _shield_sig = privacy.shield_sol(&funding_keypair, rounded_lamports).await
        .context("Failed to shield funds")?;
    
    // Unshield to deployer
    let _unshield_sig = privacy.unshield_sol(&deployer.pubkey(), rounded_lamports).await
        .context("Failed to unshield funds")?;
    
    print_success("Funding complete");
    
    println!("\nDeployer balance updated.");
    println!("Your funding wallet is no longer used.");
    
    println!("\nNext step:");
    println!("→ Deploy using `shield-deploy deploy`");
    
    Ok(())
}