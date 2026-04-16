# How Clerk Works

Clerk is a file-based personal knowledge management system exposed as an MCP server. This document explains how it manages, organizes, and searches data internally.

## Data Model

Every piece of information in Clerk is an **item**. There are three item types:

| Type | Purpose | Unique fields |
|---|---|---|
| **Note** | General-purpose information and reference material | `content` |
| **Todo** | Actionable tasks with lifecycle tracking | `description`, `status`, `priority`, `due` |
| **Document** | Longer-form technical content | `content`, `summary` |

All items share common metadata:

- `id` — UUID v4, assigned at creation, immutable
- `title` — human-readable name
- `tags` — list of strings for cross-cutting classification
- `category` — optional single-value grouping
- `created` / `updated` — UTC timestamps

## Storage Layer

### File Format

Items are stored as individual markdown files with YAML frontmatter:

```yaml
---
id: "550e8400-e29b-41d4-a716-446655440000"
title: "Rust Async Patterns"
type: "note"
tags: ["rust", "async"]
category: "engineering"
created: "2026-04-16T10:00:00Z"
updated: "2026-04-16T10:00:00Z"
---

Content body in markdown...
```

### Directory Structure

```
~/.clerk/
├── notes/           # Note markdown files
├── todos/           # Todo markdown files
├── documents/       # Document markdown files
├── tmp/             # Temporary downloads (never indexed, auto-cleaned)
└── config.toml      # Optional configuration
```

Files are named using URL-safe slugs derived from the title (max 60 characters, truncated on hyphen boundaries). Collisions are resolved by appending `-2`, `-3`, etc.

### Content Limits

File content is capped at `max_content_length` (default: 10,000 characters). Larger content should be split across multiple items linked by shared tags.

## In-Memory Index

At startup, Clerk scans all `.md` files in the `notes/`, `todos/`, and `documents/` directories and builds an in-memory index. The `tmp/` directory is never scanned.

### Index Structure

The index maintains three data structures:

1. **Primary index** — `HashMap<String, IndexEntry>` mapping item ID to a lightweight entry (id, title, type, tags, category, file path, timestamps). This provides O(1) lookup by ID.

2. **Tag index** — `HashMap<String, HashSet<String>>` mapping each tag to the set of item IDs that carry it. This provides O(1) lookup of all items with a given tag.

3. **Category index** — `HashMap<String, HashSet<String>>` mapping each category to its item IDs. Same O(1) lookup.

### Index Maintenance

The index stays in sync with disk through CRUD operations:

- **Create** — writes file to disk, inserts entry into all three index maps
- **Update** — rewrites file, removes old entry from indexes, inserts updated entry
- **Delete** — removes file, cleans up all index references (empty tags/categories are pruned)
- **Read** — looks up file path from the primary index, reads and parses from disk

The index is never persisted — it's rebuilt from disk on every server startup. This keeps the source of truth in the markdown files and avoids stale index problems.

## Search

### How Search Works

The `search` tool performs case-insensitive substring matching with a **tiered scoring system**:

| Match location | Score | Disk I/O required? |
|---|---|---|
| Title | 100 | No (in-memory index) |
| Tag | 50 | No (in-memory index) |
| Category | 25 | No (in-memory index) |
| File content | 10 | Yes (reads file from disk) |

**Key optimization:** Content matching (the most expensive operation) is only performed when no metadata match was found. If an item already matched on title, tag, or category, its file content is never read from disk. This means most searches complete entirely from memory.

Results are sorted by score descending, with title as a tiebreaker.

### Filters

Both `search` and `list_items` support combinable filters:

- **Type filter** — restrict to note, todo, or document
- **Tag filter** — all specified tags must be present (AND logic)
- **Category filter** — exact match (case-insensitive)
- **Status filter** (`list_items` only) — filter todos by pending, in_progress, or done (requires disk read)

### Performance Characteristics

| Operation | Complexity | Notes |
|---|---|---|
| Lookup by ID | O(1) | HashMap primary index |
| Find by tag | O(k) | k = items with that tag; HashSet lookup + entry resolution |
| Find by category | O(k) | k = items in that category; same structure |
| Find by type | O(n) | Linear scan of all entries |
| Full-text search (metadata hit) | O(n) | Scans all index entries, no disk I/O |
| Full-text search (content fallback) | O(n * d) | d = disk read per non-matching item |
| Find related | O(t * k) | t = tags on the source item, k = avg items per tag |
| List items (paginated) | O(n) | Filter + sort, then slice |

**Practical performance:** The index lives entirely in memory, so metadata operations are fast regardless of item count. Disk I/O only happens for:

- Reading full item content (read_note, read_document, etc.)
- Content-fallback in search (items with no metadata match)
- Status filtering for todos (status is stored in file, not in the index)

For a typical personal knowledge base (hundreds to low thousands of items), all operations complete in milliseconds.

### What's NOT Indexed

- File content (body text) — only read from disk on demand
- Todo status, priority, due date — stored in files, not in the index
- Document summary — stored in files, not in the index

