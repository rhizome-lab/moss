# Moss Roadmap

See `CHANGELOG.md` for completed features (Phases 15-29).

See `~/git/prose/moss/` for full synthesis design documents.

## Future Work

### Interface Generators

Additional interface generators for the library-first architecture:

- [ ] `moss.gen.lsp` - Generate LSP handlers from API
- [ ] `moss.gen.grpc` - Generate gRPC proto + handlers from API
- [ ] `moss-lsp` entry point (requires `[lsp]` extra)
- [ ] Unix socket transport for local high-performance server

### Non-LLM Code Generators

Alternative synthesis approaches that don't rely on LLMs. See `docs/synthesis-generators.md` for details.

#### High Priority
- [ ] `EnumerativeGenerator` - enumerate ASTs, test against examples (Escher/Myth)
- [ ] `ComponentGenerator` - combine library functions bottom-up (SyPet/InSynth)
- [ ] `SMTGenerator` - Z3-based type-guided synthesis (Synquid)

#### Medium Priority
- [ ] `PBEGenerator` - Programming by Example (FlashFill/PROSE)
- [ ] `SketchGenerator` - fill holes in user templates (Sketch/Rosette)
- [ ] `RelationalGenerator` - miniKanren-style logic programming

#### Research/Experimental
- [ ] `GeneticGenerator` - evolutionary search (PushGP)
- [ ] `NeuralGuidedGenerator` - small model guides enumeration (DeepCoder)
- [ ] `BidirectionalStrategy` - λ²-style type+example guided search

### DreamCoder-style Learning

Advanced abstraction discovery:

- [ ] Compression-based abstraction discovery
- [ ] MDL-based abstraction scoring

### Multi-Language Expansion

- [ ] Full TypeScript/JavaScript synthesis support
- [ ] Go and Rust synthesis strategies

### External Dependency Analysis

Analyze PyPI/npm dependencies (not just internal imports):

- [x] Parse pyproject.toml/requirements.txt for dependencies (`moss external-deps`)
- [x] Resolve full dependency tree (transitive dependencies) (`--resolve` flag)
- [x] Show dependency weight (how many sub-dependencies each brings)
- [x] Identify heavy/bloated dependencies (`--warn-weight` threshold)
- [x] Check for known vulnerabilities (`--check-vulns` via OSV API)
- [x] License compatibility checking (`--check-licenses` flag)
- [x] package.json/npm support (dependencies, devDependencies, optional, peer)

### CLI Output Enhancement

Token-efficient output modes for AI agent consumption:

- [x] `--compact` flag for single-line summaries (e.g., `deps: 5 direct | vulns: 0 | licenses: ok`)
- [ ] `--jq EXPR` flag - pipe JSON output through jq for field extraction
- [ ] `--query EXPR` flag - relaxed DWIM syntax for flexible querying (needs design work)

The `--jq` option is straightforward (shell out to jq). The `--query` variant would allow more natural queries like `"direct deps"` or `"high vulns"` but requires careful design to handle fuzzy matching.

### Enterprise Features

- [ ] Team collaboration (shared caches)
- [ ] Role-based access control

---

## Vision: Augmenting the Vibe Coding Loop

Moss should both **replace** and **augment** conventional AI coding assistants like Claude Code, Gemini CLI, and Codex CLI. The goal is not to compete on chat UX, but to provide the structural awareness layer that makes any agentic coding loop more reliable:

- **As a replacement**: `moss run` can orchestrate full tasks with verification loops, shadow git, and checkpoint approval
- **As an augmentation**: Tools like `moss skeleton`, `moss deps`, `moss check-refs`, `moss mutate` can be called by *any* agent to get architectural context before making changes
- **MCP integration**: `moss-mcp` exposes all capabilities to MCP-compatible agents

The key insight: vibe coding works better when the agent understands structure, not just text.

---

## Notes

### PyPI Naming Collision

There's an existing `moss` package on PyPI (a data science tool requiring numpy/pandas). Before publishing, we need to either:
- Rename the package (e.g., `moss-tools`, `moss-orchestrate`, `toolmoss`)
- Check if the existing package is abandoned and claim the name
- Use a different registry
