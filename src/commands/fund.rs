use anyhow::{Context, Result};
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signer::Signer;
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
    
    if amount_sol < 0.02 {
        print_warning(&format!(
            "Privacy Cash requires minimum 0.02 SOL\n\
            You entered: {amount_sol} SOL"
        ));
        println!("\n💡 Tip: Use at least 0.02 SOL for ZK-proof privacy");
        println!("   Or use 0.02 SOL as minimum for best privacy\n");
        
        if !prompt_confirmation("Continue anyway (will adjust to minimum)?")? {
            println!("Cancelled.");
            return Ok(());
        }
    }
    
    let amount_lamports = (amount_sol.max(0.02) * LAMPORTS_PER_SOL as f64) as u64;
    
    let rounded_lamports = PrivacyLayer::round_amount(amount_lamports);
    let rounded_sol = rounded_lamports as f64 / LAMPORTS_PER_SOL as f64;
    
    if rounded_lamports != amount_lamports {
        println!("\n💡 Amount adjusted to {rounded_sol} SOL");
        println!("   (Privacy Cash minimum: 0.02 SOL)");
    }
    
    println!();
    let wallet_choice = prompt_funding_wallet()?;
    
    println!("\n🔒 Privacy Cash - Zero-Knowledge Proof Transfer");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Privacy technique: Groth16 ZK Proofs");
    println!();
    println!("What happens:");
    println!("  1. Your wallet deposits to Privacy Cash pool (visible)");
    println!("  2. ZK proof generated client-side (Groth16)");
    println!("  3. Burner receives from pool (amount HIDDEN!)");
    println!("  4. No on-chain link between your wallet and burner");
    println!();
    println!("Privacy level: VERY HIGH");
    println!("  ✓ Withdraw amount is hidden (ZK proof)");
    println!("  ✓ No direct wallet → burner link");
    println!("  ✓ Privacy Cash pool breaks connection");
    println!();
    println!("Fees: ~0.006 SOL (Privacy Cash network fee)");
    println!();
    println!("Requirements:");
    println!("  • Circuit files in ./circuit/ directory");
    println!("  • Minimum 0.02 SOL");
    println!("  • Sufficient balance for amount + fees");
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if !prompt_confirmation("\nContinue with Privacy Cash transfer?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    // Load funding keypair
    let funding_keypair = load_funding_keypair(wallet_choice)
        .context("Failed to load funding wallet")?;
    
    println!("\n💰 Funding wallet: {}", funding_keypair.pubkey());
    println!("🎯 Burner wallet: {}", deployer.pubkey());
    println!("💸 Amount: {rounded_sol} SOL");
    
    // Initialize privacy layer
    let rpc_url = get_rpc_url()?;
    let privacy = PrivacyLayer::new(&rpc_url);
    
    println!("\n🚀 Starting Privacy Cash transfer...");
    println!("   (This may take 10-30 seconds for ZK proof generation)");
    
    // Execute private transfer via Privacy Cash
    let result = privacy.fund_burner_private(
        &funding_keypair,
        &deployer.pubkey(),
        rounded_sol,
    )
    .await
    .context("Privacy Cash transfer failed")?;
    
    // Apply additional privacy delay
    privacy.apply_privacy_delay().await;
    
    print_success("Privacy Cash funding complete!");
    
    println!("\n📝 Transaction Summary:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Deposit TX:   {}", result.deposit_signature);
    println!("Withdraw TX:  {} (amount hidden!)", result.withdraw_signature);
    println!();
    println!("Amount deposited: {} SOL", 
        result.amount_deposited as f64 / LAMPORTS_PER_SOL as f64);
    println!("Amount received:  {} SOL (hidden on-chain)", 
        result.amount_received as f64 / LAMPORTS_PER_SOL as f64);
    println!("Privacy Cash fee: {} SOL", 
        result.total_fees as f64 / LAMPORTS_PER_SOL as f64);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    println!("\n🎉 Privacy achieved!");
    println!("  ✓ Withdraw amount hidden via ZK proof");
    println!("  ✓ No on-chain link to funding wallet");
    println!("  ✓ Burner wallet ready for anonymous deployment");
    
    println!("\n💡 Privacy tips:");
    println!("  • The funding wallet is no longer needed");
    println!("  • Burner appears as random Privacy Cash user");
    println!("  • Use privacy-focused RPC for extra privacy");
    
    println!("\nNext step:");
    println!("→ Deploy using `shield-deploy deploy`");
    
    Ok(())
}