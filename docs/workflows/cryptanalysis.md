# Cryptanalysis Workflow

Analyzing cryptographic implementations for weaknesses - finding bugs in crypto code, not breaking algorithms themselves.

## Trigger

- Security audit of crypto-using codebase
- Reviewing custom crypto implementation
- Investigating suspected vulnerability
- Compliance review (FIPS, PCI-DSS)
- Incident response involving crypto failure

## Goal

- Identify implementation weaknesses (not algorithmic breaks)
- Find misuse of cryptographic primitives
- Verify correct usage of crypto libraries
- Document risks and recommend fixes

## Prerequisites

- Understanding of cryptographic concepts
- Knowledge of common crypto pitfalls
- Access to source code (this is code review, not cryptanalysis proper)
- Testing environment for verification

## Scope Clarification

This workflow is about **implementation review**, not mathematical cryptanalysis:

| In Scope | Out of Scope |
|----------|--------------|
| Hardcoded keys | Breaking AES |
| Weak random sources | Factoring RSA keys |
| Mode misuse (ECB) | Quantum attacks |
| Timing side channels | Differential cryptanalysis |
| Library misuse | Algorithm design |

## Why Crypto Bugs Are Common

1. **Subtle correctness**: Small mistakes = total failure
2. **Silent failures**: Broken crypto often "works" (just insecurely)
3. **API complexity**: Easy to misuse crypto libraries
4. **Copy-paste**: Bad examples propagate
5. **"Roll your own"**: Custom crypto is almost always wrong
6. **Side channels**: Timing, power, cache attacks are non-obvious

## Common Vulnerability Categories

### Key Management

```python
# BAD: Hardcoded key
SECRET_KEY = "super_secret_key_12345"

# BAD: Key derived from predictable source
key = hashlib.sha256(username.encode()).digest()

# BAD: Key logged/exposed
logger.debug(f"Using key: {key.hex()}")

# GOOD: Key from secure source, not logged
key = secrets.token_bytes(32)
```

### Random Number Generation

```python
# BAD: Predictable RNG
import random
nonce = random.randint(0, 2**128)

# BAD: Seeded with time
random.seed(time.time())

# GOOD: Cryptographic RNG
import secrets
nonce = secrets.token_bytes(16)
```

### Mode of Operation

```python
# BAD: ECB mode (patterns preserved)
cipher = AES.new(key, AES.MODE_ECB)

# BAD: CBC with predictable IV
cipher = AES.new(key, AES.MODE_CBC, iv=b'\x00'*16)

# BAD: CTR with reused nonce
cipher = AES.new(key, AES.MODE_CTR, nonce=fixed_nonce)

# GOOD: GCM with random nonce
nonce = secrets.token_bytes(12)
cipher = AES.new(key, AES.MODE_GCM, nonce=nonce)
```

### Authentication

```python
# BAD: Encryption without authentication (padding oracle)
ciphertext = aes_cbc_encrypt(plaintext)

# BAD: MAC-then-encrypt (vulnerable to some attacks)
mac = hmac(plaintext)
ciphertext = encrypt(plaintext + mac)

# GOOD: Authenticated encryption (GCM, ChaCha20-Poly1305)
ciphertext, tag = aes_gcm_encrypt(plaintext)

# GOOD: Encrypt-then-MAC
ciphertext = encrypt(plaintext)
mac = hmac(ciphertext)
```

### Comparison

```python
# BAD: Timing attack via early exit
def verify_mac(expected, actual):
    if len(expected) != len(actual):
        return False
    for i in range(len(expected)):
        if expected[i] != actual[i]:
            return False  # Early exit leaks position
    return True

# GOOD: Constant-time comparison
import hmac
def verify_mac(expected, actual):
    return hmac.compare_digest(expected, actual)
```

### Hash Usage

```python
# BAD: MD5/SHA1 for security purposes
hash = hashlib.md5(password.encode())

# BAD: Unsalted password hash
hash = hashlib.sha256(password.encode())

# BAD: Fast hash for passwords
hash = hashlib.sha256(salt + password.encode())

# GOOD: Password-specific KDF
hash = bcrypt.hashpw(password.encode(), bcrypt.gensalt())
# or argon2, scrypt
```

## Core Strategy: Survey → Analyze → Verify → Report

```
┌─────────────────────────────────────────────────────────┐
│                      SURVEY                              │
│  Find all crypto usage in codebase                      │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     ANALYZE                              │
│  Check each usage against known pitfalls                │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      VERIFY                              │
│  Confirm vulnerabilities are exploitable                │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      REPORT                              │
│  Document findings with severity and remediation        │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Survey Crypto Usage

### Find Crypto Imports

```bash
# Python
grep -rn "from cryptography" --include="*.py"
grep -rn "import hashlib" --include="*.py"
grep -rn "from Crypto" --include="*.py"  # PyCryptodome

