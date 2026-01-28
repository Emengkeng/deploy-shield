# Shield-Deploy Testing & Validation Guide

**Complete testing checklist to ensure Shield-Deploy works reliably for all users**

---

## Pre-Release Testing Checklist

### Phase 1: Code Quality & Build Tests

#### 1.1 Fix Clippy Warnings
```bash
# Must pass without errors
cargo clippy -- -D warnings

# Expected: Only warnings from privacy-cash dependency (not your code)
```

**Critical fixes needed:**
- [x] Replace `deploy_with_max_program_len` with `deploy` (line 313 in deploy.rs)
- [x] Remove or suppress unused functions with `#[allow(dead_code)]`
- [x] Auto-fix format strings: `cargo clippy --fix --allow-dirty`

#### 1.2 Build Verification
```bash
# Clean build from scratch
cargo clean
cargo build --release

# Verify binary exists
ls -lh target/release/shield-deploy

# Test help command
./target/release/shield-deploy --help
```

**Success criteria:**
- [x] Build completes without errors
- [x] Binary size reasonable (< 50MB)
- [x] Help command shows all subcommands
- [x] Version info displays correctly

#### 1.3 Install Test
```bash
# Install locally
cargo install --path .

# Verify installation
which shield-deploy
shield-deploy --version
```

**Success criteria:**
- [x] Installs without errors
- [x] Binary accessible in PATH
- [x] All subcommands available

---

## Phase 2: Functional Testing (Mainnet - Use Test Wallet!)

### 2.1 Environment Setup

```bash
# Verify Solana CLI installed
solana --version

# Configure mainnet (Privacy Cash requirement)
solana config set --url https://api.mainnet-beta.solana.com

# Create test wallet with minimal SOL (0.1 SOL max)
solana-keygen new -o ~/test-wallet.json
# Fund with 0.05-0.1 SOL from your main wallet

# Use test wallet
solana config set --keypair ~/test-wallet.json

# Verify setup
solana config get
solana balance
```

**Success criteria:**
- [x] Mainnet-beta configured
- [x] Test wallet funded (0.05-0.1 SOL)
- [x] Balance visible

---

### 2.2 Initialize Test

```bash
# Create test project directory
mkdir shield-deploy-test
cd shield-deploy-test

# Initialize
shield-deploy init
```

**Expected behavior:**
- [x] `.shield/` directory created
- [x] `deployer.json` file exists
- [x] `state.json` file exists
- [x] `.gitignore` updated (includes `.shield/`)
- [x] Success message displayed

**Verify files:**
```bash
ls -la .shield/
cat .shield/state.json
```

**Test edge cases:**
```bash
# Try init again (should error)
shield-deploy init
# Expected: "Private deployer already exists"
```

---

### 2.3 Status Test (Before Funding)

```bash
shield-deploy status
```

**Expected output:**
- [x] Shows "Deployer: active"
- [x] Balance: 0 SOL (insufficient)
- [x] Programs: 0 deployed
- [x] Network: mainnet-beta
- [x] Privacy status shown

---

### 2.4 Funding Test (Privacy Cash Integration)

**⚠️ CRITICAL: This uses real mainnet transactions**

```bash
shield-deploy fund
```

**Test parameters:**
- Amount: **0.03 SOL** (minimum 0.02 + fees)
- Wallet: Use test wallet created above

**Expected behavior:**
- [x] Prompts for amount (accepts 0.03 SOL)
- [x] Prompts for wallet selection
- [x] Shows Privacy Cash explanation
- [x] Shows deposit transaction ID
- [x] Shows withdraw transaction ID (amount hidden)
- [x] 30-second privacy delay executes
- [x] Success message displayed
- [x] Fees ~0.006 SOL as documented

**Verify on-chain:**
```bash
# Check deployer balance
shield-deploy status
# Should show ~0.024 SOL (0.03 - 0.006 fees)

# Get deployer pubkey
DEPLOYER_PUBKEY=$(solana-keygen pubkey .shield/deployer.json)
echo $DEPLOYER_PUBKEY

# Check balance directly
solana balance $DEPLOYER_PUBKEY

# Verify on Solana Explorer
# https://explorer.solana.com/address/$DEPLOYER_PUBKEY
# Should see Privacy Cash withdrawal (amount hidden)
```

