use anyhow::{Context, Result};
use privacy_cash::{send_privately, SendPrivatelyResult};
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair},
};
use std::thread;
use std::time::Duration;

const PRIVACY_DELAY_SECS: u64 = 30;

/// Privacy layer using Privacy Cash for ZK-proof private transfers
/// 
/// Privacy Cash uses Groth16 zero-knowledge proofs to provide complete
/// transaction privacy on Solana. When you send funds through Privacy Cash:
/// 
/// 1. Deposit to Privacy Cash pool (visible on-chain)
/// 2. ZK proof generated client-side (Groth16)
/// 3. Withdraw to recipient using ZK proof (amount & sender hidden!)
/// 
/// Privacy Model:
/// ==============
/// - Deposit amount: Visible on-chain
/// - Withdraw amount: HIDDEN (ZK proof)
/// - Sender → Recipient link: BROKEN (privacy pool)
/// - Privacy level: VERY HIGH
/// 
/// How This Works for Shield-Deploy:
/// =================================
/// 
/// Funding wallet → Privacy Cash pool (deposit visible)
///      ↓ [Privacy Cash internal transfer]
/// Privacy Cash pool → Burner wallet (withdraw amount HIDDEN!)
///      ↓ [30s delay]
/// Burner wallet → Deploy program (no link to funding wallet)
/// 
/// On-chain observers see:
/// - Funding wallet deposited to Privacy Cash ✓
/// - Someone withdrew from Privacy Cash ✓
/// - But: Amount withdrawn is HIDDEN
/// - And: No link between deposit and withdraw
/// 
/// Result: Burner wallet appears to have random funds from Privacy Cash
pub struct PrivacyLayer {
    rpc_url: Option<String>,
}

impl PrivacyLayer {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_url: Some(rpc_url.to_string()),
        }
    }

    /// Fund burner wallet using Privacy Cash ZK-proof protocol
    /// 
    /// This performs a privacy-preserving transfer using Groth16 ZK proofs.
    /// The amount transferred to the burner is hidden on-chain.
    /// 
    /// Steps:
    /// 1. Deposit from funding wallet to Privacy Cash (visible)
    /// 2. Generate Groth16 ZK proof (client-side)
    /// 3. Withdraw to burner wallet with ZK proof (amount hidden!)
    /// 
    /// Privacy guarantees:
    /// - Withdraw amount is HIDDEN on-chain
    /// - No direct link between funding wallet and burner
    /// - Privacy Cash pool acts as mixing service
    /// 
    /// Requirements:
    /// - Minimum amounts: 0.02 SOL, 2 USDC, 2 USDT
    /// - Circuit files must be in ./circuit/ directory
    /// - Funding wallet must have sufficient balance + fees (~0.006 SOL fee)
    pub async fn fund_burner_private(
        &self,
        funding_keypair: &Keypair,
        burner_pubkey: &Pubkey,
        amount_sol: f64,
    ) -> Result<SendPrivatelyResult> {
        println!("\n🔒 Funding burner via Privacy Cash (ZK-proof private transfer)...");
        println!("  ↳ Amount: {amount_sol} SOL");
        println!("  ↳ Privacy: Groth16 zero-knowledge proofs");
        println!("  ↳ Withdraw amount will be HIDDEN on-chain");
        
        // Check minimum amount
        if amount_sol < 0.02 {
            anyhow::bail!(
                "Privacy Cash requires minimum 0.02 SOL\n\
                You specified: {amount_sol} SOL\n\
                Please fund at least 0.02 SOL for privacy"
            );
        }
        
        // Convert keypair to base58 private key
        let private_key_bytes = funding_keypair.to_bytes();
        let private_key_base58 = bs58::encode(&private_key_bytes).into_string();
        
        println!("\n📝 Generating ZK proof (Groth16)...");
        println!("  ↳ This may take a few seconds");
        println!("  ↳ Proof generated client-side (secure)");
        
        // Use Privacy Cash's send_privately() - ONE FUNCTION DOES EVERYTHING!
        let result = send_privately(
            &private_key_base58,
            &burner_pubkey.to_string(),
            amount_sol,
            "sol",
            self.rpc_url.as_deref(),
        )
        .await
        .context("Privacy Cash transfer failed")?;
        
        println!("\n✅ Privacy Cash transfer complete!");
        println!("  ↳ Deposit TX: {}", result.deposit_signature);
        println!("  ↳ Withdraw TX: {} (amount HIDDEN!)", result.withdraw_signature);
        println!("\n💰 Transaction details:");
        println!("  ↳ Deposited: {} SOL", 
            result.amount_deposited as f64 / LAMPORTS_PER_SOL as f64);
        println!("  ↳ Received: {} SOL (hidden from chain observers)", 
            result.amount_received as f64 / LAMPORTS_PER_SOL as f64);
        println!("  ↳ Total fees: {} SOL", 
            result.total_fees as f64 / LAMPORTS_PER_SOL as f64);
        
        println!("\n🎉 Burner wallet funded privately!");
        println!("  ↳ On-chain observers cannot see withdraw amount");
        println!("  ↳ No link between funding wallet and burner");
        println!("  ↳ Burner appears as random Privacy Cash user");
        
        Ok(result)
    }

    /// Apply privacy delay before burner's first deployment
    /// 
    /// This breaks timing correlation between:
    /// - Privacy Cash withdraw timestamp
    /// - First deployment timestamp
    /// 
    /// Without delay: "Privacy Cash withdraw at T, deploy at T+5s" = linkable
    /// With delay: "Privacy Cash withdraw at T, deploy at T+30s" = harder to link
    pub async fn apply_privacy_delay(&self) {
        println!("\n⏳ Applying privacy delay ({PRIVACY_DELAY_SECS} seconds)...");
        println!("  ↳ This breaks timing correlation");
        println!("  ↳ Makes linking withdraw → deploy harder");
        
        thread::sleep(Duration::from_secs(PRIVACY_DELAY_SECS));
        
        println!("  ✓ Privacy delay complete");
    }

    /// Round amount to recommended Privacy Cash minimums
    /// 
    /// Privacy Cash has minimum amounts:
    /// - SOL: 0.02 SOL
    /// - USDC: 2 USDC
    /// - USDT: 2 USDT
    /// 
    /// This rounds up to nearest valid amount for better privacy
    pub fn round_amount(amount_lamports: u64) -> u64 {
        let sol = amount_lamports as f64 / LAMPORTS_PER_SOL as f64;
        
        let final_sol = sol.max(0.02);
        // Round to 0.1 SOL, minimum 0.02
        // let rounded_sol = if sol < 0.02 {
        //     0.02
        // } else {
        //     (sol * 10.0).ceil() / 10.0
        // };
        
        (final_sol * LAMPORTS_PER_SOL as f64) as u64
    }
}