# Rust
grep -rn "use ring::" --include="*.rs"
grep -rn "use openssl::" --include="*.rs"
grep -rn "use aes::" --include="*.rs"

# JavaScript
grep -rn "require('crypto')" --include="*.js"
grep -rn "import.*crypto" --include="*.ts"

# Go
grep -rn "crypto/" --include="*.go"
```

### Find Crypto-Related Strings

```bash
# Key-related
grep -rni "secret\|key\|password\|token" --include="*.py"
grep -rni "AES\|RSA\|HMAC\|SHA" --include="*.py"

# Encoding (often near crypto)
grep -rni "base64\|hex\|encode\|decode" --include="*.py"
```

### Map Crypto Flow

```markdown
## Crypto Usage Map

### Key Sources
- config.py:12 - reads KEY from environment
- auth.py:45 - derives key from password

### Encryption Points
- storage.py:78 - encrypts user data with AES-CBC
- api.py:123 - encrypts API tokens with AES-GCM

### Hashing Points
- auth.py:89 - hashes passwords with bcrypt
- cache.py:34 - hashes cache keys with MD5 (non-security)

### Signature/MAC Points
- jwt.py:56 - signs tokens with HS256
- webhook.py:78 - verifies webhook signatures
```

## Phase 2: Analyze Each Usage

### Checklist per Usage

```markdown
## Crypto Review Checklist

### Key Management
- [ ] Key not hardcoded?
- [ ] Key from secure source (KMS, HSM, env)?
- [ ] Key not logged?
- [ ] Key rotation possible?
- [ ] Appropriate key length?

### Random Numbers
- [ ] Using crypto RNG (not math.random)?
- [ ] Not seeded with predictable value?
- [ ] Sufficient entropy?

### Symmetric Encryption
- [ ] Modern algorithm (AES, ChaCha20)?
- [ ] Authenticated mode (GCM, CCM, Poly1305)?
- [ ] IV/nonce never reused?
- [ ] IV/nonce unpredictable (for CBC)?

### Asymmetric Encryption
- [ ] Sufficient key size (RSA 2048+, ECDSA 256+)?
- [ ] Proper padding (OAEP for RSA)?
- [ ] Private key protected?

### Hashing
- [ ] Appropriate algorithm for use case?
- [ ] Passwords use slow KDF (bcrypt, argon2)?
- [ ] Salts used for passwords?

### Signatures/MACs
- [ ] Constant-time comparison?
- [ ] Appropriate algorithm?
- [ ] Verify before use?

### TLS/Certificates
- [ ] TLS 1.2+ only?
- [ ] Certificate validation enabled?
- [ ] Strong cipher suites?
```

### Static Analysis Tools

```bash
# Python: bandit for security issues
bandit -r src/ -ll  # medium and high severity

# Rust: cargo audit for vulnerable dependencies
cargo audit

# JavaScript: npm audit
npm audit

# General: semgrep with crypto rules
semgrep --config=p/secrets
semgrep --config=p/crypto
```

### Code Patterns to Search

```bash
# ECB mode (almost always wrong)
grep -rn "MODE_ECB\|ECB\|aes-ecb" --include="*.py" --include="*.js"

# Weak algorithms
grep -rn "MD5\|SHA1\|DES\|RC4" --include="*.py" --include="*.rs"

# Hardcoded secrets pattern
grep -rn "key.*=.*['\"][a-zA-Z0-9]" --include="*.py"

# Predictable random
grep -rn "random.seed\|srand\|Math.random" --include="*.py" --include="*.js"

# String comparison for secrets
grep -rn "== secret\|== key\|== password" --include="*.py"
```

## Phase 3: Verify Vulnerabilities

### Timing Attack Verification

```python
import time
import statistics

def measure_comparison(secret, guess):
    times = []
    for _ in range(1000):
        start = time.perf_counter_ns()
        vulnerable_compare(secret, guess)
        times.append(time.perf_counter_ns() - start)
    return statistics.mean(times)

# If timing varies with correct prefix length, vulnerable
for i in range(len(secret)):
    guess = secret[:i] + 'X' * (len(secret) - i)
    print(f"Prefix {i}: {measure_comparison(secret, guess)}ns")
```

### Nonce Reuse Detection

```python
# Collect all nonces used
nonces = set()
for ciphertext in all_ciphertexts:
    nonce = extract_nonce(ciphertext)
    if nonce in nonces:
        print(f"REUSED NONCE: {nonce.hex()}")
    nonces.add(nonce)
```

### ECB Pattern Visibility

```python
from PIL import Image

