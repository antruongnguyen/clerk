# Clerk MCP Server — Implementation Plan

## Context

Clerk is a personal information management MCP server written in Rust. The user wants to manage notes, todos, and technical documents as local markdown files in `~/.clerk/`, accessible to AI agents via STDIO and HTTP transports. The project has no implementation yet — only `docs/ideas.md` (vision) and `CLAUDE.md` (dev guidelines). Two reference MCP servers exist at `/Users/I756434/Projects/mcp/mcp-safeshell` (transport/tools/config patterns) and `/Users/I756434/Projects/mcp/mcp-bmad-method` (resources/index/workspace patterns).

---

## 1. Data Model

### 1.1 Directory Structure (`~/.clerk/`)

```
~/.clerk/
├── config.toml              # User configuration
├── notes/                   # General notes
│   ├── 2026-04-16-meeting-notes.md
│   └── rust-async-patterns.md
├── todos/                   # Todo items
│   ├── 2026-04-16-fix-login-bug.md
│   └── 2026-04-17-review-pr.md
└── documents/               # Technical documents
    ├── system-architecture.md
    └── api-design-guide.md
```

### 1.2 Markdown Frontmatter Schema

**Note:**
```yaml
---
id: "uuid-v4"
title: "Meeting Notes"
type: "note"
tags: ["meeting", "project-alpha"]
category: "work"
created: "2026-04-16T10:00:00Z"
updated: "2026-04-16T10:00:00Z"
---
Content here...
```

**Todo:**
```yaml
---
id: "uuid-v4"
title: "Fix login bug"
type: "todo"
tags: ["bug", "auth"]
category: "engineering"
status: "pending"           # pending | in_progress | done
priority: "high"            # low | medium | high
due: "2026-04-20"           # optional
created: "2026-04-16T10:00:00Z"
updated: "2026-04-16T10:00:00Z"
---
Description of what needs to be done...
```

**Document:**
```yaml
---
id: "uuid-v4"
title: "System Architecture"
type: "document"
tags: ["architecture", "backend"]
category: "engineering"
summary: "Overview of the system's microservice architecture and data flow."
created: "2026-04-16T10:00:00Z"
updated: "2026-04-16T10:00:00Z"
---
Full document content...
```

### 1.3 File Naming Convention

Format: `{slug}.md` where slug is derived from the title (lowercase, hyphens, max 60 chars). On collision, append `-2`, `-3`, etc.

### 1.4 Knowledge Graph

Tags and categories form the graph edges. The in-memory index maintains:
- `tags_index: HashMap<String, Vec<String>>` — tag → list of item IDs
- `categories_index: HashMap<String, Vec<String>>` — category → list of item IDs

This enables "find related items" by intersecting tag sets and traversing shared categories.

---

## 2. Module Architecture

```
clerk/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point: tracing, config, transport dispatch
│   ├── server.rs            # ServerHandler impl, tool_router, tool definitions
│   ├── config.rs            # Config loading (TOML + env overrides)
│   ├── storage/
│   │   ├── mod.rs           # Storage trait and re-exports
│   │   ├── markdown.rs      # Frontmatter parsing, file read/write
│   │   └── index.rs         # In-memory index: build, query, update
│   ├── models.rs            # Core data types: Note, Todo, Document, Item enum
│   └── search.rs            # Search and filtering logic
├── tests/
│   └── integration.rs       # End-to-end MCP tool tests
├── CLAUDE.md
├── README.md
└── docs/
    ├── ideas.md
    └── plan.md              # This file
```

### Module Dependency Flow

```
main.rs → config.rs → server.rs → storage/ → models.rs
                          ↓
                      search.rs
```

---

## 3. MCP Tools

### 3.1 Notes CRUD (4 tools)

| Tool | Description | Key Params |
|------|-------------|------------|
| `create_note` | Create a new note | `title`, `content`, `tags?`, `category?` |
| `read_note` | Read a note by ID or title | `id` or `title` |
| `update_note` | Update note content/metadata | `id`, `title?`, `content?`, `tags?`, `category?` |
| `delete_note` | Delete a note | `id` |

### 3.2 Todos CRUD (5 tools)

| Tool | Description | Key Params |
|------|-------------|------------|
| `create_todo` | Create a new todo | `title`, `description?`, `tags?`, `category?`, `priority?`, `due?` |
| `read_todo` | Read a todo by ID or title | `id` or `title` |
| `update_todo` | Update todo content/metadata | `id`, fields to update |
| `delete_todo` | Delete a todo | `id` |
| `set_todo_status` | Change todo status | `id`, `status` (pending/in_progress/done) |

### 3.3 Documents CRUD (4 tools)

| Tool | Description | Key Params |
|------|-------------|------------|
| `create_document` | Create a technical document | `title`, `content`, `summary?`, `tags?`, `category?` |
| `read_document` | Read a document by ID or title | `id` or `title` |
| `update_document` | Update document content/metadata | `id`, fields to update |
| `delete_document` | Delete a document | `id` |

### 3.4 Search & Discovery (4 tools)

| Tool | Description | Key Params |
|------|-------------|------------|
| `search` | Full-text search across all items | `query`, `type?` (note/todo/document), `tags?`, `category?` |
| `list_items` | List items with filters | `type?`, `tags?`, `category?`, `status?`, `limit?`, `offset?` |
| `list_tags` | List all tags with item counts | (none) |
| `list_categories` | List all categories with item counts | (none) |
| `find_related` | Find items related to a given item via shared tags | `id` |

