# Clerk MCP Server

A personal information management MCP (Model Context Protocol) server written in Rust. Clerk manages notes, todos, and technical documents as local markdown files in `~/.clerk/`, with a knowledge-graph structure linking items through tags and categories.

## Features

- **Notes** -- Create, read, update, and delete general-purpose notes
- **Todos** -- Track tasks with status (pending/in_progress/done), priority, and due dates
- **Documents** -- Manage longer-form technical documents with summaries
- **Search** -- Full-text search across all items (title, tags, category, content)
- **Discovery** -- Browse by tags, categories, find related items by shared tags
- **MCP Resources** -- Browse `clerk://` URIs for structured summaries
- **Dual transport** -- STDIO (for Claude Desktop, editors) and HTTP (for web clients)

## Installation

```bash
cargo install --path .
```

## Usage

### STDIO transport (default)

```bash
clerk-mcp
# or explicitly:
clerk-mcp --transport stdio
```

### HTTP transport

```bash
clerk-mcp --transport http
# Custom bind address:
clerk-mcp --transport http --bind 127.0.0.1:8080
```

## Configuration

Clerk reads configuration from `~/.clerk/config.toml`:

```toml
data_dir = "~/.clerk"
max_content_length = 10000
http_bind = "127.0.0.1:3456"
log_level = "info"
```

Environment variable overrides:

| Variable | Description |
|---|---|
| `CLERK_DATA_DIR` | Root data directory |
| `CLERK_MAX_CONTENT_LENGTH` | Max content length per file |
| `CLERK_HTTP_BIND` | HTTP bind address |
| `CLERK_LOG_LEVEL` | Log level filter |
| `CLERK_CONFIG` | Path to config file |

## Tools

### Notes (4 tools)

| Tool | Description |
|---|---|
| `create_note` | Create a new note with title, content, tags, category |
| `read_note` | Read a note by ID |
| `update_note` | Update a note's title, content, tags, or category |
| `delete_note` | Delete a note by ID |

### Todos (5 tools)

| Tool | Description |
|---|---|
| `create_todo` | Create a todo with title, description, priority, due date |
| `read_todo` | Read a todo by ID |
| `update_todo` | Update a todo's fields |
| `delete_todo` | Delete a todo by ID |
| `set_todo_status` | Change status to pending, in_progress, or done |

### Documents (4 tools)

| Tool | Description |
|---|---|
| `create_document` | Create a document with title, content, summary |
| `read_document` | Read a document by ID |
| `update_document` | Update a document's fields |
| `delete_document` | Delete a document by ID |

### Search & Discovery (5 tools)

| Tool | Description |
|---|---|
| `search` | Full-text search with optional type/tag/category filters |
| `list_items` | Paginated listing with filters (type, tags, category, status) |
| `list_tags` | All tags with item counts |
| `list_categories` | All categories with item counts |
| `find_related` | Items sharing tags with a given item |

## MCP Resources

Browse structured data via `clerk://` URIs:

| URI | Description |
|---|---|
| `clerk://items` | Summary of all items |
| `clerk://notes` | All notes |
| `clerk://todos` | All todos with status |
| `clerk://documents` | All documents with summaries |
| `clerk://tags` | Tag cloud with counts |
| `clerk://tags/{tag}` | Items with a specific tag |
| `clerk://categories/{category}` | Items in a category |
| `clerk://items/{id}` | Full content of a specific item |

## MCP Client Configuration

### Claude Desktop (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "clerk": {
      "command": "clerk-mcp"
    }
  }
}
```

### HTTP mode (`.mcp.json`)

```json
{
  "mcpServers": {
    "clerk": {
      "type": "http",
      "url": "http://127.0.0.1:3456/mcp"
    }
  }
}
```

## Data Format

Items are stored as markdown files with YAML frontmatter in `~/.clerk/{notes,todos,documents}/`:

```yaml
---
id: "uuid-v4"
title: "Example Note"
type: "note"
tags: ["rust", "mcp"]
category: "engineering"
created: "2026-04-16T10:00:00Z"
updated: "2026-04-16T10:00:00Z"
---
Markdown content here...
```

## Development

```bash
cargo build --release       # Release build
cargo test                  # Run tests
cargo clippy                # Lint (must pass with zero warnings)
cargo fmt                   # Format code
cargo install --path .      # Install the project
```

## License

MIT
