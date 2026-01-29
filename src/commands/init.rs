use anyhow::{Context, Result};
use solana_sdk::signature::Keypair;
use std::fs;
use std::path::PathBuf;
use indicatif::{ProgressBar, ProgressStyle};
use crate::config::Config;
use crate::utils::{print_header, print_success, prompt_confirmation};

const CIRCUIT_BASE_URL: &str = "https://raw.githubusercontent.com/Emengkeng/shield-deploy/main/circuit";

pub async fn execute() -> Result<()> {
    print_header("Shield-Deploy");
    
    let config = Config::new()?;
    
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
    println!("• The deployer key stays on this machine");
    println!("• Circuit files (~19MB) will be downloaded on first use\n");
    
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
    
    // Download and setup circuit files
    setup_circuit_files().await?;
    
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
    println!("Circuits:       circuit/ directory (19MB)");
    
    println!("\nNext step:");
    println!("→ Fund the deployer with SOL using `shield-deploy fund`");
    
    Ok(())
}

/// Setup circuit files - download from public hosting
async fn setup_circuit_files() -> Result<()> {
    let circuit_dir = PathBuf::from("circuit");
    fs::create_dir_all(&circuit_dir)
        .context("Failed to create circuit directory")?;
    
    let wasm_path = circuit_dir.join("transaction2.wasm");
    let zkey_path = circuit_dir.join("transaction2.zkey");
    
    // Check if both files exist and are valid
    let needs_download = !wasm_path.exists() || !zkey_path.exists() 
        || !verify_circuit_files(&wasm_path, &zkey_path)?;
    
    if needs_download {
        println!("\n📥 Downloading ZK circuit files...");
        println!("   This is a one-time download (~19MB)");
        println!("   Source: {}\n", CIRCUIT_BASE_URL);
        
        download_circuits(&wasm_path, &zkey_path).await
            .context("Failed to download circuit files")?;
        
        println!("\n  ✓ Circuit files downloaded and verified");
    } else {
        println!("  ✓ Circuit files already present");
    }
    
    // Add circuit directory to .gitignore
    add_circuit_to_gitignore()?;
    
    Ok(())
}

/// Download both circuit files
async fn download_circuits(
    wasm_path: &PathBuf,
    zkey_path: &PathBuf,
) -> Result<()> {
    // Download WASM file
    download_circuit_file(
        "transaction2.wasm",
        wasm_path,
        3_050_000, // ~3.05MB
    ).await.context("Failed to download transaction2.wasm")?;
    
    // Download ZKEY file
    download_circuit_file(
        "transaction2.zkey",
        zkey_path,
        15_700_000, // ~15.7MB
    ).await.context("Failed to download transaction2.zkey")?;
    
    Ok(())
}

/// Download a single circuit file with progress bar
async fn download_circuit_file(
    filename: &str,
    dest_path: &PathBuf,
    expected_size: u64,
) -> Result<()> {
    let url = format!("{}/{}", CIRCUIT_BASE_URL, filename);
    
    println!("   Downloading {}...", filename);
    
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(std::time::Duration::from_secs(300)) // 5 min timeout for large files
        .build()?;
    
    let response = client.get(&url)
        .send()
        .await
        .context(format!("Failed to connect to {}", url))?;
    
    if !response.status().is_success() {
        anyhow::bail!(
            "Failed to download {}: HTTP {}\n\
            URL: {}\n\n\
            The circuit files could not be downloaded.\n\
            Please check your internet connection or report this issue at:\n\
            https://github.com/Emengkeng/shield-deploy/issues",
            filename,
            response.status(),
            url
        );
    }
    
    let total_size = response.content_length().unwrap_or(expected_size);
    
    // Create progress bar
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("   [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );
    
    // Download with progress tracking
    let mut file = fs::File::create(dest_path)
        .context(format!("Failed to create {}", dest_path.display()))?;
    
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    
    use futures_util::StreamExt;
    use std::io::Write;
    
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error while downloading file")?;
        file.write_all(&chunk)
            .context("Failed to write to file")?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }
    
    pb.finish_with_message(format!("   ✓ {} downloaded", filename));
    
    // Verify file size
    let actual_size = fs::metadata(dest_path)?.len();
    if actual_size == 0 {
        anyhow::bail!("Downloaded file is empty: {}", filename);
    }
    
    Ok(())
}

/// Verify circuit files exist and have reasonable sizes
fn verify_circuit_files(wasm_path: &PathBuf, zkey_path: &PathBuf) -> Result<bool> {
    if !wasm_path.exists() || !zkey_path.exists() {
        return Ok(false);
    }
    
    let wasm_size = fs::metadata(wasm_path)?.len();
    let zkey_size = fs::metadata(zkey_path)?.len();
    
    // Verify files aren't corrupted (basic size check)
    let wasm_valid = wasm_size > 2_000_000 && wasm_size < 5_000_000; // ~3MB ± tolerance
    let zkey_valid = zkey_size > 14_000_000 && zkey_size < 17_000_000; // ~15.6MB ± tolerance
    
    Ok(wasm_valid && zkey_valid)
}

/// Add circuit directory to .gitignore
fn add_circuit_to_gitignore() -> Result<()> {
    let gitignore_path = PathBuf::from(".gitignore");
    
    let mut content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };
    
    let circuit_entry = "circuit/";
    
    if !content.contains(circuit_entry) {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("\n# ZK circuit files (downloaded locally)\n");
        content.push_str(circuit_entry);
        content.push('\n');
        
        fs::write(&gitignore_path, content)?;
    }
    
    Ok(())
}