### 3.5 Total: 18 tools

---

## 4. MCP Resources

Expose browsable resources via `clerk://` URIs:

| URI Pattern | Description |
|-------------|-------------|
| `clerk://items` | List of all items (summary) |
| `clerk://notes` | List of all notes |
| `clerk://todos` | List of all todos (with status) |
| `clerk://documents` | List of all documents (with summaries) |
| `clerk://tags` | Tag cloud with counts |
| `clerk://tags/{tag}` | All items with a specific tag |
| `clerk://categories/{category}` | All items in a category |
| `clerk://items/{id}` | Full content of a specific item |

Resource templates for parameterized URIs:
- `clerk://tags/{tag}`
- `clerk://categories/{category}`
- `clerk://items/{id}`

---

## 5. Configuration (`~/.clerk/config.toml`)

```toml
# Data directory (default: ~/.clerk/)
data_dir = "~/.clerk"

# Maximum content length per file in characters
max_content_length = 10000

# HTTP transport bind address
http_bind = "127.0.0.1:3456"

# Log level
log_level = "info"
```

Environment variable overrides: `CLERK_DATA_DIR`, `CLERK_MAX_CONTENT_LENGTH`, `CLERK_HTTP_BIND`, `CLERK_LOG_LEVEL`.

---

## 6. Implementation Phases

### Phase 1: Foundation (Cargo.toml, main.rs, config.rs, models.rs)
- Create `Cargo.toml` with all dependencies
- Implement `config.rs` — TOML loading from `~/.clerk/config.toml` with env overrides
- Implement `models.rs` — core types (Note, Todo, Document, ItemMeta, Item enum)
- Implement `main.rs` — tracing setup, config loading, transport dispatch (stdio + http)
- Implement skeleton `server.rs` — ServerHandler with `get_info()`, empty tool_router
- **Verify:** `cargo build` succeeds, server starts in stdio mode

### Phase 2: Storage Layer (storage/)
- Implement `storage/markdown.rs` — frontmatter parsing (YAML between `---` delimiters), file read/write, slug generation
- Implement `storage/index.rs` — in-memory index built by scanning `~/.clerk/`, HashMap lookups by ID/tag/category
- Implement `storage/mod.rs` — Storage struct combining markdown ops + index
- **Verify:** Unit tests for frontmatter parsing, index building, CRUD operations on temp directories

### Phase 3: Core Tools (server.rs — CRUD)
- Implement all 13 CRUD tools (notes: 4, todos: 5, documents: 4) in `server.rs` using `#[tool_router]` macros
- Wire Storage into Server via `Arc<RwLock<Storage>>`
- **Verify:** Each tool works via MCP Inspector or stdio client

### Phase 4: Search & Discovery (search.rs + remaining tools)
- Implement `search.rs` — full-text search (case-insensitive substring matching across title + content + tags)
- Implement the 5 search/discovery tools: `search`, `list_items`, `list_tags`, `list_categories`, `find_related`
- **Verify:** Search returns correct results across different item types

### Phase 5: Resources & Polish
- Implement `list_resources()`, `read_resource()`, `list_resource_templates()` in ServerHandler
- Implement resource subscriptions (notify on item changes)
- Add README.md content, .mcp.json example config
- **Verify:** Resources browsable from MCP client, full end-to-end test

---

## 7. Key Patterns to Follow (from reference projects)

### Transport setup (from mcp-safeshell `src/main.rs`)
```rust
// Stdio: server.serve(rmcp::transport::stdio()).await
// HTTP: StreamableHttpService::new(factory, LocalSessionManager, config)
//       axum::Router::new().nest_service("/mcp", service)
```

### Tool definition (from mcp-safeshell `src/server.rs`)
```rust
#[tool_router]
impl ClerkServer {
    #[tool(description = "Create a new note")]
    async fn create_note(
        &self,
        Parameters(req): Parameters<CreateNoteRequest>,
    ) -> Result<CallToolResult, McpError> { ... }
}

#[tool_handler]
impl ServerHandler for ClerkServer {
    fn get_info(&self) -> ServerInfo { ... }
}
```

### Request types (from mcp-safeshell `src/server.rs`)
```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateNoteRequest {
    #[schemars(description = "Title of the note")]
    pub title: String,
    #[schemars(description = "Markdown content of the note")]
    pub content: String,
    #[schemars(description = "Tags for categorization")]
    #[serde(default)]
    pub tags: Vec<String>,
    #[schemars(description = "Category name")]
    pub category: Option<String>,
}
```

### Resources (from mcp-bmad-method `src/main.rs`)
```rust
async fn list_resources(&self, ...) -> Result<ListResourcesResult, ErrorData> {
    // Build RawResource list from index
}
async fn read_resource(&self, request: ReadResourceRequestParams, ...) -> Result<ReadResourceResult, ErrorData> {
    // Match on request.uri pattern, return ResourceContents::text()
}
```

---

## 8. Verification Plan

1. **Unit tests**: Frontmatter parsing, index building, search logic, slug generation
2. **Integration tests**: Each MCP tool via in-process server (see mcp-bmad-method `tests.rs` pattern)
3. **Manual testing**: Start server in stdio mode, connect via MCP Inspector, exercise all tools
4. **HTTP transport test**: Start in http mode, connect via `.mcp.json` config
5. **Edge cases**: Empty data dir, duplicate titles, max content length enforcement, special characters in tags
