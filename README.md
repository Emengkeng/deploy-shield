# Shield-Deploy

**Privacy-preserving Solana program deployment using ZK-proof private transfers**

---

DISCLAIMER

Shield-Deploy is in active development and should be considered experimental software.
This tool has not been formally audited. Use at your own risk. The authors assume no responsibility for:

Loss of funds due to bugs or vulnerabilities
Loss of program upgrade authority due to key management issues
Privacy failures or metadata leakage
Any damages arising from the use of this software

Recommendations:

Test thoroughly on devnet before mainnet use
Never deploy production programs without understanding the risks
Always maintain secure backups of your deployer keys
Review the code yourself before trusting it with real funds

By using Shield-Deploy, you acknowledge these risks and accept full responsibility for any consequences.

## The Problem

When you deploy a Solana program, your funding wallet is permanently linked on-chain to:
- The program deployment transaction
- All upgrade transactions
- The program's upgrade authority

This creates a traceable connection between your identity (funding wallet) and your deployed programs, which can:
- Reveal your development activity
- Link multiple projects to the same entity
- Expose wallet balances and transaction patterns

**Shield-Deploy breaks this link.**

---

## What Shield-Deploy Does

Shield-Deploy uses Privacy Cash's Groth16 zero-knowledge proofs to obscure the funding path between your main wallet and program deployments. With Privacy Cash, the amount transferred to your burner wallet is **completely hidden on-chain**.

**How it works:**

1. **Initialization**: Generate a project-scoped "burner" deployer wallet
2. **Funding**: Move SOL from your main wallet → Privacy Cash pool → burner (amount hidden, unlinkable on-chain)
3. **Deployment**: Burner wallet deploys and controls the program
4. **Upgrades**: Burner continues to upgrade the program without ever involving your main wallet

**Result:** Your main wallet never appears in program deployment transactions, and observers cannot see how much you funded the burner.

---

## Architecture

```
Main Wallet (funding)
    ↓ [Deposit to Privacy Cash - visible]
Privacy Cash Pool (Groth16 ZK proofs)
    ↓ [Withdraw to burner - AMOUNT HIDDEN]
Burner Deployer (unshield)
    ↓ [30s delay + anonymity set]
Program Deploy/Upgrade
```

**Privacy guarantees:**
- On-chain: No direct link between main wallet and program
- Amount privacy: Withdraw amount is **hidden via ZK proofs**
- Timing: 30-second delay prevents correlation
- Anonymity set: Privacy Cash pool provides mixing

**What Shield-Deploy does NOT protect:**
- RPC metadata (IP addresses, request patterns)
- Bytecode fingerprinting (identical programs are identifiable)
- Network-level traffic analysis
- Deposit transaction visibility (deposit amount is visible)

---

## Installation

```bash
# Clone and build
git clone https://github.com/Emengkeng/deploy-shield
cd deploy-shield
cargo build --release

# Install
cargo install --path .
```

**Requirements:**
- Rust 1.88+
- Solana CLI tools
- Privacy Cash circuit files (downloaded automatically on first use)

---

## Usage

### 1. Initialize a private deployer

```bash
shield-deploy init
```

Creates a `.shield/` directory with:
- `deployer.json`: The burner keypair (automatically added to `.gitignore`)
- `state.json`: Project metadata

### 2. Fund the deployer

```bash
shield-deploy fund
```

Prompts for:
- Amount to fund (minimum 0.02 SOL for Privacy Cash)
- Funding wallet (Solana CLI wallet or keypair file)

What happens:
1. Your funding wallet deposits SOL into Privacy Cash pool (deposit visible)
2. Privacy Cash generates Groth16 ZK proof client-side
3. Burner wallet receives SOL via ZK proof withdrawal (**amount hidden on-chain**)
4. 30-second privacy delay prevents timing correlation
5. **Your funding wallet is unlinkable from the burner wallet**

### 3. Deploy your program

```bash
shield-deploy deploy
```

Or specify a program file:
```bash
shield-deploy deploy --program target/deploy/my_program.so
```

What happens:
- Burner wallet deploys the program
- Burner is set as upgrade authority
- Your main wallet remains unlinkable on-chain

### 4. Upgrade your program

```bash
shield-deploy upgrade
```

Or specify:
```bash
shield-deploy upgrade --program target/deploy/my_program.so
```

The same burner upgrades the program—no main wallet involved.

### 5. Check status

```bash
shield-deploy status
```

Shows:
- Deployer balance
- Deployed programs
- Network
- Privacy status

