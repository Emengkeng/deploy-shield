use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::{signature::Keypair, pubkey::Pubkey};
use std::str::FromStr;
use std::fs;
use std::path::{Path, PathBuf};

const SHIELD_DIR: &str = ".shield";
const DEPLOYER_FILE: &str = "deployer.json";
const STATE_FILE: &str = "state.json";

#[derive(Serialize, Deserialize)]
pub struct DeployerKeypair {
    pub keypair: Vec<u8>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ProjectState {
    pub network: String,
    pub deployed_programs: Vec<DeployedProgram>,
    pub last_balance: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeployedProgram {
    pub program_id: String,
    pub deployed_at: i64,
    pub last_upgraded: Option<i64>,
}

pub struct Config {
    shield_dir: PathBuf,
}

impl Config {
    pub fn new() -> Result<Self> {
        let shield_dir = PathBuf::from(SHIELD_DIR);
        Ok(Self { shield_dir })
    }

    /// Validate that all deployed programs still exist and are accessible
    #[allow(dead_code)]
    pub fn validate_deployed_programs(&self) -> Result<Vec<String>> {
        let state = self.load_state()?;
        let mut warnings = Vec::new();
        
        if !state.deployed_programs.is_empty() {
            let rpc_url = crate::utils::get_rpc_url()?;
            let rpc_client = RpcClient::new_with_commitment(
                rpc_url,
                CommitmentConfig::confirmed(),
            );
            
            for program in &state.deployed_programs {
                let program_id = Pubkey::from_str(&program.program_id)?;
                
                match rpc_client.get_account(&program_id) {
                    Ok(_) => {
                        // Program exists - good
                    }
                    Err(_) => {
                        warnings.push(format!(
                            "Warning: Program {} not found on-chain (may have been closed)",
                            program.program_id
                        ));
                    }
                }
            }
        }
        
        Ok(warnings)
    }

    pub fn ensure_shield_dir(&self) -> Result<()> {
        if !self.shield_dir.exists() {
            fs::create_dir_all(&self.shield_dir)
                .context("Failed to create .shield directory")?;
        }
        Ok(())
    }

    pub fn deployer_path(&self) -> PathBuf {
        self.shield_dir.join(DEPLOYER_FILE)
    }

    pub fn state_path(&self) -> PathBuf {
        self.shield_dir.join(STATE_FILE)
    }

    pub fn deployer_exists(&self) -> bool {
        self.deployer_path().exists()
    }

    pub fn save_deployer(&self, keypair: &Keypair) -> Result<()> {
        self.ensure_shield_dir()?;
        
        let deployer_data = DeployerKeypair {
            keypair: keypair.to_bytes().to_vec(),
        };
        
        let json = serde_json::to_string_pretty(&deployer_data)?;
        fs::write(self.deployer_path(), json)
            .context("Failed to write deployer keypair")?;
        
        Ok(())
    }

    pub fn load_deployer(&self) -> Result<Keypair> {
        let json = fs::read_to_string(self.deployer_path())
            .context("Failed to read deployer keypair")?;
        
        let data: DeployerKeypair = serde_json::from_str(&json)?;
        
        let keypair = Keypair::from_bytes(&data.keypair)
            .map_err(|e| anyhow::anyhow!("Invalid keypair: {e}"))?;
        
        Ok(keypair)
    }

    pub fn load_state(&self) -> Result<ProjectState> {
        if !self.state_path().exists() {
            return Ok(ProjectState::default());
        }
        
        let json = fs::read_to_string(self.state_path())
            .context("Failed to read state")?;
        
        let state: ProjectState = serde_json::from_str(&json)?;
        Ok(state)
    }

    pub fn save_state(&self, state: &ProjectState) -> Result<()> {
        self.ensure_shield_dir()?;
        
        let json = serde_json::to_string_pretty(state)?;
        fs::write(self.state_path(), json)
            .context("Failed to write state")?;
        
        Ok(())
    }

    pub fn add_gitignore(&self) -> Result<()> {
        let gitignore_path = Path::new(".gitignore");
        let shield_entry = ".shield/\n";
        
        if gitignore_path.exists() {
            let content = fs::read_to_string(gitignore_path)?;
            if !content.contains(".shield") {
                fs::write(gitignore_path, format!("{content}{shield_entry}"))?;
            }
        } else {
            fs::write(gitignore_path, shield_entry)?;
        }
        
        Ok(())
    }
}