use std::sync::Arc;

use chrono::NaiveDate;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars::{self, JsonSchema},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::models::*;
use crate::search;
use crate::storage::Storage;

// ── Request schemas ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateNoteRequest {
    #[schemars(description = "Title of the note")]
    pub title: String,
    #[schemars(description = "Markdown content of the note")]
    pub content: String,
    #[serde(default)]
    #[schemars(description = "Tags for categorization")]
    pub tags: Vec<String>,
    #[schemars(description = "Optional category name")]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadNoteRequest {
    #[schemars(description = "ID of the note to read")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateNoteRequest {
    #[schemars(description = "ID of the note to update")]
    pub id: String,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New markdown content")]
    pub content: Option<String>,
    #[schemars(description = "New tags (replaces existing tags)")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "New category")]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteNoteRequest {
    #[schemars(description = "ID of the note to delete")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTodoRequest {
    #[schemars(description = "Title of the todo")]
    pub title: String,
    #[serde(default)]
    #[schemars(description = "Description of what needs to be done")]
    pub description: String,
    #[serde(default)]
    #[schemars(description = "Tags for categorization")]
    pub tags: Vec<String>,
    #[schemars(description = "Optional category name")]
    pub category: Option<String>,
    #[schemars(description = "Priority: low, medium, or high (default: medium)")]
    pub priority: Option<String>,
    #[schemars(description = "Due date in YYYY-MM-DD format")]
    pub due: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadTodoRequest {
    #[schemars(description = "ID of the todo to read")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTodoRequest {
    #[schemars(description = "ID of the todo to update")]
    pub id: String,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description")]
    pub description: Option<String>,
    #[schemars(description = "New tags (replaces existing tags)")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "New category")]
    pub category: Option<String>,
    #[schemars(description = "New priority: low, medium, or high")]
    pub priority: Option<String>,
    #[schemars(description = "New due date in YYYY-MM-DD format")]
    pub due: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteTodoRequest {
    #[schemars(description = "ID of the todo to delete")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetTodoStatusRequest {
    #[schemars(description = "ID of the todo")]
    pub id: String,
    #[schemars(description = "New status: pending, in_progress, or done")]
    pub status: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateDocumentRequest {
    #[schemars(description = "Title of the document")]
    pub title: String,
    #[schemars(description = "Markdown content of the document")]
    pub content: String,
    #[schemars(description = "Optional short summary/abstract")]
    pub summary: Option<String>,
    #[serde(default)]
    #[schemars(description = "Tags for categorization")]
    pub tags: Vec<String>,
    #[schemars(description = "Optional category name")]
    pub category: Option<String>,
    #[schemars(description = "Optional source URL this content was retrieved from")]
    pub source_url: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadDocumentRequest {
    #[schemars(description = "ID of the document to read")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateDocumentRequest {
    #[schemars(description = "ID of the document to update")]
    pub id: String,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New markdown content")]
    pub content: Option<String>,
    #[schemars(description = "New summary")]
    pub summary: Option<String>,
    #[schemars(description = "New tags (replaces existing tags)")]
    pub tags: Option<Vec<String>>,
    #[schemars(description = "New category")]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteDocumentRequest {
    #[schemars(description = "ID of the document to delete")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateDocumentFromUrlRequest {
    #[schemars(
        description = "URL to download text content from (e.g., llms.txt, llms-full.txt, or any plain text URL)"
    )]
    pub url: String,
    #[schemars(description = "Title of the document")]
    pub title: String,
    #[schemars(description = "Optional short summary/abstract")]
    pub summary: Option<String>,
    #[serde(default)]
    #[schemars(description = "Tags for categorization")]
    pub tags: Vec<String>,
    #[schemars(description = "Optional category name")]
    pub category: Option<String>,
}

// ── Search/Discovery request schemas ─────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Search query string")]
    pub query: String,
    #[schemars(description = "Filter by item type: note, todo, or document")]
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Filter by tags (items must have all specified tags)")]
    pub tags: Vec<String>,
    #[schemars(description = "Filter by category name")]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListItemsRequest {
    #[schemars(description = "Filter by item type: note, todo, or document")]
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Filter by tags (items must have all specified tags)")]
    pub tags: Vec<String>,
    #[schemars(description = "Filter by category name")]
    pub category: Option<String>,
    #[schemars(description = "Filter by todo status: pending, in_progress, or done")]
    pub status: Option<String>,
    #[schemars(description = "Maximum number of items to return (default 20)")]
    pub limit: Option<usize>,
    #[schemars(description = "Number of items to skip for pagination (default 0)")]
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindRelatedRequest {
    #[schemars(description = "ID of the item to find related items for")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindBySourceUrlRequest {
    #[schemars(description = "Source URL to find all documents created from")]
    pub source_url: String,
}

// ── Server ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ClerkServer {
    config: Config,
    storage: Arc<RwLock<Storage>>,
    #[allow(dead_code)] // Read by #[tool_handler] macro expansion
    tool_router: ToolRouter<ClerkServer>,
}

