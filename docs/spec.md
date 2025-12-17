# The Low-Friction Agent Protocol: Complete Specification

## 1. Headless Agent Protocol: Architecture Overview

This document proposes a headless, architecture-first orchestration layer for AI engineering. It contrasts with current "chat-based" coding assistants by prioritizing architectural awareness and verification over conversational fluency.

### Value Proposition

Current tools (VS Code Copilot, etc.) often treat code as raw text, relying on large context windows to "guess" the right edit. This approach suffers from:
* **Context Drift:** Irrelevant files clutter the window.
* **Hallucination:** Models guess file paths or imports.
* **Brittleness:** Edits often break syntax or build steps.

This protocol suggests a **"Compiled Context"** approach:
1.  **Architecture > Syntax:** Understands the project structure (AST) before reading file content.
2.  **Verification > Generation:** Code is verified (compiled/linted) in a silent loop before being shown to the user.
3.  **Governance > Permission:** Agents operate on "Handles" (capabilities) rather than raw files.

### Comparison to SOTA

| Feature | SOTA (Standard Agents) | Proposed Protocol |
| :--- | :--- | :--- |
| **Context** | Raw text / File dumps | Dynamic Views (AST Skeletons, Graphs) |
| **Editing** | Line-based Diff / Rewrite | AST-based Structural Anchors |
| **Memory** | Chat History + Vector RAG | Episodic Log + Semantic Rules |
| **Safety** | "Undo" (Text Buffer) | Shadow Git (Atomic Commits) |

### Tradeoffs

* **Complexity:** Requires language-specific parsers (Tree-sitter) and configuration, unlike generic text-in/text-out models.
* **Latency:** The "Silent Loop" (compile/fix cycles) increases time-to-first-token but aims to reduce time-to-working-code.
* **Rigidity:** Structured editing is safer but may struggle with severely broken/non-parsable code (requires fallback modes).

---

## 2. System Architecture

The system is designed as a modular **Headless Service** exposing an API. It avoids dependency on client-side UI state.

### Core Components

#### A. Event Bus
Handles asynchronous communication between components.
* Events: `UserMessage`, `PlanGenerated`, `ToolCall`, `ValidationFailed`, `ShadowCommit`.

#### B. Plugin System
Core logic is isolated into plugins to support various languages and domains.

* **Context Host:** Manages "View Providers" (e.g., Skeletons, Graphs). It does not natively understand code but delegates to plugins.
* **Structural Editor:** Executes precise edits using AST analysis (Fuzzy Matching, Patching) rather than raw string replacement.
* **Policy Engine:** Intercepts tool calls to enforce safety rules (e.g., "Velocity Checks", "Quarantine").
* **Validator:** Runs the domain-specific verification loop (e.g., `tsc`, linters, tests).

#### C. Configuration Engine
Configuration should be executable code (e.g., TypeScript) rather than static JSON/YAML.
* **Composability:** Allows importing and extending "Distros" (presets of rules and views).
* **Type Safety:** Ensures configuration validity at startup.

#### D. Memory Layer
Separates raw experience from learned logic.
* **Episodic Store:** Log of `(State, Action, Outcome)`. Vector-indexed for retrieval of similar past situations.
* **Semantic Graph:** Generalized rules and patterns derived from episodes (e.g., "Project X requires specific linter rules").
* **Pattern Matcher:** Offline service that scans episodes to promote repeated outcomes into semantic rules.

#### E. Execution Layer (Shadow Git)
Agents operate on a temporary **Shadow Branch**.
* **Atomic Actions:** Every tool call creates a commit.
* **Reversibility:** Allows instant rollback via Git primitives (`reset`).
* **Merge Strategy:** Successful tasks are squashed and merged to the main branch.

### Data Flow

1.  **Input:** User Request -> Config Engine -> Planner.
2.  **Context:** Planner queries Context Host for Views (not raw files).
3.  **Loop:**
    * Draft -> Shadow Git.
    * Trigger Validator.
    * If Error -> Retry/Fix (Internal Loop).
    * If Success -> Commit.
4.  **Output:** Commit Handle returned to User.

---

## 3. Context Engine: Dynamic Views

Context windows are scarce. This module generates a "Compiled View" of the project state rather than relying on raw file dumps or chat history.

### View Provider Protocol

Plugins register **Render Modes** to provide different abstractions of the code/artifact.

* **Interface:** Providers define `supports(target)` and `render(target, options)`.
* **Standard Providers:**
    * **Skeleton:** Class/Function signatures only (AST-based). Low token cost.
    * **Control Flow Graph (CFG):** Logic structure (`if/else`) with elided bodies. Good for debugging.
    * **Dependency Graph:** Import/Export relationships. Good for refactoring.
    * **Elided Literals:** Replaces strings/numbers with placeholders. Good for type fixing.

### Intent-Based Heuristics (DWIM)

To reduce friction, the system attempts to predict the optimal view based on user intent.

* **Explore:** Defaults to `Skeleton` + `DependencyGraph`.
* **Debug:** Defaults to `CFG` + `RawText` (Line Range).
* **Refactor:** Defaults to `DependencyGraph` + `Skeleton`.

### Compilation Pipeline

1.  **Capability Discovery:** Identify which plugins support the target artifact.
2.  **Selection:** Apply heuristics or user overrides to select the View.
3.  **Static Context:** Inject pinned architecture docs or style guides.
4.  **Memory Injection:**
    * **Semantic:** Inject relevant rules (e.g., "This file uses tabs").
    * **Episodic:** Inject summaries of similar past tasks/failures.

### Pattern Matching (Offline)

An asynchronous process scans the Episodic Store for repeated failures.
* *Observation:* "Action X failed 3 times in Context Y."
* *Action:* Generate a Semantic Rule to warn future agents about Context Y.