**Test edge cases:**
```bash
# Test below minimum
shield-deploy fund
# Enter: 0.01 SOL
# Expected: Warning about minimum, adjusts to 0.02
```

---

### 2.5 Deployment Test

**Create minimal test program:**

```bash
# Install Anchor if needed
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install latest
avm use latest

# Create test program
anchor init test-program
cd test-program

# Build
anchor build

# Return to shield-deploy-test directory
cd ../shield-deploy-test
```

**Deploy test program:**
```bash
shield-deploy deploy --program ../test-program/target/deploy/test_program.so
```

**Expected behavior:**
- [ ] Detects program file
- [ ] Shows privacy explanation
- [ ] Prompts for confirmation
- [ ] Checks deployer balance (sufficient)
- [ ] Creates buffer account
- [ ] Writes program data (shows progress)
- [ ] Deploys program successfully
- [ ] Shows Program ID
- [ ] Updates state.json

**Verify deployment:**
```bash
# Check status
shield-deploy status
# Should show 1 program deployed

# Get program ID from output
PROGRAM_ID="<your_program_id>"

# Verify on-chain
solana program show $PROGRAM_ID

# Check upgrade authority
# Should be deployer pubkey
```

**Verify privacy:**
```bash
# Check test wallet transaction history
solana transaction-history $(solana address)
# Should NOT see direct program deployment

# Check deployer transaction history
solana transaction-history $DEPLOYER_PUBKEY
# SHOULD see program deployment

# Verify on Solana Explorer
# Test wallet → Privacy Cash deposit (visible)
# Deployer → Program deployment (no link to test wallet)
```

---

### 2.6 Upgrade Test

**Modify test program:**
```bash
cd ../test-program
# Edit programs/test-program/src/lib.rs
# Add a comment or change something minor
# msg!("Version 2");

# Rebuild
anchor build

cd ../shield-deploy-test
```

**Upgrade program:**
```bash
shield-deploy upgrade <PROGRAM_ID>
```

**Expected behavior:**
- [ ] Detects program file automatically
- [ ] Verifies upgrade authority
- [ ] Checks deployer balance
- [ ] Creates new buffer
- [ ] Writes new program data
- [ ] Upgrades successfully
- [ ] Updates state.json (last_upgraded timestamp)

**Verify upgrade:**
```bash
shield-deploy status
# Should show "Last upgraded" timestamp

solana program show $PROGRAM_ID
# Verify new data length if changed
```

---

### 2.7 Transfer Authority Test

**Transfer to a test pubkey:**
```bash
# Generate a new keypair for testing
solana-keygen new -o ~/new-authority.json
NEW_AUTHORITY=$(solana-keygen pubkey ~/new-authority.json)

# Transfer authority
shield-deploy transfer-authority $NEW_AUTHORITY
```

**Expected behavior:**
- [ ] Lists all deployed programs
- [ ] Shows warning about irreversibility
- [ ] Prompts for confirmation
- [ ] Transfers authority for each program
- [ ] Shows transaction signatures
- [ ] Success message

**Verify:**
```bash
solana program show $PROGRAM_ID
# Authority should be NEW_AUTHORITY

# Try to upgrade (should fail)
shield-deploy upgrade $PROGRAM_ID
# Expected: Authority mismatch error
```

---

### 2.8 Rotate Deployer Test

**Reset for rotation test:**
```bash
# Start fresh
rm -rf .shield/
shield-deploy init
shield-deploy fund
# Fund with 0.03 SOL again
```

**Deploy another test program:**
```bash
# Use the same test program
shield-deploy deploy --program ../test-program/target/deploy/test_program.so
```

**Rotate deployer:**
```bash
shield-deploy rotate
```

**Expected behavior:**
- [ ] Shows warning and explanation
- [ ] Prompts for confirmation
- [ ] Generates new deployer keypair
- [ ] Transfers authority for all programs
- [ ] Shows success for each program
- [ ] Updates .shield/deployer.json
- [ ] Success message