### 6. Advanced: Rotate deployer

```bash
shield-deploy rotate
```

Creates a new burner and transfers upgrade authority.
Use if you suspect key exposure.

### 7. Transfer authority

```bash
shield-deploy transfer-authority <new_authority_pubkey>
```

Transfer upgrade authority to a DAO, multisig, or other address.

### 8. Finalize program (make immutable)

```bash
shield-deploy finalize <program_id>
```

**⚠️ WARNING: This is PERMANENT and IRREVERSIBLE**

Makes a program completely immutable by setting the upgrade authority to `None`. After this operation:
- **NO ONE** can upgrade the program (including you)
- Bugs cannot be fixed
- Features cannot be added
- This operation **CANNOT BE UNDONE**

Only use this when:
- The program has been thoroughly audited
- All tests pass
- The code is production-ready
- You understand you're giving up all control

This is useful for creating trustless programs where users need absolute certainty that the code cannot change.

---

## How Privacy Works

### The Problem

Traditional deployment:
```
Main Wallet → Deploy Transaction → Program
           → Upgrade Transaction → Program
```

**Result:** Your wallet is permanently linked to the program on-chain.

### Shield-Deploy Solution

```
Main Wallet
    ↓ [Privacy Cash deposit - visible]
Privacy Cash Pool (ZK proof mixing)
    ↓ [ZK proof withdrawal - AMOUNT HIDDEN]
Burner Wallet (unlinked identity)
    ↓ [30s privacy delay]
Deploy/Upgrade Transactions
```

**Result:** Programs are deployed by a wallet with no clear owner and unknown funding amount.

### Privacy Techniques

1. **Zero-Knowledge Proof Privacy (Groth16)**
   - Privacy Cash uses Groth16 ZK proofs for complete amount privacy
   - Withdraw amount is **cryptographically hidden** on-chain
   - On-chain observers cannot see how much SOL was transferred
   - Proof generated client-side (no trusted setup per transaction)

2. **Burner Wallet Separation**
   - Generated locally, never published as "yours"
   - One burner per project
   - No metadata linking burner to your identity

3. **Mixing Pool Anonymity**
   - Privacy Cash pool acts as a mixing service
   - Your withdraw is indistinguishable from other Privacy Cash users
   - Larger pool TVL = better anonymity set

4. **Timing Delay**
   - 30-second delay between funding and first deployment
   - Breaks simple timing correlation
   - Prevents: "Privacy Cash withdraw at 10:00:00, deployed at 10:00:01"

5. **No Metadata Storage**
   - Never stores: "Burner X belongs to User Y"
   - Never stores: "Funded from Wallet Z"
   - Only stores: Burner pubkey + deployed programs

### Privacy Cash vs Traditional Transfers

**Traditional Transfer:**
```
Wallet A sends 5.5 SOL to Wallet B
→ On-chain: Everyone sees 5.5 SOL transferred
→ Privacy: NONE
```

**Privacy Cash Transfer:**
```
Wallet A deposits 5.5 SOL to Privacy Cash
Wallet B withdraws via ZK proof
→ On-chain: Deposit visible, withdraw amount HIDDEN
→ Privacy: VERY HIGH
```

### Honest Privacy Assessment

