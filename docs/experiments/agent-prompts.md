# Agent Prompt Experiments

Tracking prompt iterations and their effectiveness at preventing pre-answering.

## Problem Statement

LLMs pre-answer: they output commands and `$(done)` in the same turn, answering before seeing command results. This happens because LLM training rewards task completion - if the task *looks* complete in 1 turn, the LLM does 1 turn.

**Root cause**: Task framing makes single-turn look like correct completion.
**Fix direction**: Reframe task so multi-turn IS correct completion.

## Baseline: Original Prompt (pre-8e33340)

Unknown state. No logs available.

## Experiment 1: Simple Prompt (commit 8e33340)

```
Coding session. Output commands in [cmd][/cmd] tags. Conclude quickly using done.
[commands]
[cmd]done answer here[/cmd]
...
```

**Model**: Gemini Flash
**Result**: "Flash typically needs 4-5 turns but now concludes reliably"
**Context model**: Conversational (append-only chat history)
**Analysis**: Simple prompt, no memory management. Worked but used problematic conversational model.

## Experiment 2: Memory Management + $(wait) (current, pre-investigator)

```
Coding session. Output commands in $(cmd) syntax. Multiple per turn OK.

Command outputs disappear after each turn. To manage context:
- $(keep) or $(keep 1 3) saves outputs to working memory
- $(note key fact here) records insights for this session
...
- $(wait) waits for command results before answering

IMPORTANT: If you issue commands that produce the answer, use $(wait) to see results first.
DO NOT call $(done) in same turn as commands that contain the answer.
```

**Model**: Various (Claude, Gemini)
**Result**: Pre-answering still occurs. $(wait) is a band-aid (post-processing).
**Context model**: Ephemeral (1-turn visibility window)
**Analysis**:
- Adding $(wait) instruction doesn't prevent pre-answering
- Complex memory management may distract from core task
- Prompt says "don't do X" but doesn't reframe what correct completion looks like

## Experiment 3: Investigator Role

### 3a: Initial (verbose, no example)

First attempt used verbose prompt with many memory commands. Claude ignored `$(cmd)` syntax entirely - used XML function calls and hallucinated the answer.

**Session**: renuh3aq
**Model**: Claude (anthropic)
**Result**: FAILURE - Used XML syntax, hallucinated fake file names

### 3b: Simplified with concrete example (current)

```
You are a code investigator. Gather evidence, then conclude.

Output commands using $(command) syntax. Example turn:
$(view .)
$(text-search "main")
$(note found entry point in src/main.rs)

WORKFLOW:
1. GATHER - Run commands to explore
2. RECORD - $(note) findings you discover
3. CONCLUDE - $(done answer) citing evidence

Commands:
$(view path) $(view path/symbol) $(view --types-only path)
$(text-search "pattern")
$(run shell command)
$(note finding)
$(done answer citing evidence)

Outputs disappear each turn unless you $(note) them.
Conclusion must cite evidence. No evidence = keep investigating.
```

**Hypothesis**: Concrete example + role framing + evidence requirement = multi-turn behavior.

### Design Rationale

1. **Concrete example**: Shows exact syntax - prevents XML function call default
2. **Role framing**: "investigator" makes gathering evidence THE job
3. **Workflow steps**: Multi-turn baked into structure (GATHER → RECORD → CONCLUDE)
4. **Evidence requirement**: "Conclusion must cite evidence" - positive framing
5. **Simplified commands**: Only essential commands listed
6. **No negative instructions**: No "don't do X", only positive guidance

---

## Session Log

Format: `session_id | model | task | turns | correct | notes`

| Session | Model | Task | Turns | Correct | Notes |
|---------|-------|------|-------|---------|-------|
| renuh3aq | claude | count lua scripts | 1 | NO | 3a prompt: Used XML syntax, hallucinated answer |
| uz6b5k9p | claude | count lua scripts | 1 | NO | 4a prompt: XML tags, hallucinated (no syntax example) |
| 7d489957 | claude | count lua scripts | 1 | NO | 4b prompt: XML tags, hallucinated ("not XML" instruction ignored) |
| 4n7je3d9 | claude | count lua scripts | 1 | NO | 4c prompt: XML tags, hallucinated ("text conversation" framing ignored) |
| y56rz3tq | claude | count lua scripts | 1 | NO | 4d prompt: XML tags, hallucinated (single "Example: $(view .)" not enough) |
| mgwx9cdy | claude | count lua scripts | 1 | NO | 4e prompt: XML tags, hallucinated (multi-line syntax ref without narrative) |
| spqh8mh6 | claude | count lua scripts | 3 | YES | 5a prompt: Correct syntax, correct answer |
| n85yswq8 | claude | main binary crate | 1 | NO | 5a prompt: Pre-answered all cmds in 1 turn, hallucinated "goose" |
| mj9d4ktm | claude | rust edition | 13 | YES | 5a prompt: Correct but severe looping (viewed Cargo.toml 5+ times) |
| 74myszjv | claude | count Provider variants | 9 | YES | 5a prompt: Correct, used $(note) properly |
| 84gmtqny | claude | count lua scripts | 2 | YES | 3b prompt: Correct syntax, answer, cited evidence |
| s9evceus | claude | find Anthropic default model | 8 | YES | Took many turns, some looping on $(view .), but correct answer with line citation |
| g4n93rvr | claude | count Provider enum variants | 5 | YES | Correct: 13 variants, all named, cited line numbers |

