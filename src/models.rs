use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of an item stored in Clerk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Note,
    Todo,
    Document,
}

/// Status of a todo item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
}

/// Priority level for a todo item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
}

/// Common metadata shared across all item types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMeta {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub item_type: ItemType,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl ItemMeta {
    pub fn new(title: String, item_type: ItemType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7().to_string(),
            title,
            item_type,
            tags: Vec::new(),
            category: None,
            source_url: None,
            created: now,
            updated: now,
        }
    }
}

/// A general-purpose note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    #[serde(flatten)]
    pub meta: ItemMeta,
    pub content: String,
}

/// A todo / task item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    #[serde(flatten)]
    pub meta: ItemMeta,
    #[serde(default)]
    pub description: String,
    pub status: TodoStatus,
    pub priority: Priority,
    #[serde(default)]
    pub due: Option<NaiveDate>,
}

/// A longer-form technical document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    #[serde(flatten)]
    pub meta: ItemMeta,
    pub content: String,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Unified enum wrapping all item types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Note(Note),
    Todo(Todo),
    Document(Document),
}

impl Item {
    /// Access the common metadata for any item.
    pub fn meta(&self) -> &ItemMeta {
        match self {
            Item::Note(n) => &n.meta,
            Item::Todo(t) => &t.meta,
            Item::Document(d) => &d.meta,
        }
    }

    /// Access the common metadata mutably for any item.
    pub fn meta_mut(&mut self) -> &mut ItemMeta {
        match self {
            Item::Note(n) => &mut n.meta,
            Item::Todo(t) => &mut t.meta,
            Item::Document(d) => &mut d.meta,
        }
    }
}
