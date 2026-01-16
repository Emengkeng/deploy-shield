use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
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
        
        let keypair = Keypair::try_from(data.keypair.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid keypair: {}", e))?;
        
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
                fs::write(gitignore_path, format!("{}{}", content, shield_entry))?;
            }
        } else {
            fs::write(gitignore_path, shield_entry)?;
        }
        
        Ok(())
    }
}