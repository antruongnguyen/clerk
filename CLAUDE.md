# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Clerk is an MCP (Model Context Protocol) server written in Rust using the `rmcp` SDK. It provides tools for managing and organizing personal notes, todos, and technical documents. Data is stored as local markdown files in `~/.clerk/` with a knowledge-graph-like structure linking documents through tags and categories. The server supports both STDIO and HTTP transports for AI agent interaction.

See `docs/ideas.md` for the full product vision and design details.

## Technology Stack

- **Language:** Rust, edition 2024, resolver 3
- **Async runtime:** tokio
- **MCP SDK:** rmcp (features: `server`, `schemars`, `transport-io`, `transport-streamable-http-server`)
- **HTTP framework:** axum
- **Serialization:** serde / serde_json
- **Logging:** tracing + tracing-subscriber (JSON format to stderr)
- **Error handling:** anyhow (in main), rmcp::ErrorData (in tool handlers)

Reference implementations for architecture patterns:
- `/Users/I756434/Projects/mcp/mcp-safeshell` — transport setup, tool macros, config loading
- `/Users/I756434/Projects/mcp/mcp-bmad-method` — workspace layout, resource templates

## Build & Development Commands

```bash
cargo build                  # Debug build
cargo build --release        # Optimized release build
cargo test                   # Run all tests
cargo test <test_name>       # Run a single test
cargo clippy                 # Lint (must pass with zero warnings)
cargo fmt                    # Format code
cargo install --path .       # Install locally
```

## Code Conventions

- All code must pass `cargo clippy` with zero warnings.
- Use `tracing` for all logging — never `println!` or `eprintln!` directly.
- Logs go to stderr; stdout is reserved for MCP protocol messages.
- MCP tools are defined using `#[tool]` and `#[tool_handler]` macros from rmcp.
- Schema generation uses `schemars::JsonSchema` derive.
- Transport mode (stdio vs http) is selected via CLI arguments.
- Markdown file content should stay under ~10,000 characters; split larger content across files.

## Design Constraints

- All user data lives in `~/.clerk/` as markdown files — no database.
- Files must be structured for efficient AI agent consumption (clear naming, consistent frontmatter format).
- Documents support: date, tags, categories, and optional summary/abstract.
- Related documents are linked through tags/categories forming a navigable knowledge graph.

## Behavioral Guidelines

- **Think before coding.** State assumptions explicitly. If multiple interpretations exist, present them — don't pick silently.
- **Simplicity first.** Minimum code that solves the problem. No speculative abstractions, no features beyond what was asked, no error handling for impossible scenarios.
- **Surgical changes.** Touch only what you must. Match existing style. Don't "improve" adjacent code. Remove only imports/variables that YOUR changes made unused.
- **Goal-driven execution.** Transform tasks into verifiable goals with success criteria. For multi-step tasks, state a brief plan with verification checks at each step.

## Git Rules

- Never include `Co-authored-by:` lines or AI attribution in commit messages.
- No "Generated with Claude Code" footers in commits or PRs.