impl ClerkServer {
    pub fn new(config: Config) -> Self {
        let storage = Storage::new(&config)
            .expect("failed to initialize storage");
        Self {
            config: config.clone(),
            storage: Arc::new(RwLock::new(storage)),
            tool_router: Self::tool_router(),
        }
    }
}

// ── Tool implementations ──────────────────────────────────────────────────

#[tool_router]
impl ClerkServer {
    // ── Notes ─────────────────────────────────────────────────────────

    #[tool(description = "Create a new note with a title and markdown content. \
        Provide up to 20 relevant tags for knowledge graph linking. \
        Provide at least a category (e.g. work, personal, engineering, research). \
        Check list_tags first to reuse existing tags.")]
    async fn create_note(
        &self,
        Parameters(req): Parameters<CreateNoteRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut meta = ItemMeta::new(req.title, ItemType::Note);
        meta.tags = req.tags;
        meta.category = req.category;

        let item = Item::Note(Note {
            meta,
            content: req.content,
        });

        let mut storage = self.storage.write().await;
        match storage.create_item(item) {
            Ok(created) => ok_json(&created),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Read a note by its ID, returning full content and metadata")]
    async fn read_note(
        &self,
        Parameters(req): Parameters<ReadNoteRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        match storage.read_item(&req.id) {
            Ok(Item::Note(n)) => ok_json(&n),
            Ok(_) => Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a note", req.id),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Update an existing note's title, content, tags, or category. \
        When updating content, review and update tags to match the new content. \
        Provide up to 20 tags. \
        Maintain existing relevant tags and add new ones to preserve knowledge graph relationships.")]
    async fn update_note(
        &self,
        Parameters(req): Parameters<UpdateNoteRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        let existing = match storage.read_item(&req.id) {
            Ok(Item::Note(n)) => n,
            Ok(_) => return Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a note", req.id),
            )])),
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };

        let mut note = existing;
        if let Some(title) = req.title {
            note.meta.title = title;
        }
        if let Some(content) = req.content {
            note.content = content;
        }
        if let Some(tags) = req.tags {
            note.meta.tags = tags;
        }
        if let Some(category) = req.category {
            note.meta.category = Some(category);
        }

        match storage.update_item(Item::Note(note)) {
            Ok(updated) => ok_json(&updated),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Delete a note by its ID")]
    async fn delete_note(
        &self,
        Parameters(req): Parameters<DeleteNoteRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        match storage.delete_item(&req.id) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                format!("Deleted note {}", req.id),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    // ── Todos ─────────────────────────────────────────────────────────

    #[tool(description = "Create a new todo with a title, optional description, priority, and due date")]
    async fn create_todo(
        &self,
        Parameters(req): Parameters<CreateTodoRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut meta = ItemMeta::new(req.title, ItemType::Todo);
        meta.tags = req.tags;
        meta.category = req.category;

        let priority = parse_priority(req.priority.as_deref());
        let due = parse_due_date(req.due.as_deref())?;

        let item = Item::Todo(Todo {
            meta,
            description: req.description,
            status: TodoStatus::Pending,
            priority,
            due,
        });

        let mut storage = self.storage.write().await;
        match storage.create_item(item) {
            Ok(created) => ok_json(&created),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Read a todo by its ID, returning status, priority, due date, and description")]
    async fn read_todo(
        &self,
        Parameters(req): Parameters<ReadTodoRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        match storage.read_item(&req.id) {
            Ok(Item::Todo(t)) => ok_json(&t),
            Ok(_) => Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a todo", req.id),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Update an existing todo's title, description, tags, category, priority, or due date")]
    async fn update_todo(
        &self,
        Parameters(req): Parameters<UpdateTodoRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        let existing = match storage.read_item(&req.id) {
            Ok(Item::Todo(t)) => t,
            Ok(_) => return Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a todo", req.id),
            )])),
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };

        let mut todo = existing;
        if let Some(title) = req.title {
            todo.meta.title = title;
        }
        if let Some(description) = req.description {
            todo.description = description;
        }
        if let Some(tags) = req.tags {
            todo.meta.tags = tags;
        }
        if let Some(category) = req.category {
            todo.meta.category = Some(category);
        }
        if let Some(priority) = req.priority {
            todo.priority = parse_priority(Some(priority.as_str()));
        }
        if let Some(due_str) = req.due {
            todo.due = parse_due_date(Some(due_str.as_str()))?;
        }

        match storage.update_item(Item::Todo(todo)) {
            Ok(updated) => ok_json(&updated),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Delete a todo by its ID")]
    async fn delete_todo(
        &self,
        Parameters(req): Parameters<DeleteTodoRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        match storage.delete_item(&req.id) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                format!("Deleted todo {}", req.id),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Change a todo's status to pending, in_progress, or done")]
    async fn set_todo_status(
        &self,
        Parameters(req): Parameters<SetTodoStatusRequest>,
    ) -> Result<CallToolResult, McpError> {
        let status = match req.status.as_str() {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "done" => TodoStatus::Done,
            other => return Err(McpError::invalid_params(
                format!("invalid status: {other}. Use pending, in_progress, or done"),
                None,
            )),
        };

        let mut storage = self.storage.write().await;
        let existing = match storage.read_item(&req.id) {
            Ok(Item::Todo(t)) => t,
            Ok(_) => return Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a todo", req.id),
            )])),
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };

        let mut todo = existing;
        todo.status = status;

        match storage.update_item(Item::Todo(todo)) {
            Ok(updated) => ok_json(&updated),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    // ── Documents ─────────────────────────────────────────────────────

    #[tool(description = "Create a new technical document with a title, content, and optional summary. \
        Content exceeding the size limit is automatically split into multiple linked parts. \
        Provide a summary of up to 5 sentences describing the document's purpose and key points. \
        Provide up to 50 relevant tags for knowledge graph linking. \
        Provide at least a category. Check list_tags first to reuse existing tags.")]
    async fn create_document(
        &self,
        Parameters(req): Parameters<CreateDocumentRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        match storage.create_document_split(
            req.title,
            req.content,
            req.summary,
            req.tags,
            req.category,
            req.source_url,
        ) {
            Ok(items) if items.len() == 1 => ok_json(&items[0]),
            Ok(items) => ok_split_json(&items, 0),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Create a new document by downloading text content from a URL. \
        Content exceeding the size limit is automatically split into multiple linked parts. \
        The source URL is stored for provenance tracking. \
        If documents from the same URL already exist, they are removed before creating new ones. \
        Suitable for plain text URLs like llms.txt or llms-full.txt. \
        Provide a summary of up to 5 sentences. \
        Provide up to 50 relevant tags. Provide at least a category.")]
    async fn create_document_from_url(
        &self,
        Parameters(req): Parameters<CreateDocumentFromUrlRequest>,
    ) -> Result<CallToolResult, McpError> {
        let tmp_dir = {
            let storage = self.storage.read().await;
            storage.tmp_dir()
        };

        let timeout_secs = self.config.download_timeout_secs;
        let content = download_url_content(&req.url, &tmp_dir, timeout_secs)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("failed to download URL: {e}"), None)
            })?;

        let mut storage = self.storage.write().await;

        // Remove existing documents from this URL before creating new ones.
        let deleted = storage.delete_items_by_source_url(&req.url)
            .map_err(|e| McpError::internal_error(
                format!("failed to remove existing documents for URL: {e}"), None,
            ))?;

        match storage.create_document_split(
            req.title,
            content,
            req.summary,
            req.tags,
            req.category,
            Some(req.url),
        ) {
            Ok(items) if items.len() == 1 => {
                if deleted > 0 {
                    ok_json_with_note(&items[0], &format!("Replaced {deleted} existing document(s) from this URL"))
                } else {
                    ok_json(&items[0])
                }
            }
            Ok(items) => ok_split_json(&items, deleted),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Read a document by its ID, returning full content, summary, and metadata")]
    async fn read_document(
        &self,
        Parameters(req): Parameters<ReadDocumentRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        match storage.read_item(&req.id) {
            Ok(Item::Document(d)) => ok_json(&d),
            Ok(_) => Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a document", req.id),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Update an existing document's title, content, summary, tags, or category. \
        When updating content, also update the summary (up to 5 sentences) to reflect the changes. \
        Provide up to 50 tags. \
        Maintain existing relevant tags and add new ones to preserve knowledge graph relationships.")]
    async fn update_document(
        &self,
        Parameters(req): Parameters<UpdateDocumentRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        let existing = match storage.read_item(&req.id) {
            Ok(Item::Document(d)) => d,
            Ok(_) => return Ok(CallToolResult::error(vec![Content::text(
                format!("Item {} is not a document", req.id),
            )])),
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };

        let mut doc = existing;
        if let Some(title) = req.title {
            doc.meta.title = title;
        }
        if let Some(content) = req.content {
            doc.content = content;
        }
        if let Some(summary) = req.summary {
            doc.summary = Some(summary);
        }
        if let Some(tags) = req.tags {
            doc.meta.tags = tags;
        }
        if let Some(category) = req.category {
            doc.meta.category = Some(category);
        }

        match storage.update_item(Item::Document(doc)) {
            Ok(updated) => ok_json(&updated),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Delete a document by its ID")]
    async fn delete_document(
        &self,
        Parameters(req): Parameters<DeleteDocumentRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut storage = self.storage.write().await;
        match storage.delete_item(&req.id) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                format!("Deleted document {}", req.id),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    // ── Search & Discovery ───────────────────────────────────────────

    #[tool(description = "Full-text search across all items (notes, todos, documents). \
        Searches title, tags, category, and content. Results sorted by relevance.")]
    async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        let type_filter = req.item_type.as_deref().and_then(parse_item_type);
        let tag_filter = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags.as_slice())
        };

        let results = search::search_items(
            storage.index(),
            &req.query,
            type_filter.as_ref(),
            tag_filter,
            req.category.as_deref(),
        );

        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|e| index_entry_to_json(e))
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "query": req.query,
            "count": entries.len(),
            "results": entries,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List items with optional filters and pagination. \
        Filter by type, tags, category, or todo status. Default limit is 20.")]
    async fn list_items(
        &self,
        Parameters(req): Parameters<ListItemsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        let type_filter = req.item_type.as_deref().and_then(parse_item_type);
        let tag_filter = if req.tags.is_empty() {
            None
        } else {
            Some(req.tags.as_slice())
        };
        let limit = req.limit.unwrap_or(20);
        let offset = req.offset.unwrap_or(0);

        let (results, total) = search::list_items(
            storage.index(),
            type_filter.as_ref(),
            tag_filter,
            req.category.as_deref(),
            req.status.as_deref(),
            limit,
            offset,
        );

        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|e| index_entry_to_json(e))
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "total": total,
            "offset": offset,
            "limit": limit,
            "count": entries.len(),
            "items": entries,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List all tags with their item counts, sorted by count descending")]
    async fn list_tags(&self) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        let mut tags = storage.index().all_tags();
        // Sort by count descending.
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let entries: Vec<serde_json::Value> = tags
            .iter()
            .map(|(tag, count)| {
                serde_json::json!({ "tag": tag, "count": count })
            })
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "total": entries.len(),
            "tags": entries,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List all categories with their item counts")]
    async fn list_categories(&self) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        let categories = storage.index().all_categories();

        let entries: Vec<serde_json::Value> = categories
            .iter()
            .map(|(cat, count)| {
                serde_json::json!({ "category": cat, "count": count })
            })
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "total": entries.len(),
            "categories": entries,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Find items related to a given item by shared tags, sorted by overlap")]
    async fn find_related(
        &self,
        Parameters(req): Parameters<FindRelatedRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        let related = storage.index().find_related(&req.id);

        let entries: Vec<serde_json::Value> = related
            .iter()
            .map(|e| index_entry_to_json(e))
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "id": req.id,
            "count": entries.len(),
            "related": entries,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Find all documents created from a given source URL. \
        Use this to discover all parts of a document imported from a URL, \
        enabling bulk update or removal of content from a specific source.")]
    async fn find_by_source_url(
        &self,
        Parameters(req): Parameters<FindBySourceUrlRequest>,
    ) -> Result<CallToolResult, McpError> {
        let storage = self.storage.read().await;
        let items = storage.index().find_by_source_url(&req.source_url);

        let entries: Vec<serde_json::Value> = items
            .iter()
            .map(|e| index_entry_to_json(e))
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "source_url": req.source_url,
            "count": entries.len(),
            "documents": entries,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for ClerkServer {
    fn get_info(&self) -> ServerInfo {
        let mut impl_info = Implementation::default();
        impl_info.name = "clerk-mcp".to_string();
        impl_info.title = Some("Clerk MCP Server".to_string());
        impl_info.version = env!("CARGO_PKG_VERSION").to_string();
        impl_info.description =
            Some("Personal knowledge management — notes, todos, and technical documents stored as \
                  local markdown files with tag-based linking and full-text search".to_string());

        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_logging()
            .build();
        info.server_info = impl_info;
        info.instructions = Some(
            "Clerk is a personal knowledge management MCP server. All data lives as markdown files \
             in ~/.clerk/ organized into notes, todos, and documents.\n\n\
             ITEM TYPES:\n\
             - Notes: general-purpose information and reference material (title, content, tags, category).\n\
             - Todos: actionable tasks with status (pending/in_progress/done), priority (low/medium/high), \
               and optional due date (YYYY-MM-DD).\n\
             - Documents: longer-form technical content with an optional summary field. \
               Documents exceeding the size limit are automatically split into multiple linked parts.\n\n\
             TOOLS:\n\
             - CRUD: create/read/update/delete for each type (create_note, read_note, etc.).\n\
             - create_document / create_document_from_url: create documents with auto-split \
               for large content. Content from URLs is tracked via source_url for provenance.\n\
             - find_by_source_url: find all documents created from a given URL (for bulk update/removal).\n\
             - set_todo_status: transition a todo between pending, in_progress, and done.\n\
             - search: full-text search across all items; filter by type, tags, and category.\n\
             - list_items: paginated listing with filters (type, tags, category, status); \
               default limit 20.\n\
             - list_tags / list_categories: discover existing tags and categories with counts.\n\
             - find_related: given an item ID, find items sharing its tags (knowledge graph traversal).\n\n\
             RESOURCES (clerk:// URIs):\n\
             - clerk://items, clerk://notes, clerk://todos, clerk://documents — summary listings.\n\
             - clerk://tags — tag cloud with counts.\n\
             - clerk://tags/{tag} — items with a specific tag.\n\
             - clerk://categories/{category} — items in a specific category.\n\
             - clerk://items/{id} — full content of a specific item.\n\n\
             BEST PRACTICES:\n\
             - Search before creating to avoid duplicates.\n\
             - Reuse existing tags (check list_tags) rather than inventing new ones.\n\
             - Provide up to 20 tags for notes, up to 50 tags for documents. Provide at least a category.\n\
             - Provide summaries of up to 5 sentences for documents.\n\
             - When updating content, review and update tags/summary to maintain relationships.\n\
             - Content exceeding ~10,000 chars is auto-split into parts; no manual splitting needed.\n\
             - When cross-referencing other Clerk items in content, use clerk://items/{id} as the link URL. \
               Never use placeholder text like (link). Search for the target item first to get its ID."
                .to_string(),
        );
        info
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut resources = vec![
            RawResource::new("clerk://items", "All Items")
                .with_description("Summary list of all items")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResource::new("clerk://notes", "All Notes")
                .with_description("List of all notes")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResource::new("clerk://todos", "All Todos")
                .with_description("List of all todos with status")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResource::new("clerk://documents", "All Documents")
                .with_description("List of all documents with summaries")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResource::new("clerk://tags", "Tag Cloud")
                .with_description("All tags with item counts")
                .with_mime_type("text/markdown")
                .no_annotation(),
        ];

        // Add per-tag resources.
        let storage = self.storage.read().await;
        for (tag, count) in storage.index().all_tags() {
            let uri = format!("clerk://tags/{tag}");
            resources.push(
                RawResource::new(&uri, format!("Tag: {tag}"))
                    .with_description(format!("{count} item(s) with tag '{tag}'"))
                    .with_mime_type("text/markdown")
                    .no_annotation(),
            );
        }

        // Add per-category resources.
        for (cat, count) in storage.index().all_categories() {
            let uri = format!("clerk://categories/{cat}");
            resources.push(
                RawResource::new(&uri, format!("Category: {cat}"))
                    .with_description(format!("{count} item(s) in category '{cat}'"))
                    .with_mime_type("text/markdown")
                    .no_annotation(),
            );
        }

        // Add per-item resources.
        for entry in storage.index().all_items() {
            let uri = format!("clerk://items/{}", entry.id);
            resources.push(
                RawResource::new(&uri, &entry.title)
                    .with_description(format!("{:?}: {}", entry.item_type, entry.title))
                    .with_mime_type("text/markdown")
                    .no_annotation(),
            );
        }

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let templates = vec![
            RawResourceTemplate::new("clerk://tags/{tag}", "Items by Tag")
                .with_description("List all items with a specific tag")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new("clerk://categories/{category}", "Items by Category")
                .with_description("List all items in a specific category")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new("clerk://items/{id}", "Item by ID")
                .with_description("Full content of a specific item")
                .with_mime_type("text/markdown")
                .no_annotation(),
        ];

        Ok(ListResourceTemplatesResult {
            resource_templates: templates,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = &request.uri;
        let storage = self.storage.read().await;

        let content = if uri == "clerk://items" {
            render_all_items(storage.index())
        } else if uri == "clerk://notes" {
            render_items_by_type(storage.index(), &ItemType::Note, "Notes")
        } else if uri == "clerk://todos" {
            render_todos(storage.index())
        } else if uri == "clerk://documents" {
            render_documents(storage.index())
        } else if uri == "clerk://tags" {
            render_tag_cloud(storage.index())
        } else if let Some(tag) = uri.strip_prefix("clerk://tags/") {
            render_items_for_tag(storage.index(), tag)
        } else if let Some(cat) = uri.strip_prefix("clerk://categories/") {
            render_items_for_category(storage.index(), cat)
        } else if let Some(id) = uri.strip_prefix("clerk://items/") {
            render_item_full(&storage, id)?
        } else {
            return Err(McpError::invalid_params(
                format!("Unknown resource URI: {uri}"),
                None,
            ));
        };

        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(content, uri.clone()).with_mime_type("text/markdown"),
        ]))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn ok_json(value: &impl serde::Serialize) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

fn ok_split_json(items: &[Item], deleted: usize) -> Result<CallToolResult, McpError> {
    let mut obj = serde_json::json!({
        "total_parts": items.len(),
        "message": format!("Content was split into {} parts", items.len()),
        "parts": items,
    });
    if deleted > 0 {
        obj["replaced"] = serde_json::json!(format!("Removed {deleted} existing document(s) from this URL"));
    }
    let json = serde_json::to_string_pretty(&obj)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

fn ok_json_with_note(value: &impl serde::Serialize, note: &str) -> Result<CallToolResult, McpError> {
    let mut obj = serde_json::to_value(value)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    if let Some(map) = obj.as_object_mut() {
        map.insert("_note".to_string(), serde_json::Value::String(note.to_string()));
    }
    let json = serde_json::to_string_pretty(&obj)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

async fn download_url_content(
    url: &str,
    tmp_dir: &std::path::Path,
    timeout_secs: u64,
) -> anyhow::Result<String> {
    tracing::info!(%url, "downloading URL content");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {} for URL: {}", response.status(), url);
    }

    let file_name = format!("download-{}.tmp", uuid::Uuid::now_v7());
    let tmp_path = tmp_dir.join(&file_name);

    let bytes = response.bytes().await?;
    tokio::fs::write(&tmp_path, &bytes).await?;

    let content = tokio::fs::read_to_string(&tmp_path).await;

    // Clean up temp file regardless of read result.
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let content = content?;

    tracing::info!(%url, bytes = content.len(), "URL content downloaded");
    Ok(content)
}

fn parse_priority(s: Option<&str>) -> Priority {
    match s {
        Some("low") => Priority::Low,
        Some("high") => Priority::High,
        _ => Priority::Medium,
    }
}

fn parse_due_date(s: Option<&str>) -> Result<Option<NaiveDate>, McpError> {
    match s {
        None => Ok(None),
        Some(date_str) => date_str
            .parse::<NaiveDate>()
            .map(Some)
            .map_err(|_| McpError::invalid_params(
                format!("invalid due date: {date_str}. Use YYYY-MM-DD format"),
                None,
            )),
    }
}

fn parse_item_type(s: &str) -> Option<ItemType> {
    match s {
        "note" => Some(ItemType::Note),
        "todo" => Some(ItemType::Todo),
        "document" => Some(ItemType::Document),
        _ => None,
    }
}

fn index_entry_to_json(entry: &crate::storage::index::IndexEntry) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "id": entry.id,
        "title": entry.title,
        "type": match entry.item_type {
            ItemType::Note => "note",
            ItemType::Todo => "todo",
            ItemType::Document => "document",
        },
        "tags": entry.tags,
        "category": entry.category,
        "created": entry.created.to_rfc3339(),
        "updated": entry.updated.to_rfc3339(),
    });
    if let Some(ref url) = entry.source_url {
        obj["source_url"] = serde_json::Value::String(url.clone());
    }
    obj
}

// ── Resource rendering helpers ───────────────────────────────────────────

fn render_all_items(index: &crate::storage::index::Index) -> String {
    let items = index.all_items();
    if items.is_empty() {
        return "# All Items\n\nNo items found.".to_string();
    }

    let mut lines = vec![format!("# All Items ({} total)\n", items.len())];
    for entry in &items {
        let type_str = match entry.item_type {
            ItemType::Note => "note",
            ItemType::Todo => "todo",
            ItemType::Document => "document",
        };
        let tags = if entry.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", entry.tags.join(", "))
        };
        lines.push(format!(
            "- **{}** ({}) — {}{}", entry.title, type_str, entry.id, tags
        ));
    }
    lines.join("\n")
}

fn render_items_by_type(
    index: &crate::storage::index::Index,
    item_type: &ItemType,
    label: &str,
) -> String {
    let items = index.find_by_type(item_type);
    if items.is_empty() {
        return format!("# {label}\n\nNo {label} found.");
    }

    let mut lines = vec![format!("# {label} ({} total)\n", items.len())];
    for entry in &items {
        let tags = if entry.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", entry.tags.join(", "))
        };
        lines.push(format!("- **{}** — {}{}", entry.title, entry.id, tags));
    }
    lines.join("\n")
}

fn render_todos(index: &crate::storage::index::Index) -> String {
    let items = index.find_by_type(&ItemType::Todo);
    if items.is_empty() {
        return "# Todos\n\nNo todos found.".to_string();
    }

    let mut lines = vec![format!("# Todos ({} total)\n", items.len())];
    for entry in &items {
        let status = crate::storage::markdown::read_item_from_file(&entry.file_path)
            .ok()
            .and_then(|item| {
                if let Item::Todo(t) = item {
                    Some(match t.status {
                        TodoStatus::Pending => "pending",
                        TodoStatus::InProgress => "in_progress",
                        TodoStatus::Done => "done",
                    })
                } else {
                    None
                }
            })
            .unwrap_or("unknown");
        lines.push(format!(
            "- **{}** [{}] — {}", entry.title, status, entry.id
        ));
    }
    lines.join("\n")
}

fn render_documents(index: &crate::storage::index::Index) -> String {
    let items = index.find_by_type(&ItemType::Document);
    if items.is_empty() {
        return "# Documents\n\nNo documents found.".to_string();
    }

    let mut lines = vec![format!("# Documents ({} total)\n", items.len())];
    for entry in &items {
        let summary = crate::storage::markdown::read_item_from_file(&entry.file_path)
            .ok()
            .and_then(|item| {
                if let Item::Document(d) = item {
                    d.summary
                } else {
                    None
                }
            });
        let summary_str = summary
            .map(|s| format!(" -- {s}"))
            .unwrap_or_default();
        lines.push(format!(
            "- **{}** — {}{}", entry.title, entry.id, summary_str
        ));
    }
    lines.join("\n")
}

fn render_tag_cloud(index: &crate::storage::index::Index) -> String {
    let tags = index.all_tags();
    if tags.is_empty() {
        return "# Tags\n\nNo tags found.".to_string();
    }

    let mut sorted = tags;
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut lines = vec![format!("# Tags ({} total)\n", sorted.len())];
    for (tag, count) in &sorted {
        lines.push(format!("- **{tag}** ({count})"));
    }
    lines.join("\n")
}

fn render_items_for_tag(index: &crate::storage::index::Index, tag: &str) -> String {
    let items = index.find_by_tag(tag);
    if items.is_empty() {
        return format!("# Tag: {tag}\n\nNo items with tag '{tag}'.");
    }

    let mut lines = vec![format!("# Tag: {tag} ({} items)\n", items.len())];
    for entry in &items {
        let type_str = match entry.item_type {
            ItemType::Note => "note",
            ItemType::Todo => "todo",
            ItemType::Document => "document",
        };
        lines.push(format!(
            "- **{}** ({}) — {}", entry.title, type_str, entry.id
        ));
    }
    lines.join("\n")
}

fn render_items_for_category(index: &crate::storage::index::Index, category: &str) -> String {
    let items = index.find_by_category(category);
    if items.is_empty() {
        return format!("# Category: {category}\n\nNo items in category '{category}'.");
    }

    let mut lines = vec![format!("# Category: {category} ({} items)\n", items.len())];
    for entry in &items {
        let type_str = match entry.item_type {
            ItemType::Note => "note",
            ItemType::Todo => "todo",
            ItemType::Document => "document",
        };
        lines.push(format!(
            "- **{}** ({}) — {}", entry.title, type_str, entry.id
        ));
    }
    lines.join("\n")
}

fn render_item_full(storage: &Storage, id: &str) -> Result<String, McpError> {
    let item = storage
        .read_item(id)
        .map_err(|e| McpError::invalid_params(format!("Item not found: {e}"), None))?;

    let meta = item.meta();
    let type_str = match meta.item_type {
        ItemType::Note => "Note",
        ItemType::Todo => "Todo",
        ItemType::Document => "Document",
    };
    let tags = if meta.tags.is_empty() {
        "none".to_string()
    } else {
        meta.tags.join(", ")
    };
    let category = meta.category.as_deref().unwrap_or("none");

    let mut lines = vec![
        format!("# {}", meta.title),
        String::new(),
        format!("**Type:** {type_str}"),
        format!("**ID:** {}", meta.id),
        format!("**Tags:** {tags}"),
        format!("**Category:** {category}"),
    ];
    if let Some(ref url) = meta.source_url {
        lines.push(format!("**Source URL:** {url}"));
    }
    lines.push(format!("**Created:** {}", meta.created.to_rfc3339()));
    lines.push(format!("**Updated:** {}", meta.updated.to_rfc3339()));

    match &item {
        Item::Note(n) => {
            lines.push(String::new());
            lines.push("---".to_string());
            lines.push(String::new());
            lines.push(n.content.clone());
        }
        Item::Todo(t) => {
            let status = match t.status {
                TodoStatus::Pending => "pending",
                TodoStatus::InProgress => "in_progress",
                TodoStatus::Done => "done",
            };
            let priority = match t.priority {
                Priority::Low => "low",
                Priority::Medium => "medium",
                Priority::High => "high",
            };
            lines.push(format!("**Status:** {status}"));
            lines.push(format!("**Priority:** {priority}"));
            if let Some(ref due) = t.due {
                lines.push(format!("**Due:** {due}"));
            }
            lines.push(String::new());
            lines.push("---".to_string());
            lines.push(String::new());
            lines.push(t.description.clone());
        }
        Item::Document(d) => {
            if let Some(ref summary) = d.summary {
                lines.push(format!("**Summary:** {summary}"));
            }
            lines.push(String::new());
            lines.push("---".to_string());
            lines.push(String::new());
            lines.push(d.content.clone());
        }
    }

    Ok(lines.join("\n"))
}