**What This Protects:**
- ✅ Identity separation (burner has no clear owner)
- ✅ **Amount privacy (withdraw amount hidden via ZK proofs)**
- ✅ Portfolio privacy (can't link all your projects)
- ✅ Funding source anonymity (no direct link to main wallet)
- ✅ Advanced on-chain analysis resistance

**What This Does NOT Protect:**
- ❌ Deposit transaction visibility (deposit amount is public)
- ❌ RPC node seeing your IP address
- ❌ Bytecode fingerprinting (identical programs linkable)
- ❌ Network-level traffic analysis

**Bottom Line:**
Shield-Deploy provides strong privacy for developers using Groth16 ZK proofs.
It hides withdraw amounts and breaks wallet linkage, making it significantly better than direct deployment.

---

## Configuration

### Network Selection

Shield-Deploy automatically detects your Solana CLI network configuration:

```bash
solana config get
```

To change networks:
```bash
solana config set --url https://api.devnet.solana.com
```

Supported networks:
- Devnet (recommended for testing)
- Mainnet-beta
- Testnet
- Localhost (requires Privacy Cash local deployment)

### Privacy Cash Requirements

**Minimum amounts:**
- SOL: 0.02 SOL
- USDC: 2 USDC
- USDT: 2 USDT

**Circuit files:**
Privacy Cash requires Groth16 circuit files for ZK proof generation. These are automatically downloaded to `./circuit/` on first use.

**Fees:**
Privacy Cash charges approximately 0.006 SOL in fees for deposit + withdraw operations.

### RPC Endpoints

Shield-Deploy uses your Solana CLI RPC configuration by default.

For privacy-focused RPC, consider:
- Running your own Solana validator
- Using a trusted RPC provider
- Routing through Tor (advanced)

---

## Project Structure

```
your-project/
├── .shield/                    # Created by init
│   ├── deployer.json          # Burner keypair (KEEP PRIVATE)
│   └── state.json             # Project metadata
├── circuit/                   # Privacy Cash ZK circuit files (auto-downloaded)
│   ├── proving_key.bin
│   └── verification_key.bin
├── target/deploy/             # Your Anchor build output
│   └── your_program.so
├── programs/                  # Your Solana programs
├── .gitignore                 # Auto-updated to ignore .shield/ and circuit/
└── ...
```

**Important:** `.shield/deployer.json` contains your deployment authority. If compromised, an attacker can upgrade your program. Treat it like a private key.

---

## Troubleshooting

### "No private deployer found"

Run `shield-deploy init` first.

### "Insufficient deployer balance"

Fund the deployer:
```bash
shield-deploy fund
```

Check balance:
```bash
shield-deploy status
```

### "Privacy Cash requires minimum 0.02 SOL"

Privacy Cash has minimum deposit amounts. Fund at least:
- 0.02 SOL for SOL transfers
- 2 USDC for USDC transfers
- 2 USDT for USDT transfers

### "Program file not found"

Build your program first:
```bash
anchor build
```

Or specify the path:
```bash
shield-deploy deploy --program path/to/program.so
```

### "Circuit files not found"

Privacy Cash circuit files should download automatically. If they don't:
```bash
# Manually download from Privacy Cash repository
mkdir -p circuit
# Follow Privacy Cash documentation for circuit file setup
```

### "Privacy Cash transfer failed"

Common issues:
- Insufficient balance for deposit + fees (~0.006 SOL fees)
- Network connectivity issues
- RPC rate limiting

Check your balance and RPC connection:
```bash
solana balance
solana config get
```

### Connection Errors

Check your RPC:
```bash
solana config get
solana balance  # Test connection
```

### "Invalid keypair"

Your `.shield/deployer.json` may be corrupted. If you have a backup, restore it. Otherwise, you'll need to:
```bash
rm -rf .shield/
shield-deploy init
shield-deploy fund
```

**Warning:** This creates a new deployer. You'll lose control of programs deployed with the old key unless you transfer authority first.

---

## Development Workflow

### Typical Flow

1. **Start new project**
   ```bash
   anchor init my_project
   cd my_project
   shield-deploy init
   ```

2. **Fund deployer**
   ```bash
   shield-deploy fund
   ```

3. **Build and deploy**
   ```bash
   anchor build
   shield-deploy deploy
   ```

4. **Make changes**
   ```bash
   # Edit code
   anchor build
   shield-deploy upgrade
   ```

5. **Check status anytime**
   ```bash
   shield-deploy status
   ```
---

## Technical Architecture

### Privacy Cash Integration

Shield-Deploy uses Privacy Cash's Groth16 ZK-proof protocol:

```
┌──────────────────────────────────────────────────────────┐
│ Privacy Cash Protocol (Groth16 ZK Proofs)                │
├──────────────────────────────────────────────────────────┤
│ • Deposit: Visible on-chain transaction                 │
│ • ZK Proof: Generated client-side (Groth16)             │
│ • Withdraw: Amount HIDDEN via zero-knowledge proof      │
│ • Privacy: Very high (cryptographic hiding)             │
└──────────────────────────────────────────────────────────┘
                          ↕
┌──────────────────────────────────────────────────────────┐
│ Shield-Deploy Privacy Layer                              │
├──────────────────────────────────────────────────────────┤
│ • send_privately(): Main wallet → Privacy Cash → Burner │
│ • ZK proof generation: Client-side, no trusted party    │
│ • Timing delay: 30s obfuscation                         │
│ • Result: Burner funded with hidden amount              │
└──────────────────────────────────────────────────────────┘
                          ↕
┌──────────────────────────────────────────────────────────┐
│ Solana BPF Loader Upgradeable                            │
├──────────────────────────────────────────────────────────┤
│ • Deploy: Burner wallet deploys program                 │
│ • Upgrade: Burner wallet upgrades program                │
│ • Authority: Burner holds upgrade authority              │
└──────────────────────────────────────────────────────────┘
```

### Privacy Cash Flow

```
1. Deposit Phase (visible on-chain):
   Funding Wallet → Privacy Cash Pool
   - Amount: VISIBLE
   - Sender: VISIBLE
   
2. ZK Proof Generation (client-side):
   - Groth16 proof created locally
   - Proves: "I deposited, allow withdraw"
   - Hides: Withdraw amount
   
3. Withdraw Phase (amount hidden):
   Privacy Cash Pool → Burner Wallet
   - Amount: HIDDEN (via ZK proof)
   - Link to deposit: BROKEN
   
4. Result:
   On-chain observers see:
   - Someone deposited X SOL
   - Someone withdrew ??? SOL (hidden!)
   - Cannot link deposit to withdraw
```

### State Management

```
.shield/deployer.json:
{
  "keypair": [byte array]  // Ed25519 keypair for burner
}

.shield/state.json:
{
  "network": "devnet",
  "deployed_programs": [
    {
      "program_id": "...",
      "deployed_at": timestamp,
      "last_upgraded": timestamp
    }
  ],
  "last_balance": 5000000000
}
```

---

## Contributing

Shield-Deploy is open source. Contributions welcome!

### Areas for Improvement

1. **Enhanced Privacy**
   - Multi-hop Privacy Cash transfers (deposit → wait → re-deposit)
   - Tor integration for RPC privacy
   - Decoy transactions
   - Support for other Privacy Cash tokens (USDC, USDT)

2. **Better Key Management**
   - Hardware wallet support for burner
   - Encrypted storage with passphrase
   - Multi-sig burner wallets

3. **UX Improvements**
   - GUI application
   - Browser extension
   - Better progress indicators
   - Privacy Cash pool TVL monitoring

4. **Testing**
   - Integration tests with Privacy Cash
   - Fuzz testing
   - Deployment scenario tests
   - Privacy analysis tools

5. **Documentation**
   - Video tutorials
   - More examples
   - Privacy best practices guide

---
## TODO
[] - Add a command to fetch users burner wallet address

## FAQ

**Q: Is this completely untraceable?**  
A: No. It hides the withdraw amount via ZK proofs and breaks wallet linkage, but deposit transactions are visible, and it doesn't protect against RPC metadata or sophisticated traffic analysis.

**Q: How does Privacy Cash hide the amount?**  
A: Privacy Cash uses Groth16 zero-knowledge proofs. When you withdraw, a ZK proof cryptographically proves you deposited funds without revealing the amount. This is similar to how Zcash works.

**Q: Can I use this for mainnet?**  
A: Yes, but audit the code first. This is a privacy tool—ensure you understand the threat model before production use.

**Q: What if I lose my burner key?**  
A: You lose upgrade authority for your programs. Always back up `.shield/deployer.json`.

**Q: Does this make my program private?**  
A: No. The program code and its transactions are still public. This only hides WHO deployed it and HOW MUCH they funded the burner.

**Q: Why 30 seconds delay?**  
A: Breaks timing correlation. Without it, Privacy Cash withdraw and deployment transactions could be linked by timestamp.

**Q: What's the minimum amount I can fund?**  
A: Privacy Cash requires minimum 0.02 SOL (or 2 USDC/USDT). This is a protocol requirement for ZK proof generation.

**Q: How much do Privacy Cash fees cost?**  
A: Approximately 0.006 SOL for deposit + withdraw operations combined.

**Q: Is Privacy Cash trustless?**  
A: Yes. Groth16 proofs are generated client-side. There's no trusted third party in the privacy mechanism.

**Q: Can Privacy Cash be traced?**  
A: The withdraw amount is cryptographically hidden via ZK proofs. However, sophisticated adversaries could still attempt timing analysis or network-level correlation. Privacy Cash provides very strong on-chain privacy but isn't a silver bullet.

---

## License

MIT

---

## Acknowledgments

- **Privacy Cash** - For Groth16 ZK-proof private transfer infrastructure
- **Solana Foundation** - For the blockchain infrastructure
- **Anchor** - For the excellent program framework
- **Groth16** - For the zero-knowledge proof system

---

## Contact

For questions, issues, or contributions:
- Twitter: [[X](https://x.com/emJuslen)]

**Privacy-preserving deployment for Solana developers**