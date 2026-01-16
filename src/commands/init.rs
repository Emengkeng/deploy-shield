use anyhow::{Context, Result};
use solana_sdk::signature::Keypair;
use crate::config::Config;
use crate::utils::{print_header, print_success, prompt_confirmation};

pub async fn execute() -> Result<()> {
    print_header("Shield-Deploy");
    
    let config = Config::new()?;
    
    // Check if already initialized
    if config.deployer_exists() {
        anyhow::bail!(
            "Private deployer already exists.\n\
            Run `shield-deploy status` to view current state.\n\
            Run `shield-deploy rotate` to create a new deployer."
        );
    }
    
    println!("\nThis will create a private deployer for this project.\n");
    println!("• The deployer will fund and upgrade your program");
    println!("• Your main wallet will never deploy directly");
    println!("• The deployer key stays on this machine\n");
    
    if !prompt_confirmation("Proceed?")? {
        println!("Cancelled.");
        return Ok(());
    }
    
    // Generate new burner keypair
    let deployer = Keypair::new();
    
    // Save deployer
    config.save_deployer(&deployer)
        .context("Failed to save deployer")?;
    
    // Add to .gitignore
    config.add_gitignore()
        .context("Failed to update .gitignore")?;
    
    // Initialize state
    let state = crate::config::ProjectState {
        network: crate::utils::get_network_name(),
        deployed_programs: vec![],
        last_balance: 0,
    };
    config.save_state(&state)?;
    
    print_success("Private deployer created");
    
    println!("\nProject:        {}", 
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("Deployer:       project burner");
    println!("Location:       .shield/deployer.json");
    
    println!("\nNext step:");
    println!("→ Fund the deployer with SOL using `shield-deploy fund`");
    
    Ok(())
}