def visualize_ecb(ciphertext, width):
    """ECB-encrypted images show patterns."""
    # Each block maps to same output, patterns visible
    blocks = [ciphertext[i:i+16] for i in range(0, len(ciphertext), 16)]
    # Color blocks by their encrypted value
    colors = {block: hash(block) % 256 for block in set(blocks)}
    pixels = [colors[b] for b in blocks]
    # Create image
    img = Image.new('L', (width, len(pixels)//width))
    img.putdata(pixels)
    img.save('ecb_visualization.png')
```

### Padding Oracle Test

```python
def test_padding_oracle(ciphertext):
    """Check if server reveals padding errors differently."""
    results = {}

    for byte_val in range(256):
        modified = ciphertext[:-1] + bytes([byte_val])
        try:
            response = send_to_server(modified)
            results[byte_val] = ('success', response)
        except PaddingError:
            results[byte_val] = ('padding_error', None)
        except DecryptionError:
            results[byte_val] = ('decrypt_error', None)

    # If we can distinguish padding errors, oracle exists
    error_types = set(r[0] for r in results.values())
    if len(error_types) > 1:
        print("PADDING ORACLE DETECTED")
```

## Phase 4: Report Findings

### Severity Classification

| Severity | Impact | Example |
|----------|--------|---------|
| Critical | Complete break | Hardcoded encryption key in repo |
| High | Significant weakness | ECB mode, nonce reuse |
| Medium | Exploitable with effort | Timing side channel |
| Low | Defense in depth | Using SHA-256 instead of SHA-3 |
| Info | Best practice | Missing key rotation mechanism |

### Finding Template

```markdown
## Finding: Hardcoded Encryption Key

**Severity**: Critical
**Location**: src/crypto.py:42
**CWE**: CWE-321 (Hard-coded Cryptographic Key)

### Description
The encryption key is hardcoded in source code:
```python
SECRET_KEY = "aabbccdd11223344"
```

### Impact
Anyone with source code access can decrypt all encrypted data.
Key in version control history even if removed now.

### Proof of Concept
```python
from crypto import SECRET_KEY
plaintext = decrypt(ciphertext, SECRET_KEY)
```

### Remediation
1. Remove hardcoded key immediately
2. Rotate key (assume compromised)
3. Load key from secure source (KMS, HSM, env var)
4. Add secret scanning to CI/CD

### References
- OWASP: https://owasp.org/...
- CWE-321: https://cwe.mitre.org/data/definitions/321.html
```

## Tools

| Tool | Purpose |
|------|---------|
| semgrep | Pattern-based code search |
| bandit | Python security linter |
| cargo-audit | Rust dependency audit |
| npm audit | JS dependency audit |
| Cryptosense | Commercial crypto analysis |
| timing-safe-equal | Timing attack testing |

## LLM-Specific Techniques

### Code Review Prompt

```
Review this encryption code for security issues:

```python
def encrypt_data(plaintext, password):
    key = hashlib.md5(password.encode()).digest()
    cipher = AES.new(key, AES.MODE_CBC)
    padded = pad(plaintext.encode(), 16)
    return cipher.encrypt(padded)
```

Check for:
1. Key derivation weaknesses
2. Mode of operation issues
3. IV/nonce handling
4. Authentication
5. Algorithm choices
```

### Pattern Matching

```
Identify crypto anti-patterns in this codebase:
[code dump]

Look for:
- ECB mode usage
- MD5/SHA1 for security
- Predictable random
- String comparison of secrets
- Missing authentication on ciphertext
```

### Fix Generation

```
This code has a timing vulnerability:

```python
def verify_signature(expected, actual):
    return expected == actual
```

Generate a secure replacement with:
1. Constant-time comparison
2. Appropriate library usage
3. Error handling
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Missed vulnerability | Penetration test, incident | Add to checklist |
| False positive | Verification fails | Understand context |
| Incomplete survey | Later discovery | Better search patterns |
| Severity misclassification | Exploit development | Revise assessment |

## Anti-patterns

- **"Using a library means it's secure"**: Libraries can be misused
- **"It's encrypted so it's safe"**: Encryption without auth is incomplete
- **"Security through obscurity"**: Hardcoded keys "hidden" in code
- **"Tests pass so it's correct"**: Crypto tests rarely cover security
- **"It's only used internally"**: Internal attackers exist

## Open Questions

### Automated Crypto Analysis

How much can be automated?
- Pattern detection: yes
- Severity assessment: partially
- Exploitability: requires context
- Business impact: requires domain knowledge

### Post-Quantum Readiness

When to start migration?
- Harvest-now-decrypt-later is real threat
- But migration is expensive
- Hybrid approaches (classical + PQ)?

### Side Channel Complexity

Which side channels matter?
- Timing: often exploitable remotely
- Cache: requires local access
- Power: requires physical access
- How to prioritize?

## See Also

- [Security Audit](security-audit.md) - Broader security review
- [Reverse Engineering Code](reverse-engineering-code.md) - Understanding crypto implementations
- [Code Review](code-review.md) - General review process