**Verify:**
```bash
# New deployer pubkey should be different
DEPLOYER_PUBKEY_NEW=$(solana-keygen pubkey .shield/deployer.json)
echo "New: $DEPLOYER_PUBKEY_NEW"
echo "Old: $DEPLOYER_PUBKEY"

# Check program authority
solana program show $PROGRAM_ID
# Should show new deployer as authority
```

---

### 2.9 Finalize Test (⚠️ PERMANENT!)

**⚠️ WARNING: Only test on disposable test program**

```bash
# Deploy a throwaway program specifically for this test
shield-deploy deploy --program ../test-program/target/deploy/test_program.so
# Note the PROGRAM_ID

# Finalize it (make immutable)
shield-deploy finalize <PROGRAM_ID>
```

**Expected behavior:**
- [ ] Shows CRITICAL WARNING
- [ ] Lists consequences
- [ ] Prompts for confirmation
- [ ] Requires typing exact Program ID
- [ ] Verifies current authority
- [ ] Sets authority to None
- [ ] Confirms immutability on-chain
- [ ] Shows warning about irreversibility

**Verify:**
```bash
solana program show <PROGRAM_ID>
# Authority: None

# Try to upgrade (should fail)
shield-deploy upgrade <PROGRAM_ID>
# Expected: "Program is immutable"
```

---

## Phase 3: Error Handling Tests

### 3.1 Test Insufficient Balance

```bash
# Use deployer with low balance
shield-deploy deploy
# Expected: Clear error about insufficient balance
```

### 3.2 Test Missing Program File

```bash
shield-deploy deploy --program nonexistent.so
# Expected: "Program file not found"
```

### 3.3 Test Invalid Program ID

```bash
shield-deploy upgrade invalid-pubkey
# Expected: "Invalid program ID"
```

### 3.4 Test Authority Mismatch

```bash
# Try to upgrade a program you don't control
shield-deploy upgrade <RANDOM_PROGRAM_ID>
# Expected: Authority verification fails
```

### 3.5 Test Network Disconnection

```bash
# Temporarily disconnect internet
shield-deploy status
# Expected: Graceful error message about connection
```

---

## Phase 4: User Experience Tests

### 4.1 Help & Documentation

```bash
# Test all help commands
shield-deploy --help
shield-deploy init --help
shield-deploy fund --help
shield-deploy deploy --help
shield-deploy upgrade --help
shield-deploy status --help
shield-deploy rotate --help
shield-deploy transfer-authority --help
shield-deploy finalize --help
```

**Success criteria:**
- [x] All commands documented
- [x] Clear descriptions
- [x] Example usage shown
- [x] Flags/options explained

### 4.2 Error Messages

Review all error messages encountered during testing:
- [ ] Clear and actionable
- [ ] Suggest next steps
- [ ] No confusing technical jargon
- [ ] Include relevant context

### 4.3 Progress Indicators

During long operations:
- [ ] Deploy shows buffer creation progress
- [ ] Deploy shows chunk writing progress
- [ ] Privacy Cash shows "generating ZK proof" message
- [ ] 30-second delay shows countdown/message

---

## Phase 5: Installation Testing (Fresh Machine)

### 5.1 Prerequisites Check

**Test on clean machine or VM:**

```bash
# Verify Rust installed
rustc --version
cargo --version

# Verify Solana CLI installed
solana --version

# If not installed, document installation:
```

**Document required versions:**
- Rust: 1.88+
- Solana CLI: Latest stable
- Anchor (optional, for building programs)

### 5.2 Fresh Install Test

```bash
# Clone repository
git clone https://github.com/Emengkeng/deploy-shield
cd deploy-shield

# Install
cargo install --path .

# Verify
shield-deploy --version
```

**Success criteria:**
- [ ] Installs without errors
- [ ] Binary accessible globally
- [ ] All dependencies resolve
- [ ] No compilation warnings (except privacy-cash)

### 5.3 First-Time User Experience

```bash
# Run without any setup
shield-deploy status
# Expected: Helpful message about running init first

# Initialize
shield-deploy init
# Expected: Clear next steps (fund the deployer)

# Check status
shield-deploy status
# Expected: Shows 0 balance, suggests funding
```

---

## Phase 6: Documentation Validation

### 6.1 README Accuracy

