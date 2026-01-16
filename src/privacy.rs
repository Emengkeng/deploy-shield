use anyhow::{Context, Result};
use light_client::rpc::RpcConnection;
use light_sdk::transfer::{compress_sol, decompress_sol};
use light_client::indexer::Indexer;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::thread;
use std::time::Duration;

const MIN_POOL_TVL: u64 = 100_000_000_000; // 100 SOL minimum for meaningful privacy
const SHIELD_DELAY_SECS: u64 = 30;

/// Privacy layer using Light Protocol's ZK Compression
/// 
/// Light Protocol stores SOL as compressed accounts in Merkle trees.
/// Compress = move SOL into compressed state (generates ZK proof)
/// Decompress = move SOL out of compressed state (generates ZK proof)
pub struct PrivacyLayer {
    rpc_client: RpcClient,
}

impl PrivacyLayer {
    pub fn new(rpc_url: &str) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.to_string(),
            CommitmentConfig::confirmed(),
        );
        Self { rpc_client }
    }

    /// Compress SOL using Light Protocol
    /// 
    /// This moves SOL from a regular Solana account into a compressed account.
    /// The compressed account is stored as a hash in a Merkle tree (no rent required).
    pub async fn compress_sol(
        &self,
        from_keypair: &Keypair,
        amount_lamports: u64,
    ) -> Result<String> {
        println!("\n Compressing {} SOL through Light Protocol...", 
            amount_lamports as f64 / 1_000_000_000.0);

        let signature = compress_sol(
            &self.rpc_connection,
            from_keypair,
            amount_lamports,
        )
        .await
        .context("Failed to compress SOL")?;

        println!("Compression transaction confirmed");
        Ok(signature.to_string())
    }

    /// Decompress SOL to burner wallet
    /// 
    /// This moves SOL from a compressed account back to a regular Solana account.
    /// The burner receives the SOL and can use it for deployments.
    pub async fn decompress_sol(
        &self,
        to_pubkey: &Pubkey,
        amount_lamports: u64,
    ) -> Result<String> {
        println!("\n Applying privacy delay ({} seconds)...", SHIELD_DELAY_SECS);
        thread::sleep(Duration::from_secs(SHIELD_DELAY_SECS));

        println!("Decompressing to deployer...");

        let signature = decompress_sol(
            &self.rpc_connection,
            to_pubkey,
            amount_lamports,
        )
        .await
        .context("Failed to decompress SOL")?;

        println!("Decompression transaction confirmed");
        Ok(signature.to_string())
    }

    /// Check if Light Protocol's Merkle trees have sufficient activity
    /// 
    /// Privacy depends on the anonymity set - how many other users are using the system.
    /// A larger anonymity set = better privacy.
    pub fn check_anonymity_set(&self) -> Result<bool> {

        println!("\n Checking Light Protocol anonymity set...");

        let indexer = Indexer::new(&self.rpc_connection.url())
            .context("Failed to create indexer")?;

        let state_trees = indexer
            .get_state_merkle_tree_accounts()
            .context("Failed to query state trees")?;

        let total_accounts: u64 = state_trees
            .iter()
            .map(|tree| tree.next_index)
            .sum();

        const MIN_ACCOUNTS: u64 = 1000;

        if total_accounts < MIN_ACCOUNTS {
            println!("\n  Warning: Low anonymity set detected");
            println!("   Active accounts: {}", total_accounts);
            println!("   Recommended: {}", MIN_ACCOUNTS);
            println!("   Privacy guarantees may be weaker.");
            return Ok(false);
        }

        println!(" Anonymity set is adequate");
        println!("  ↳ Active accounts: {}", total_accounts);
        Ok(true)
    }

    /// Round amount to prevent correlation attacks
    /// 
    /// If you compress 6.7291 SOL and someone decompresses 6.7291 SOL,
    /// that's linkable even with ZK proofs. Rounding breaks this.
    pub fn round_amount(amount_lamports: u64) -> u64 {
        let sol = amount_lamports as f64 / 1_000_000_000.0;
        let rounded_sol = (sol * 10.0).round() / 10.0;
        (rounded_sol * 1_000_000_000.0) as u64
    }
}