use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    signer::Signer
};
use solana_sdk::commitment_config::CommitmentConfig;
use crate::config::Config;
use crate::utils::*;

pub async fn execute() -> Result<()> {
    print_header("Shield-Deploy Status");
    
    let config = Config::new()?;
    
    if !config.deployer_exists() {
        println!("\nNo private deployer found.");
        println!("\nRun `shield-deploy init` to get started.");
        return Ok(());
    }
    
    let deployer = config.load_deployer()?;
    let state = config.load_state()?;
    
    let rpc_url = get_rpc_url()?;
    let rpc_client = RpcClient::new_with_commitment(
        rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );
    
    let balance = rpc_client.get_balance(&deployer.pubkey())
        .context("Failed to get deployer balance")?;
    
    let balance_status = if balance >= 5_000_000_000 {
        "sufficient"
    } else if balance >= 1_000_000_000 {
        "low"
    } else {
        "insufficient"
    };
    
    println!();
    println!("Project:        {}", 
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("Deployer:       active");
    println!("Network:        {}", state.network);
    println!("Balance:        {} ({})", format_sol(balance), balance_status);
    println!("Programs:       {} deployed", state.deployed_programs.len());
    
    if !state.deployed_programs.is_empty() {
        println!("\nDeployed Programs:");
        for (i, program) in state.deployed_programs.iter().enumerate() {
            println!("  {}. {}", i + 1, program.program_id);
            if let Some(upgraded) = program.last_upgraded {
                let datetime = chrono::DateTime::from_timestamp(upgraded, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!("     Last upgraded: {datetime}");
            }
        }
    }
    
    println!("\nPrivacy:");
    println!("• Main wallet not linked on-chain");
    println!("• Deployer authority active");
    
    if balance_status == "low" || balance_status == "insufficient" {
        println!("\n Low balance detected");
        println!("   Run `shield-deploy fund` to add more SOL");
    }
    
    Ok(())
}