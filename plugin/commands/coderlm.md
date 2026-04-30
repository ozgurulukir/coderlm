---
name: coderlm
description: Explore a codebase using tree-sitter-backed indexing. Use when you need to understand how code works, trace execution paths, find where errors originate, or understand the sequence of events that produce a particular outcome. Prefer this over grep/glob/read for structural code questions.
---

# CodeRLM — Structural Codebase Exploration

You have access to a tree-sitter-backed index server that knows the structure of this codebase: every function, every caller, every symbol. Use it instead of guessing with grep.

## Setup

```bash
# Initialize a session (do this once per project)
python3 .claude/coderlm_state/coderlm_cli.py init
```

## Tools

```bash
CLI=".claude/coderlm_state/coderlm_cli.py"

python3 $CLI structure                          # File tree + module overview
python3 $CLI stats                              # Server status + cache hit rates
python3 $CLI search "symbol_name"               # Find symbols by name
python3 $CLI impl function_name --file path     # Get exact implementation
python3 $CLI callers function_name --file path  # Who calls this function?
python3 $CLI tests --file path                  # Find tests covering this file
python3 $CLI grep "pattern"                     # Scope-aware pattern search
python3 $CLI peek path --start N --end N        # Read a specific line range
```

## How to Explore

Do not scan files looking for relevant code. Instead, work the way a human engineer traces through a codebase:

**Start from an entrypoint.** Every exploration begins somewhere concrete — an error message, a function name, an API endpoint, a log line. Use `grep` or `search` to locate that entrypoint in the index.

**Trace the path.** Once you've found the entrypoint, use `callers` to understand what invokes it and `impl` to read what it does. Follow the chain: what calls this? What does that caller do? What state does it pass in? Build a mental model of the execution path, not a list of files.

**Understand the sequence of events.** The goal is to reconstruct the causal chain: what had to happen in order to produce the state you're looking at. This means tracing upstream (what called this, and with what arguments?) and sometimes downstream (what happens after this point, and does it matter?).

**Stop when you have the narrative.** You're done exploring when you can explain the path from trigger to outcome — not when you've read every related file.

## What This Replaces

Without the index, Claude Code explores by globbing for filenames, grepping for strings, and reading entire files hoping to find relevant sections. That works, but it's wasteful and produces false confidence — you see code near your search term but miss the actual execution path.

With the index, you get:
- **Symbol search** instead of string matching — find the function, not every comment mentioning it
- **Caller chains** instead of grep-and-hope — know exactly what invokes a function
- **Exact implementations** instead of full-file reads — get the function body, not 500 lines of context
- **Test discovery** — find what's already tested before writing new tests

## $ARGUMENTS