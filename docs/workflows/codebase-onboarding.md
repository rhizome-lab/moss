# Codebase Onboarding Workflow

Systematically exploring and understanding an unfamiliar codebase - for new team members, taking over maintenance, or LLMs encountering a project for the first time.

## Trigger

- Joining a new team/project
- Taking over maintenance of legacy code
- Reviewing a large PR in unfamiliar area
- LLM needs to understand codebase to assist
- Auditing or evaluating external code

## Goal

- Mental model of how the system works
- Know where to find things
- Understand key abstractions and patterns
- Can navigate confidently to relevant code
- Enough context to make changes safely

## Prerequisites

- Access to the code
- Ability to build and run (ideally)
- Some documentation (even if outdated)
- Time to explore (onboarding isn't instant)

## Why Codebase Onboarding Is Hard

1. **No map**: Large codebases have thousands of files - where to start?
2. **Implicit knowledge**: Architecture decisions live in developers' heads
3. **Outdated docs**: READMEs rot, comments lie
4. **Emergent patterns**: Conventions exist but aren't documented
5. **Domain complexity**: Code embeds business logic you don't know yet
6. **Layers of history**: Current state reflects years of accumulated decisions

## What to Discover

| Category | Questions |
|----------|-----------|
| **Structure** | What are the major components? How are files organized? |
| **Entry Points** | Where does execution start? What are the public interfaces? |
| **Data Flow** | How does data move through the system? |
| **Key Abstractions** | What are the core types/classes? What patterns are used? |
| **Build & Deploy** | How to build, test, run, deploy? |
| **Domain** | What business concepts are encoded? What's the glossary? |
| **History** | Why is it structured this way? What decisions shaped it? |

## Core Strategy: Survey → Trace → Map → Verify

```
┌─────────────────────────────────────────────────────────┐
│                      SURVEY                              │
│  High-level: docs, structure, build system              │
│  Get the lay of the land without going deep             │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      TRACE                               │
│  Follow specific paths: request handling, key flows     │
│  Depth-first exploration of important code              │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       MAP                                │
│  Build mental model: components, relationships          │
│  Document as you learn                                  │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      VERIFY                              │
│  Test understanding: make small changes, predict        │
│  behavior, ask questions                                │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Survey (Broad, Shallow)

### Start with Documentation

```bash
# What exists?
ls -la README* CONTRIBUTING* ARCHITECTURE* docs/

# Read in order:
# 1. README.md - project overview, quick start
# 2. CONTRIBUTING.md - development setup
# 3. docs/architecture.md or similar - system design
# 4. CHANGELOG.md - what's been changing recently
```

Even outdated docs provide vocabulary and intent.

### Understand the Build System

```bash
# What kind of project?
ls package.json Cargo.toml go.mod pyproject.toml Makefile CMakeLists.txt

# Read the build config
cat package.json  # dependencies, scripts
cat Cargo.toml    # dependencies, features, workspaces

# What are the build commands?
cat Makefile      # or justfile, build.sh, etc.
npm run           # List available scripts
```

### Survey Directory Structure

```bash
# Top-level layout
tree -L 2 -d

# Or if large:
ls -la
ls src/
ls lib/
```

**Common patterns**:
```
src/           # Source code
lib/           # Libraries/packages
tests/         # Test files
docs/          # Documentation
scripts/       # Build/utility scripts
config/        # Configuration files
bin/           # Executables
cmd/           # Go: command entry points
internal/      # Go: private packages
pkg/           # Go: public packages
crates/        # Rust: workspace members
packages/      # Monorepo: sub-packages
```

### Identify Entry Points

```bash
# Where does execution start?

# Binary entry points
grep -r "fn main" --include="*.rs"
grep -r "func main" --include="*.go"
grep -r "if __name__" --include="*.py"

# Web entry points
grep -r "app.listen\|createServer" --include="*.js"
grep -r "@app.route\|@router" --include="*.py"

# Library entry points
cat src/lib.rs | head -50     # Rust: public exports
cat src/index.ts | head -50   # TS: main exports
cat __init__.py               # Python: package exports
```

### Get It Running

Nothing beats actually running the code:

```bash
# Follow setup instructions
npm install && npm run dev
cargo build && cargo run
docker-compose up

# Run tests to see what exists
npm test
cargo test
pytest
```

## Phase 2: Trace (Narrow, Deep)

### Pick Representative Flows

Choose 2-3 important paths to trace end-to-end:

```
Web app:
- HTTP request → response (one endpoint)
- User action → database change

CLI tool:
- Command invocation → output

Library:
- Public API call → result
```

### Trace a Request

```bash
# Find the entry point for a specific endpoint
grep -r "POST /users" --include="*.py"
grep -r '"/api/users"' --include="*.ts"

# Found: src/routes/users.ts:15
# Now trace the call chain:
# 1. Route handler
# 2. Controller/service
# 3. Data access
# 4. Response formation
```

### Use Debugger/Logging

```python
# Add strategic breakpoints
import pdb; pdb.set_trace()

# Or trace with logging
import logging
logging.basicConfig(level=logging.DEBUG)
```

```bash
# Run with verbose/debug mode
npm run dev -- --debug
cargo run -- -vvv
RUST_LOG=debug cargo run
```

### Trace Dependencies

```bash
# What does this module depend on?
grep "^import\|^from" src/users/service.py
grep "^use " src/users/service.rs

# What depends on this module?
grep -r "from users.service import" .
grep -r "use crate::users::service" .
```

## Phase 3: Map (Synthesize Understanding)

### Build Component Diagram

As you explore, sketch the major components:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Routes    │────▶│  Services   │────▶│    Data     │
│  (HTTP)     │     │  (Logic)    │     │  (Storage)  │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       └───────────────────┴───────────────────┘
                    Shared Types
```

### Document Key Abstractions

```markdown
# Key Types

## User (src/models/user.rs)
Core domain entity. Has id, email, profile.
Created via UserService.create_user().

## Repository<T> (src/db/repository.rs)
Generic data access pattern. All entities use this.
Provides: find, create, update, delete.

## Middleware (src/middleware/)
Request processing pipeline: auth, logging, rate limiting.
Order matters - defined in src/app.rs.
```

### Note Patterns and Conventions

```markdown
# Patterns Used

## Error Handling
- All errors use `thiserror` derive
- Services return Result<T, ServiceError>
- HTTP layer converts to appropriate status codes

## Testing
- Unit tests: same file, `#[cfg(test)]` module
- Integration tests: tests/ directory
- Fixtures: tests/fixtures/

## Configuration
- Environment variables for secrets
- config.toml for non-sensitive defaults
- Loaded at startup in src/config.rs
```

### Create a Glossary

Domain terms that have specific meaning in this codebase:

```markdown
# Glossary

- **Tenant**: A customer organization (multi-tenant system)
- **Workspace**: A project within a tenant
- **Pipeline**: A sequence of processing steps
- **Stage**: One step in a pipeline
- **Artifact**: Output of a stage
```

## Phase 4: Verify Understanding

### Make a Small Change

The best test of understanding: change something.

```
1. Pick a small, safe change (add log line, tweak message)
2. Predict what will happen
3. Make the change
4. Run tests
5. Was your prediction correct?
```

### Explain It to Someone

Rubber duck debugging, but for understanding:

```
"This is a web service that processes orders.
Requests come in through src/api/, get validated,
then OrderService handles the business logic,
which calls the PaymentGateway and updates the DB.
The tricky part is..."
```

If you can't explain it, you don't understand it.

### Ask Questions

For human onboarding, prepare specific questions:

```
Good: "I see OrderService calls PaymentGateway.process() -
       what happens if the payment fails after we've
       decremented inventory?"

Bad: "How does this work?"
```

Specific questions show you've done homework and need targeted help.

## LLM-Specific Techniques

LLMs (and LLM-powered tools like moss) can accelerate onboarding:

### Automated Survey

```bash
# Get project structure
find . -type f -name "*.rs" | head -100

# Get module overview
for f in src/*.rs; do head -20 "$f"; echo "---"; done

# Find key types
grep -r "^pub struct\|^pub enum" --include="*.rs"

# Find entry points
grep -r "^pub fn\|^pub async fn" --include="*.rs" src/lib.rs src/main.rs
```

### Targeted Questions

Feed context, ask specific questions:

```
Given:
- src/api/routes.rs (entry points)
- src/services/order.rs (business logic)
- src/db/models.rs (data model)

Questions:
1. What's the flow for creating an order?
2. Where is payment validation done?
3. What happens on error?
```

### Iterative Exploration

```
Round 1: "What are the main modules in src/?"
Round 2: "Tell me about src/services/ - what does each file do?"
Round 3: "Trace order creation from API to database"
Round 4: "What error handling patterns are used?"
```

### Build Understanding Artifacts

LLM can help generate:
- Component diagrams (as ASCII or Mermaid)
- Glossary from code patterns
- Flow diagrams for key paths
- Summary documents

## Onboarding Checklist

### Day 1: Environment
- [ ] Clone repo, install dependencies
- [ ] Build the project
- [ ] Run tests
- [ ] Run the application locally
- [ ] Read README and key docs

### Week 1: Survey
- [ ] Understand directory structure
- [ ] Identify entry points
- [ ] Know the build/test/deploy commands
- [ ] Read through main configuration
- [ ] Trace one complete flow end-to-end

### Month 1: Depth
- [ ] Understand key abstractions
- [ ] Know the major patterns used
- [ ] Can navigate to relevant code for a given feature
- [ ] Made several small changes successfully
- [ ] Have a mental model of the architecture

### Ongoing: Mastery
- [ ] Understand historical decisions (why, not just what)
- [ ] Can design new features fitting existing patterns
- [ ] Can identify and propose improvements
- [ ] Can onboard others

## Common Mistakes

| Mistake | Why It's Bad | Instead |
|---------|--------------|---------|
| Reading everything | Overwhelming, no retention | Survey broadly, trace deeply |
| Starting with details | No context for details to fit | Start with structure |
| Ignoring tests | Miss usage examples | Tests show how code is meant to be used |
| Only reading, no running | Passive understanding | Run, debug, modify |
| No note-taking | Forget what you learned | Document as you go |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Can't build | Build errors | Ask for help, check CI config |
| Docs are wrong | Reality differs from docs | Trust code, update docs |
| Too much complexity | Overwhelmed | Focus on one slice |
| No architecture docs | Don't know high-level design | Create them as you learn |

## Anti-patterns

- **Boiling the ocean**: Trying to understand everything before doing anything
- **Cargo cult setup**: Copying commands without understanding
- **Premature optimization of understanding**: Learning details you don't need yet
- **Isolation**: Not asking questions when stuck

## Accelerators

1. **Find a guide**: Someone who knows the codebase and will answer questions
2. **Pair programming**: Work on a task with experienced team member
3. **Good first issues**: Curated tasks designed for newcomers
4. **Architecture docs**: Even partial docs help enormously
5. **Tests as documentation**: Well-written tests show intended usage

## Open Questions

### Optimal Exploration Order

Is there an optimal order for exploring a codebase?
- Top-down (docs → structure → code)?
- Bottom-up (trace one path deeply, then broaden)?
- Hybrid (survey, then depth, then survey again)?

Probably depends on learning style and codebase characteristics.

### Measuring Understanding

How do you know when you've "understood enough"?
- Can make changes without breaking things?
- Can answer questions about the code?
- Can predict behavior before running?

### LLM Context Limits

For LLMs exploring large codebases:
- What's the right granularity for context?
- How to summarize and retain understanding across sessions?
- How to know when more exploration is needed vs. just asking?

## See Also

- [Question Answering](question-answering.md) - Answering specific questions about code
- [Codebase Orientation](codebase-orientation.md) - Related, more focused on structure
- [Bug Investigation](bug-investigation.md) - Often involves targeted exploration