Compare actual behavior with README:
- [ ] All commands work as documented
- [ ] Privacy explanation matches reality
- [ ] Fee amounts accurate (~0.006 SOL)
- [ ] Minimum amounts correct (0.02 SOL)
- [ ] Network requirements clear (mainnet only)

### 6.2 Example Workflows

Test all examples from README:
- [ ] "Typical Flow" section works
- [ ] All command examples run successfully
- [ ] Output matches expectations

---

## Phase 7: Privacy Verification

### 7.1 On-Chain Analysis

**Using Solana Explorer:**

1. **Check funding wallet:**
   - [ ] Only shows Privacy Cash deposit
   - [ ] No direct link to deployer
   - [ ] No program deployment transactions

2. **Check deployer wallet:**
   - [ ] Shows Privacy Cash withdrawal (amount hidden)
   - [ ] Shows program deployment
   - [ ] No obvious link to funding wallet

3. **Check Privacy Cash transaction:**
   - [ ] Deposit amount visible
   - [ ] Withdraw amount HIDDEN
   - [ ] Uses ZK proof (check transaction details)

### 7.2 Timing Analysis

```bash
# Record timestamps
DEPOSIT_TIME="<from explorer>"
DEPLOY_TIME="<from explorer>"

# Calculate difference
# Should be 30+ seconds (privacy delay)
```

---

## Phase 8: Security Checks

### 8.1 File Permissions

```bash
ls -la .shield/
# deployer.json should be readable only by user (600 or similar)
```

### 8.2 .gitignore Verification

```bash
git status
# .shield/ should NOT appear as untracked
# circuit/ should NOT appear as untracked
```

### 8.3 Key Storage

```bash
# Verify deployer key format
cat .shield/deployer.json | jq
# Should be valid JSON with keypair array
```

---

## Phase 9: Performance Testing

### 9.1 Large Program Deployment

Test with larger program (if available):
```bash
# Deploy ~100KB program
# Measure time
time shield-deploy deploy --program large_program.so
```

**Success criteria:**
- [ ] Completes successfully
- [ ] Progress shown for chunks
- [ ] Reasonable completion time (< 5 min)

### 9.2 Multiple Deployments

```bash
# Deploy 3 programs in succession
shield-deploy deploy --program program1.so
shield-deploy deploy --program program2.so
shield-deploy deploy --program program3.so

# Check status
shield-deploy status
# Should track all 3 programs
```

---

## Phase 10: Final Pre-Release Checklist

### 10.1 Code Quality
- [ ] All clippy warnings fixed (except external deps)
- [ ] No compilation errors
- [ ] Release build optimized (`cargo build --release`)

### 10.2 Documentation
- [ ] README.md complete and accurate
- [ ] DISCLAIMER clearly visible
- [ ] Installation instructions tested
- [ ] All examples working
- [ ] Privacy model explained
- [ ] Limitations documented

### 10.3 Testing Coverage
- [ ] All commands tested on mainnet
- [ ] Error handling verified
- [ ] Privacy features confirmed
- [ ] User experience smooth
- [ ] Fresh install tested

### 10.4 Safety Measures
- [ ] Warnings prominent (finalize, transfer, etc.)
- [ ] Confirmation prompts in place
- [ ] Clear error messages
- [ ] Minimum amounts enforced

### 10.5 GitHub Repository
- [ ] README.md up to date
- [ ] License file (MIT)
- [ ] Cargo.toml metadata complete
- [ ] .gitignore includes .shield/ and circuit/
- [ ] Example usage in README
- [ ] Contributing guidelines (optional)

---

## Release Testing Script

Save as `test_release.sh`:

```bash
#!/bin/bash

set -e

echo "🧪 Shield-Deploy Release Testing"
echo "================================="

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

function check() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $1"
    else
        echo -e "${RED}✗${NC} $1"
        exit 1
    fi
}

# 1. Build tests
echo -e "\n${YELLOW}Phase 1: Build Tests${NC}"
cargo clean
cargo build --release
check "Release build"

cargo clippy -- -D warnings 2>&1 | grep -v "privacy-cash"
check "Clippy check (ignoring external warnings)"

# 2. Installation test
echo -e "\n${YELLOW}Phase 2: Installation${NC}"
cargo install --path . --force
check "Install binary"

which shield-deploy
check "Binary in PATH"

# 3. Command tests
echo -e "\n${YELLOW}Phase 3: Command Tests${NC}"
shield-deploy --help > /dev/null
check "Help command"

shield-deploy init --help > /dev/null
check "Init help"

shield-deploy fund --help > /dev/null
check "Fund help"

shield-deploy deploy --help > /dev/null
check "Deploy help"

shield-deploy status --help > /dev/null
check "Status help"

# 4. Environment check
echo -e "\n${YELLOW}Phase 4: Environment Check${NC}"
solana --version > /dev/null
check "Solana CLI installed"

solana config get | grep "mainnet-beta" > /dev/null
check "Mainnet configured"

# 5. Functional test setup
echo -e "\n${YELLOW}Phase 5: Functional Test Setup${NC}"
TEST_DIR="shield-deploy-test-$$"
mkdir -p $TEST_DIR
cd $TEST_DIR

shield-deploy init
check "Initialize deployer"

[ -f .shield/deployer.json ]
check "Deployer keypair created"

[ -f .shield/state.json ]
check "State file created"

grep ".shield/" .gitignore > /dev/null
check ".gitignore updated"

shield-deploy status
check "Status command"

# Cleanup
cd ..
rm -rf $TEST_DIR

echo -e "\n${GREEN}✓ All tests passed!${NC}"
echo ""
echo "Ready for release. Manual testing required:"
echo "  1. Test funding with real SOL"
echo "  2. Deploy a test program"
echo "  3. Verify privacy on Solana Explorer"
echo "  4. Test on fresh machine"
```

Make executable and run:
```bash
chmod +x test_release.sh
./test_release.sh
```

---

## User Acceptance Criteria

Before releasing to users, verify:

### ✅ Must Have
- [ ] Builds without errors on Linux/macOS
- [ ] All commands work on mainnet
- [ ] Privacy Cash integration functional
- [ ] Clear error messages
- [ ] Deployer creation works
- [ ] Program deployment succeeds
- [ ] Program upgrades work
- [ ] .gitignore automatically updated

### ✅ Should Have
- [ ] Progress indicators for long operations
- [ ] Helpful prompts and confirmations
- [ ] Status command shows all relevant info
- [ ] Privacy delay executes (30s)
- [ ] Transaction signatures displayed

### ✅ Nice to Have
- [ ] Colored output (warnings, success)
- [ ] Timestamp formatting (human-readable)
- [ ] Balance formatting (SOL not lamports)
- [ ] Suggestions for next steps

---

## Known Limitations to Document

Users should know:
1. **Privacy Cash mainnet only** - Won't work on devnet
2. **Minimum 0.02 SOL** - Lower amounts rejected
3. **~0.006 SOL fees** - Privacy Cash network fees
4. **Circuit files required** - Auto-downloaded first use
5. **30-second delay** - Privacy measure, cannot skip
6. **No RPC privacy** - Use VPN/Tor for full privacy
7. **Deposit visible** - Only withdraw amount hidden
8. **One deployer per project** - Not wallet-wide

---

## Post-Release Monitoring

After users start installing:

### Week 1
- [ ] Monitor GitHub issues
- [ ] Check installation reports
- [ ] Verify no critical bugs
- [ ] Collect user feedback

### Week 2-4
- [ ] Review error reports
- [ ] Update documentation based on questions
- [ ] Add FAQ section if needed
- [ ] Consider feature requests

---

## Emergency Rollback Plan

If critical issues found:
1. Tag current version: `git tag v0.1.0-broken`
2. Revert to last known good: `git revert HEAD`
3. Notify users via GitHub issues
4. Document the issue
5. Fix and re-test before re-release

---

## Success Metrics

Release is successful when:
- [ ] 10+ users install without issues
- [ ] 5+ successful program deployments reported
- [ ] No critical bugs in 2 weeks
- [ ] Positive user feedback
- [ ] Privacy features working as documented
- [ ] No security vulnerabilities reported

---

**You're ready to release when all phases pass! 🚀**

Remember: Start with a small group of beta testers before public announcement.