### Strategy Branching

If the Validator rejects code multiple times, the system interrupts the loop.
* **Action:** Force the model to generate distinct strategies (e.g., "Standard Fix", "Hack", "Refactor") and select one before proceeding.

---

## 4. Agent Capabilities

This section outlines how agents interact with the environment, focusing on tool discovery, generative blueprints, and structural editing.

### Tool Management

* **Discovery:** Agents can search a registry for compatible MCP (Model Context Protocol) servers.
* **Onboarding:** New tools are sandboxed. The agent inspects the schema, generates test scripts, and writes usage rules before the tool is promoted to the main router.

### Bidirectional Blueprints

Agents should configure code rather than writing boilerplate.

* **Concept:** Code is a "render" of a configuration (JSON/YAML).
* **Guard Blocks:** Generated code is wrapped in markers. The system overwrites only these regions.
* **Structural Merging:** An advanced alternative to guard blocks where the system identifies "owned" AST nodes (e.g., class properties) and updates them while preserving user custom logic (methods).

### Structural Editing (Anchor-Based)

To avoid brittle line-number edits, agents use **Anchors** to target code.

* **The Anchor:** Defines target by `Type` (e.g., Function), `Header` (e.g., `function init()`), and `Context` (Parent Class).
* **Resolution:** System scans the AST for the best fuzzy match.
* **Ambiguity:** If multiple nodes match, the system rejects the edit and requests clarification.

### Ad-Hoc Scripting

Agents can write ephemeral scripts to solve computational or logic problems.
* **Use Case:** Verifying a math algorithm, parsing a binary file, or querying a complex API.
* **Lifecycle:** Scripts are sandboxed, executed, and then garbage collected upon task completion.

### Safety Primitives

* **Undo:** Maps to `git reset --hard HEAD~1`.
* **Atomic Commits:** Enforces a one-to-one mapping between tool calls and version control commits for auditability.

---

## 5. Operational Guidelines

Suggested constraints to maximize agent reliability and minimize token waste.

### Context > Persona
Avoid "Expert" personas. Use **In-Context Learning** via `StyleGuide.ts` and `Architecture.md` to define behavior.

### Concept-First Engineering
Encourage modularity by breaking features into atomic **Concepts** with decoupled **Synchronizations**. This reduces the complexity of individual edits.

### Velocity & Verification
* **Velocity Monitor:** Track progress (e.g., decreasing linter errors). If progress stalls or oscillates, interrupt the agent.
* **Quarantine:** Files with parse errors (broken AST) should be locked. Only specialized "Repair" tools should be allowed until the AST is valid.

### Frictionless Handoff
* **Predictive Views:** Don't ask the user which view to load; infer it.
* **Handles:** Agents work with references, loading content only when necessary.

### Response Format
Prioritize payloads (diffs, plans) over conversational filler.

---

## 6. Multi-Agent Orchestration

Standard multi-agent systems suffer from context pollution (sharing full chat history). This protocol proposes a **Ticket-Based** approach.

### The Ticket Protocol

Agents communicate via structured Tickets, effectively treating other agents as microservices.

* **Isolation:** Each agent instance starts with a fresh context window.
* **State Passing:** Context is passed via Handles (references to files/memory), not chat logs.
* **Output:** Agents return a Result Artifact (Summary + Diff Handle).

### Ticket Structure

A Ticket defines:
1.  **Task:** High-level objective.
2.  **Handles:** Specific files or memory entries relevant to the task.
3.  **Constraints:** Rules to respect (e.g., "Do not break API X").

### Workflow

1.  **Delegation:** A Manager/Planner creates a Ticket for a Worker (e.g., "Refactor Auth").
2.  **Execution:** The Worker spins up (fresh context), loads the Handles, performs the Silent Loop (Draft/Verify), and commits to the shadow branch.
3.  **Resolution:** The Worker dies and returns a Handle to the commit/diff.
4.  **Merge:** The Manager reviews the diff and merges it (or requests changes).

### Conflict Resolution
Since agents work on shadow branches, conflicts are handled via Git Merge strategies. The Manager acts as the final arbiter for merge conflicts.

---

## 7. Domain Generalization

This architecture can be abstracted beyond code. The core loop (Draft -> Validate -> Fix) applies to many domains.

### Abstraction Layer

| Code Concept | General Concept |
| :--- | :--- |
| **Compiler/Linter** | **Validator** (Returns structured errors) |
| **Code File** | **Artifact** (Document, Data, Image) |
| **Skeleton View** | **Outline View** (Compressed representation) |
| **Dependency Graph** | **Relationship Graph** (Citations, Links) |

### Domain Drivers

A "Driver" configures the system for a specific domain.

#### 1. Legal Driver
* **Artifact:** Contracts (Markdown/Docx).
* **Views:** Outline (Headers/Definitions), Cross-Reference Graph.
* **Validator:** Logic checks (undefined terms), Compliance checks (clause limits).
* **Blueprints:** Standard Templates (NDA, MSA).

#### 2. Data Analysis Driver
* **Artifact:** SQL, Notebooks.
* **Views:** Database Schema, Data Sample.
* **Validator:** Syntax check, Performance analysis (`EXPLAIN`), PII scan.
* **Loop:** Generate Query -> Run on Sample -> Validate Output -> Refine.

#### 3. Creative Writing Driver
* **Artifact:** Narrative Text.
* **Views:** Story Arc (Timeline), Character Graph.
* **Validator:** Consistency check (Timeline errors), Style check.

### Dynamic Validators
Validators can be:
1.  **Static:** Binaries (Linters).
2.  **Dynamic:** Tests (Unit/Integration).
3.  **Formal:** Logic solvers (Z3).
4.  **Ad-Hoc:** Agent-written scripts for specific one-off verification tasks.
