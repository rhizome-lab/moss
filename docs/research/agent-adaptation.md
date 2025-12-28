# Agent Adaptation Framework

Research notes on "Adaptation of Agentic AI" (Stanford, Harvard, Berkeley, Caltech).

- **Paper**: https://arxiv.org/abs/2512.16301 ([HTML](https://arxiv.org/html/2512.16301v2))
- **Repo**: https://github.com/pat-jj/Awesome-Adaptation-of-Agentic-AI (150+ papers)
- **Date**: December 2025

## Core Thesis

Demos use static systems. Production requires continuous adaptation.

Adaptation is "the central mechanism for improving performance, reliability, and generalization" in agentic AI.

## Why Agents Fail in Production

1. **Tool misalignment**: Agents select inappropriate tools when tools lack task-specific optimization
2. **Action distribution mismatch**: Agent preferences diverge from tool capabilities
3. **Feedback sparsity**: Output-only signals provide insufficient learning information
4. **Generalization gaps**: Tools trained on narrow distributions fail on diverse agent queries

## The 4 Adaptation Paradigms

Two dimensions: what adapts (agent vs tool) × signal source (tool execution vs final output).

| Paradigm | What Adapts | Signal Source | Best For |
|----------|-------------|---------------|----------|
| **A1** | Agent | Tool execution feedback | Tool-intensive tasks (rich intermediate feedback) |
| **A2** | Agent | Final output only | Complex reasoning (simpler implementation) |
| **T1** | Tools | Independent of agent | General-purpose tools (reusable across agents) |
| **T2** | Tools | Agent-supervised | Specialized tasks (agent-specific optimization) |

### A1: Tool Execution Signaled Agent Adaptation

Agent improves based on tool execution outcomes.

Examples:
- **Orion**: GRPO for information retrieval agents
- **DeepSeek-Prover-V2**: GRPO + SFT for theorem proving with Lean compiler
- **Tool-R1**: GRPO for multimodal QA with code execution feedback

### A2: Agent Output Signaled Agent Adaptation

Agent adapts based on final output quality, not intermediate tool feedback.

Examples:
- **VerlTool**: Multi-domain agent (math, SQL, web search) via GRPO
- **Self-RAG**: Self-reflective retrieval-augmented generation
- **DeepSeek-R1-Zero**: Pure reasoning improvement without tools
- **TextGrad**: Gradient-based prompt optimization

### T1: Agent-Agnostic Tool Adaptation

Tools improve independently, reusable across agents.

Examples:
- **CLIP**: Vision-language alignment
- **DPR**: Dense passage retrieval
- **AlphaFold2**: Protein structure prediction
- **HuggingGPT**: Multi-model tool orchestration

### T2: Agent-Supervised Tool Adaptation

Tools improve through agent-generated training signals.

Examples:
- **s3**: Small retriever adapted via agent feedback using PPO
- **RA-DIT**: Knowledge-intensive tools supervised by LLM agents
- **Proxy-Tuning**: Tool adaptation using larger model guidance

## Practical Recommendation

Combine:
- **Rare A1/A2 updates** on strong base models
- **Frequent T1/T2 adaptation** of retrievers, search policies, simulators, memory

This balances stability (don't break the base model) with responsiveness (tools stay current).

## Popular Training Methods

- **GRPO** (Group Relative Policy Optimization): Dominant RL approach across 40+ papers
- **SFT** (Supervised Fine-tuning): Foundation for most agent adaptation
- **PPO/DPO**: Preference-based learning for agent-tool alignment
- **Test-time methods**: Gradient-based and contrastive refinement at inference

## Relevance to Moss

**Current state:**
- Moss tools (view, analyze, grep) are **T1** - agent-agnostic, reusable across any LLM
- Index refresh is **T1 adaptation** - tools improve independently via file watching
- No agent adaptation (A1/A2) - moss doesn't fine-tune LLMs

**Implications:**
- T1 is the right choice for moss: general-purpose tools work with any agent
- If moss ever needed specialization, T2 (agent-supervised tool adaptation) would be the path
- A1/A2 require fine-tuning LLMs, which is outside moss's scope (use the best available models)

**Design validation:**
- "Tool misalignment" is exactly what moss's structural tools address - give agents better tools, not more context
- "Generalization gaps" supports moss's approach of broad language support (98 languages)
- Framework confirms moss's implicit strategy: invest in T1 (tool quality), outsource A1/A2 to model providers

**Open questions:**
- Should moss track tool usage patterns to identify misalignment?
- Could moss provide feedback signals for A1 adaptation? (e.g., which tool calls succeeded/failed)
- Is there value in T2-style adaptation where moss tools specialize to a specific agent's patterns?
