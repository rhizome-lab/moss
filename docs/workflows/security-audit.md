# Security Audit Workflow

Systematic review of code for security vulnerabilities - finding bugs before attackers do.

## Trigger

- Pre-release security review
- New feature involving auth/crypto/user data
- Compliance requirement (SOC2, PCI-DSS, HIPAA)
- After security incident (was anything else affected?)
- Regular cadence (quarterly, annually)
- External code integration (dependencies, acquisitions)

## Goal

- Identify security vulnerabilities
- Classify by severity and exploitability
- Provide actionable remediation guidance
- Document findings for compliance/tracking
- NOT: penetration testing (that's dynamic, this is static)

## Prerequisites

- Source code access
- Understanding of application architecture
- Threat model (what are we protecting?)
- Security checklist/standards to audit against
- Time and focus (can't rush security review)

## Audit Scope

| In Scope | Out of Scope |
|----------|--------------|
| Source code review | Penetration testing |
| Configuration review | Social engineering |
| Dependency analysis | Physical security |
| Architecture review | Incident response |
| Cryptographic usage | Runtime monitoring |

## Core Strategy: Scope → Survey → Deep Dive → Report

```
┌─────────────────────────────────────────────────────────┐
│                       SCOPE                              │
│  Define what to audit, threat model, constraints        │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      SURVEY                              │
│  Broad pass: identify high-risk areas                   │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                    DEEP DIVE                             │
│  Focused review of high-risk components                 │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      REPORT                              │
│  Document findings, severity, remediation               │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Scope Definition

### Threat Model

```markdown
## Threat Model

### Assets (what are we protecting?)
- User credentials
- Personal data (PII)
- Financial transactions
- API keys and secrets
- Session tokens

### Adversaries (who might attack?)
- External attackers (internet-facing)
- Malicious users (authenticated)
- Compromised dependencies
- Insider threats

### Attack Vectors (how might they attack?)
- Network (API endpoints)
- Input (forms, file uploads)
- Authentication bypass
- Authorization escalation
- Data exfiltration
```

### Audit Boundaries

```markdown
## Audit Scope

### In Scope
- All server-side code in `src/`
- API endpoints in `routes/`
- Authentication module
- Database queries
- Third-party integrations

### Out of Scope
- Frontend JavaScript (separate audit)
- Infrastructure/deployment
- Dependencies (covered by automated scanning)

### Time Budget
- Survey: 2 hours
- Deep dive: 6 hours
- Reporting: 2 hours
```

## Phase 2: Survey

### Identify High-Risk Areas

```bash
# Authentication/Authorization
grep -rn "authenticate\|authorize\|login\|password\|session" src/

# Cryptography
grep -rn "encrypt\|decrypt\|hash\|hmac\|sign\|verify" src/

# User input
grep -rn "request\.\|params\.\|body\.\|query\." src/

# Database queries
grep -rn "SELECT\|INSERT\|UPDATE\|DELETE\|query\|execute" src/

# File operations
grep -rn "open\|read\|write\|path\|filename" src/

# External calls
grep -rn "http\|fetch\|request\|curl\|exec\|system" src/
```

### OWASP Top 10 Checklist

| Category | What to Look For |
|----------|------------------|
| **Injection** | SQL, command, LDAP, XPath injection |
| **Broken Auth** | Weak passwords, session issues, credential exposure |
| **Sensitive Data** | Unencrypted PII, keys in code, logging secrets |
| **XXE** | XML parsing without disabling external entities |
| **Broken Access** | Missing authorization checks, IDOR |
| **Misconfig** | Debug enabled, default credentials, verbose errors |
| **XSS** | Unescaped output, DOM manipulation |
| **Insecure Deserialization** | Untrusted data deserialization |
| **Vulnerable Components** | Outdated dependencies with CVEs |
| **Insufficient Logging** | Missing audit trails, no alerting |

### Map Attack Surface

```markdown
## Attack Surface Map

### External Entry Points
- POST /api/login - accepts username/password
- POST /api/register - creates new users
- GET /api/users/:id - returns user data
- POST /api/upload - accepts file uploads
- GET /api/search?q= - search with user input

### Internal Trust Boundaries
- API → Database (SQL queries)
- API → Cache (Redis commands)
- API → Email service (SMTP)
- API → Payment processor (external API)

### Data Flows
- User credentials: form → API → bcrypt → database
- Session tokens: API → JWT → cookie → subsequent requests
- User uploads: form → API → S3 (with validation?)
```

## Phase 3: Deep Dive

### Authentication Review

```python
# Check: Password hashing
# BAD
hash = hashlib.md5(password.encode()).hexdigest()
hash = hashlib.sha256(password.encode()).hexdigest()

# GOOD
hash = bcrypt.hashpw(password.encode(), bcrypt.gensalt())
hash = argon2.hash(password)
```

```python
# Check: Session management
# BAD - predictable session ID
session_id = str(user_id) + str(time.time())

# GOOD - cryptographically random
session_id = secrets.token_urlsafe(32)
```

```python
# Check: Timing attacks on comparison
# BAD - early exit leaks info
if user_token != expected_token:
    return False

# GOOD - constant time
import hmac
if not hmac.compare_digest(user_token, expected_token):
    return False
```

### Authorization Review

```python
# Check: Missing authorization
# BAD - no check that user owns resource
@app.route('/api/users/<id>/data')
def get_user_data(id):
    return database.get_user_data(id)

# GOOD - verify ownership
@app.route('/api/users/<id>/data')
@login_required
def get_user_data(id):
    if current_user.id != id and not current_user.is_admin:
        abort(403)
    return database.get_user_data(id)
```

```python
# Check: IDOR (Insecure Direct Object Reference)
# BAD - user controls ID directly
order = Order.query.get(request.args['order_id'])

# GOOD - scope to user
order = Order.query.filter_by(
    id=request.args['order_id'],
    user_id=current_user.id
).first_or_404()
```

### Input Validation Review

```python
# Check: SQL Injection
# BAD
query = f"SELECT * FROM users WHERE name = '{user_input}'"
cursor.execute(query)

# GOOD - parameterized
cursor.execute("SELECT * FROM users WHERE name = ?", (user_input,))
```

```python
# Check: Command Injection
# BAD
os.system(f"convert {user_filename} output.png")

# GOOD - avoid shell, validate input
import subprocess
import re
if not re.match(r'^[a-zA-Z0-9._-]+$', user_filename):
    raise ValueError("Invalid filename")
subprocess.run(['convert', user_filename, 'output.png'], check=True)
```

```python
# Check: Path Traversal
# BAD
path = os.path.join('/uploads', user_filename)
return send_file(path)

# GOOD - validate, resolve, check
import os
base = os.path.realpath('/uploads')
path = os.path.realpath(os.path.join(base, user_filename))
if not path.startswith(base):
    abort(400)
return send_file(path)
```

### Output Encoding Review

```html
<!-- Check: XSS -->
<!-- BAD - unescaped -->
<div>Welcome, {{ user.name }}</div>

<!-- GOOD - escaped (depends on template engine) -->
<div>Welcome, {{ user.name | escape }}</div>
```

```javascript
// Check: DOM XSS
// BAD
element.innerHTML = userInput;

// GOOD
element.textContent = userInput;
// or sanitize if HTML is needed
element.innerHTML = DOMPurify.sanitize(userInput);
```

### Cryptography Review

See [Cryptanalysis](cryptanalysis.md) for detailed crypto review.

Quick checks:
- Using well-known libraries (not custom crypto)
- Appropriate algorithms (AES-GCM, not DES/RC4)
- Proper key management (not hardcoded)
- Secure random for keys/IVs (not Math.random)

### Dependency Review

```bash
# Check for known vulnerabilities
npm audit
cargo audit
pip-audit
bundle audit

# Check for outdated dependencies
npm outdated
cargo outdated
pip list --outdated
```

## Phase 4: Reporting

### Finding Template

```markdown
## [SEVERITY] Finding Title

**Location**: `src/auth/login.py:45`

**Description**:
Brief explanation of the vulnerability.

**Impact**:
What an attacker could do if exploited.

**Proof of Concept**:
Steps to reproduce or example exploit.

**Remediation**:
How to fix it, with code example if helpful.

**References**:
- CWE-XXX
- OWASP reference
```

### Severity Classification

| Severity | Criteria | Example |
|----------|----------|---------|
| **Critical** | Immediate exploitation, high impact | RCE, auth bypass, data breach |
| **High** | Exploitable, significant impact | SQLi, privilege escalation |
| **Medium** | Requires conditions, moderate impact | Stored XSS, CSRF |
| **Low** | Hard to exploit, limited impact | Information disclosure |
| **Info** | Best practice, no direct impact | Missing headers |

### Executive Summary

```markdown
# Security Audit Report

## Executive Summary

Audited: MyApp v2.1.0
Date: 2024-01-15
Auditor: Security Team

### Summary
| Severity | Count |
|----------|-------|
| Critical | 1 |
| High | 3 |
| Medium | 5 |
| Low | 8 |
| Info | 4 |

### Critical Findings
1. SQL Injection in search endpoint (src/api/search.py:23)

### Recommendations
1. Address critical finding immediately
2. Implement parameterized queries throughout
3. Add security headers
4. Enable dependency scanning in CI
```

## Automated Tools

| Tool | Purpose |
|------|---------|
| semgrep | Pattern-based code search |
| bandit | Python security linter |
| brakeman | Ruby/Rails security scanner |
| gosec | Go security checker |
| cargo-audit | Rust dependency audit |
| npm audit | Node.js dependency audit |
| OWASP ZAP | Dynamic analysis (proxy) |
| Snyk | Dependency and code scanning |
| CodeQL | Semantic code analysis |

### Automated vs Manual

| Automated | Manual |
|-----------|--------|
| Known vulnerability patterns | Logic flaws |
| Dependency CVEs | Business logic abuse |
| Common mistakes | Authorization design |
| Configuration issues | Data flow analysis |

Automated tools catch low-hanging fruit. Manual review catches design flaws.

## LLM-Assisted Audit

### Code Analysis Prompt

```
Review this authentication code for security vulnerabilities:

```python
def login(username, password):
    user = db.query(f"SELECT * FROM users WHERE username='{username}'")
    if user and user.password == hashlib.md5(password.encode()).hexdigest():
        session['user_id'] = user.id
        return redirect('/dashboard')
    return render_template('login.html', error='Invalid credentials')
```

Check for:
1. Injection vulnerabilities
2. Cryptographic weaknesses
3. Session management issues
4. Information disclosure
5. Authentication bypass
```

### Systematic Review Prompt

```
Audit this API endpoint against OWASP Top 10:

```python
@app.route('/api/users/<id>', methods=['GET', 'PUT', 'DELETE'])
def user_endpoint(id):
    user = User.query.get(id)
    if request.method == 'GET':
        return jsonify(user.to_dict())
    elif request.method == 'PUT':
        user.update(request.json)
        db.commit()
        return jsonify(user.to_dict())
    elif request.method == 'DELETE':
        db.delete(user)
        db.commit()
        return '', 204
```

For each applicable OWASP category, identify:
1. Whether this code is vulnerable
2. How it could be exploited
3. How to fix it
```

## Agent-Assisted Audit (spore)

```bash
# Use auditor role for systematic review (via spore)
spore @agent --audit "find SQL injection vulnerabilities in src/"

# Structured output format
# $(note SECURITY:HIGH src/api/search.py:23 - SQL injection in user input)
```

The auditor role:
1. Creates systematic audit strategy
2. Searches for vulnerability patterns
3. Reports findings in structured format
4. Suggests remediation

## Common Mistakes in Audits

| Mistake | Why It's Bad | Prevention |
|---------|--------------|------------|
| Only using automated tools | Misses logic flaws | Manual review required |
| No threat model | Auditing everything equally | Define what matters |
| Focusing on code only | Missing config/deployment | Include infra review |
| Not verifying findings | False positives waste time | PoC each finding |
| Audit without context | Missing business logic flaws | Understand the application |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Missed vulnerability | Later discovery/incident | Post-mortem, improve process |
| False positive | Verification fails | Remove from report |
| Incomplete scope | Areas unaudited | Document limitations |
| Stale findings | Already fixed | Re-verify before report |

## See Also

- [Cryptanalysis](cryptanalysis.md) - Deep dive on crypto review
- [Code Review](code-review.md) - General code review process
- [Quality Audit](quality-audit.md) - Non-security code issues