This is a deliberate trade-off: the index stays small and fast by only caching the fields needed for filtering, tagging, and discovery. Fields that are only needed when reading a specific item are loaded from disk on access.

## Scaling: What Happens After Years of Use

Clerk's design is optimized for a personal knowledge base. But what happens when data grows significantly — say 10 items/day for 5 years (~18,000 items)?

### Memory

Each `IndexEntry` is ~200-300 bytes. At 18,000 items the full index is ~5 MB — negligible for any modern machine. Memory is not a concern.

### Startup Time

The index is rebuilt from disk on every server start by reading and parsing all markdown files. At 18,000 files this could take several seconds on a cold filesystem. On warm filesystem cache (typical for a machine in active use), it will be faster but still noticeable.

### Search Performance at Scale

| Scenario | 100 items | 18,000 items | Bottleneck |
|---|---|---|---|
| Metadata-only search (title/tag/category hit) | <1ms | <10ms | None — in-memory scan |
| Content-fallback search (no metadata hit) | <10ms | **1-5 seconds** | Disk I/O — reads every file |
| Status filter on todos | <5ms | ~100ms+ | Disk I/O — reads todo files |
| Tag/category lookup | <1ms | <1ms | None — HashMap O(1) |

The **content-fallback path** is the scaling bottleneck. When you search for a term that only appears inside file bodies (not in titles, tags, or categories), Clerk reads every file from disk sequentially. At 18,000 files, this becomes slow.

### Possible Future Mitigations

These are not implemented today but are realistic improvements:

- **Content snippet in index** — cache the first ~200 characters of each item in the index, reducing disk reads for common searches
- **Inverted text index** — build a proper full-text index at startup (like a mini search engine), making all searches O(1) per matching term
- **Parallel disk reads** — use `tokio::spawn` to read files concurrently instead of sequentially
- **Early termination** — stop scanning after finding enough high-scoring results instead of scanning everything then sorting
- **Persistent index** — serialize the index to disk so startup doesn't require re-parsing all files

### Practical Advice

For best performance at scale, lean on metadata: give items descriptive titles, consistent tags, and categories. Searches that match on metadata never touch disk and stay fast regardless of data size.

## Knowledge Graph

Items are connected through **shared tags**, forming a navigable knowledge graph:

```
[Rust Async Patterns] --rust--> [Tokio Runtime Guide]
         |                              |
       async                          tokio
         |                              |
         v                              v
[Async Error Handling]         [Tokio Best Practices]
```

The `find_related` tool traverses this graph: given an item, it finds all other items that share at least one tag, ranked by the number of overlapping tags. This enables discovery without requiring explicit links between items.

**Categories** provide a separate, coarser grouping axis. While tags create a many-to-many web, categories are one-to-one (each item has at most one category).

### How Relationships Are Maintained

When a new item is created, Clerk updates the tag and category indexes immediately:

1. The item's ID is added to the `HashSet` for each of its tags in the tag index
2. The item's ID is added to the category index (if a category is set)
3. From that point, `find_related` will include this item when traversing any of its tags

**There is no automatic relationship detection.** If you create a note about "Rust async" and a document about "Tokio runtime" without giving them a shared tag, Clerk will never connect them. The knowledge graph exists entirely through explicitly shared tags.

This means relationship quality depends on tagging discipline:

- **Good tagging** — items with overlapping topics share tags, `find_related` surfaces rich connections
- **Poor tagging** — items are isolated islands, `find_related` returns nothing useful

### Who Maintains Relationships in Practice

In an AI-agent workflow, the **agent itself** is the primary relationship maintenance layer. The system prompt instructs agents to:

- Check `list_tags` before creating items to reuse existing tags
- Apply meaningful, consistent tags so items are discoverable later
- Search for existing items on the same topic before creating new ones

This means the AI agent calling Clerk's tools acts as the "glue" that keeps the knowledge graph connected. The quality of the graph is directly proportional to how well the agent follows these practices.

### Limitations of Tag-Only Linking

- **No content-based linking** — two items about the same topic with different tags are invisible to each other
- **No bidirectional explicit links** — you cannot say "item A references item B" directly
- **No automatic tag suggestions** — Clerk won't suggest tags based on content similarity
- **Tag drift** — over time, semantically equivalent tags may diverge (e.g. "js", "javascript", "JS") without any merge mechanism

## URL Import

The `create_document_from_url` tool enables creating documents from remote text content:

1. Downloads the URL to `~/.clerk/tmp/download-{uuid}.tmp`
2. Reads the file content as UTF-8 text
3. Deletes the temp file (unconditionally, even on error)
4. Creates a document from the downloaded content (auto-splitting if needed)
5. Stores the source URL in each document's metadata for provenance tracking

The temp file provides a safety boundary: if the download is interrupted or the content is invalid, no partial data pollutes the document store. The `tmp/` directory is never indexed, so temp files are invisible to search and listing.

## Auto-Split for Large Content

When `create_document` or `create_document_from_url` receives content exceeding the `max_content_length` limit (default 10,000 chars), the content is automatically split into multiple documents:

### Splitting Algorithm