### Summary (Experiment 3b)

**Results**: 3/3 correct with new prompt (vs 0/1 with 3a prompt)
**Turns**: 2-8 (multi-turn as intended, no pre-answering)
**Key insight**: Concrete example in prompt prevents LLM from defaulting to XML function calls

### Summary (Experiment 5a - restored example format)

**Results**: 3/4 correct (75%) - NOT reliable
**Issues observed**:
- Pre-answering still happens (n85yswq8: all commands + done in 1 turn)
- Severe looping (mj9d4ktm: viewed Cargo.toml 5+ times, 13 turns)
- Turn count varies wildly (3-13 for similar complexity)

---

## Fundamental Analysis

### Why does pre-answering happen despite being counterproductive?

1. **LLM goal mismatch**: Trained for "plausible text", not "correct answers"
   - Next-token prediction optimizes for likelihood, not correctness
   - "Looks correct" ≠ "is correct" - LLM can't distinguish during generation

2. **Commands as narrative, not actions**: LLM writes a story ABOUT solving the problem
   - "I'll view X, then search Y, then conclude Z" - all in one generation
   - Treats commands as rhetorical steps, not actual actions with real outputs

3. **No feedback during generation**: Once tokens flow, no error signal
   - Can't "realize" mid-response that it's hallucinating
   - Just produces most likely continuation

4. **Training data bias**: Direct Q&A dominates over tool-use-with-outputs
   - Pattern "question → answer" is deeply ingrained
   - Tool use is small fraction of training data

5. **Not using standard tool format**: Our $(cmd) syntax isn't what models are trained on
   - Claude/GPT are trained on specific tool-use formats (function calling, XML)
   - Custom syntax may not trigger proper tool-use behavior

### What makes the model switch between modes?

Unknown. Possible factors:
- Question phrasing ("what is X" vs "how many X")
- Prior knowledge confidence (common patterns vs specific details)
- Sampling randomness
- Attention patterns
- The example itself may teach pre-answering (shows commands + findings in one "turn")

### Potential solutions to explore

1. **Stop sequences**: Force generation to halt after commands (limited provider support)
2. **Standard tool format**: Use native function calling instead of custom syntax
3. **Streaming + interrupt**: Stop generation when command detected
4. **Architecture**: Build actual pause points into the system
5. **Training**: Fine-tune on proper tool-use traces (expensive)

## Experiment 6: Prose-based commands

**Hypothesis**: Natural language intentions ("I want to view X") are deeply ingrained in LLM training.
"I want to..." implies waiting for the thing. Might prevent pre-answering.

```
Unfamiliar codebase. Express intentions, I will show results.

I want to view <path>
I want to search for "<pattern>"
I want to run <shell command>
I note: <finding>
My conclusion is: <answer>

End with "next turn:" until you reach your conclusion.
```

| Session | Model | Task | Turns | Correct | Notes |
|---------|-------|------|-------|---------|-------|
| ywfyfxxc | claude | count lua scripts | 2 | YES | Prose parsed, correct answer |
| 6ajpcbq8 | claude | main binary | 1 | NO | Pre-answered all "Turn X:" as narrative, hallucinated |
| s4upd5kn | claude | count Provider variants | 1 | NO | Hallucinated XML outputs + file content, wrong answer (4 vs 13) |

**Result**: 1/3 correct (33%) - WORSE than previous

**Analysis**: Prose syntax parsed correctly, but:
- LLM still writes complete narrative including imagined outputs
- Generates fake XML tags for outputs (<read_file>, <search>)
- "next turn:" ignored - LLM writes "Turn 1:", "Turn 2:" as story beats
- Hallucination of file contents inline

## Experiment 7: Minimal "next command" prompt

**Hypothesis**: Ultra-simple prompt might prevent overthinking.

Variations tried:
- "Answer with one or more commands. I will show you the output." - still XML + hallucination
- "Respond with commands to explore this codebase." - still XML + hallucination
- "Respond with your next command." - still XML + hallucination

**Result**: All failed. Model consistently:
1. Uses XML function-call format despite $(cmd) in examples
2. Outputs multiple commands in one response
3. Hallucinates command outputs inline
4. Pre-answers based on hallucinated data

**Key observation**: No prompt variation has successfully prevented the model from:
- Defaulting to XML format (its trained function-calling syntax)
- Generating imagined outputs alongside commands
- Completing the entire task in one generation

The model treats every prompt as "write a complete story about solving this task."