1. Split on **paragraph boundaries** (`\n\n`) — preferred, preserves document structure
2. Fall back to **line boundaries** (`\n`) — for oversized paragraphs
3. Fall back to **character boundaries** — last resort for lines longer than the limit

Each chunk is guaranteed to be within the size limit. No content is lost during splitting.

### How Parts Are Organized

For a document titled "API Reference" split into 3 parts:

| Part | Title | Summary |
|---|---|---|
| 1 | API Reference (Part 1 of 3) | Original summary provided by the caller |
| 2 | API Reference (Part 2 of 3) | "Part 2 of 3. Continuation of API Reference" |
| 3 | API Reference (Part 3 of 3) | "Part 3 of 3. Continuation of API Reference" |

All parts share the same tags, category, and source URL. Each part is independently searchable and manageable.

### Single Documents Are Unchanged

When content fits within the limit, `create_document` behaves exactly as before — a single document is created. The auto-split path is transparent.

## Source URL Tracking

Documents can optionally store a `source_url` in their metadata, tracking where the content originated. This is set automatically by `create_document_from_url` and optionally by `create_document`.

### How It Works

- `source_url` is stored in YAML frontmatter and indexed in-memory via a `source_url_index` (HashMap mapping URL to item IDs)
- `find_by_source_url` queries the index to find all documents from a given URL
- This enables bulk operations: re-import (delete old, download new), update, or remove all content from a source

### Use Case: Knowledge Base from llms.txt

1. Import: `create_document_from_url("https://example.com/llms.txt", ...)` creates 5 parts
2. Later: `find_by_source_url("https://example.com/llms.txt")` returns all 5 parts
3. Re-import: delete all 5 parts, then re-import from the URL to get updated content

## MCP Resources

Clerk exposes browsable resources via `clerk://` URIs, providing read-only views into the data:

| URI pattern | What it returns |
|---|---|
| `clerk://items` | Summary list of all items |
| `clerk://notes` | All notes |
| `clerk://todos` | All todos with status |
| `clerk://documents` | All documents with summaries |
| `clerk://tags` | Tag cloud with counts |
| `clerk://tags/{tag}` | Items matching a specific tag |
| `clerk://categories/{category}` | Items in a specific category |
| `clerk://items/{id}` | Full content of a single item |

Resources are rendered as markdown text. They complement the tools by providing a browsing interface — agents can scan resources for context before deciding which tools to invoke.

## Pros and Limitations

### Pros

- **Simple, portable storage.** Everything is plain markdown files. No database, no migrations, no lock-in. You can read, edit, or move your data with any text editor or file manager.
- **Human-readable format.** YAML frontmatter + markdown body means your data is useful even without Clerk running. `grep`, `find`, and any markdown tool work directly on the files.
- **Fast metadata operations.** In-memory HashMap indexes make tag lookups, category filtering, and ID resolution O(1). For typical usage (up to a few thousand items), everything feels instant.
- **Knowledge graph without complexity.** Tag-based linking gives you a navigable web of related items without requiring a graph database or explicit link management.
- **Auto-split for large content.** Documents exceeding the size limit are automatically split into multiple linked parts. No data loss, no manual chunking.
- **Source URL provenance.** Documents track their origin URL, enabling find/update/remove of all content from a specific source.
- **AI-agent friendly.** Structured tool schemas, `clerk://` browsable resources, and clear system prompt instructions make it easy for LLMs to use Clerk effectively. Tool descriptions instruct agents to provide rich metadata.
- **Dual transport.** STDIO for local integrations (Claude Desktop, editors) and HTTP for remote/web clients from the same binary.
- **Zero configuration needed.** Works out of the box with sensible defaults. All config is optional.

### Limitations

- **No date-range search.** Timestamps (`created`, `updated`) exist in the index but there is no filter to query by date range. You cannot ask "show me notes from last week" without adding this feature.
- **Content search doesn't scale.** The content-fallback path reads files from disk sequentially. At thousands of items, searches that only match on file body content become slow (seconds, not milliseconds).
- **No full-text index.** There is no inverted index or token-based search. All matching is substring-based. This means no fuzzy matching, no stemming, no relevance ranking beyond the four-tier score system.
- **Relationships are manual.** The knowledge graph depends entirely on consistent tagging. There is no automatic relationship detection, no content similarity analysis, no suggested tags. If two items about the same topic have different tags, they are invisible to each other.
- **Tag hygiene is fragile.** No tag merging, no aliases, no normalization. "js", "javascript", and "JS" are three separate tags. Over time this can fragment the knowledge graph.
- **Index is not persisted.** The index is rebuilt from disk on every startup. With thousands of files this adds noticeable startup latency.
- **Single-user, single-machine.** No sync, no collaboration, no multi-device access (unless you layer file sync like Dropbox or git on top).
- **Content size cap.** The 10,000-character default limit per part means very large documents are split into multiple files. Each part is independently searchable but the split is opaque to the reader.
- **No binary content.** Only UTF-8 text. PDFs, images, and other binary formats cannot be stored or indexed.
- **Todo status requires disk read.** Status, priority, and due date are stored in files, not in the index. Filtering todos by status reads every todo file from disk.
