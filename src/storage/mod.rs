pub mod index;
pub mod markdown;
pub mod split;

use std::path::PathBuf;

use anyhow::{Result, bail};
use chrono::Utc;

use crate::config::Config;
use crate::models::{Document, Item, ItemMeta, ItemType};
use index::{Index, IndexEntry};

/// Manages reading, writing, and indexing items on disk.
pub struct Storage {
    data_dir: PathBuf,
    index: Index,
    max_content_length: usize,
}

impl Storage {
    /// Create a new `Storage`, ensuring subdirectories exist and building the index.
    pub fn new(config: &Config) -> Result<Self> {
        let data_dir = &config.data_dir;

        for subdir in &["notes", "todos", "documents", "tmp"] {
            let dir = data_dir.join(subdir);
            std::fs::create_dir_all(&dir)?;
        }

        let index = Index::build(data_dir)?;

        Ok(Self {
            data_dir: data_dir.clone(),
            index,
            max_content_length: config.max_content_length,
        })
    }

    /// Return a reference to the in-memory index.
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Return the path to the temporary download directory.
    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }

    /// Create a new item, write it to disk, and add it to the index.
    pub fn create_item(&mut self, item: Item) -> Result<Item> {
        self.validate_content_length(&item)?;

        let meta = item.meta();
        let subdir = type_subdir(&meta.item_type);
        let dir = self.data_dir.join(subdir);

        let slug = markdown::generate_slug(&meta.title);
        let final_slug = markdown::resolve_collision(&dir, &slug);
        let file_path = dir.join(format!("{final_slug}.md"));

        markdown::write_item_to_file(&file_path, &item)?;

        let idx_entry = IndexEntry {
            id: meta.id.clone(),
            title: meta.title.clone(),
            item_type: meta.item_type.clone(),
            tags: meta.tags.clone(),
            category: meta.category.clone(),
            source_url: meta.source_url.clone(),
            file_path,
            created: meta.created,
            updated: meta.updated,
        };
        self.index.add(idx_entry);

        tracing::debug!(id = %meta.id, title = %meta.title, "item created");
        Ok(item)
    }

    /// Read an item from disk by its ID.
    pub fn read_item(&self, id: &str) -> Result<Item> {
        let entry = self
            .index
            .get_by_id(id)
            .ok_or_else(|| anyhow::anyhow!("item not found: {id}"))?;

        markdown::read_item_from_file(&entry.file_path)
    }

    /// Update an existing item on disk and refresh the index.
    pub fn update_item(&mut self, mut item: Item) -> Result<Item> {
        self.validate_content_length(&item)?;

        let id = item.meta().id.clone();
        let entry = self
            .index
            .get_by_id(&id)
            .ok_or_else(|| anyhow::anyhow!("item not found: {id}"))?;

        let file_path = entry.file_path.clone();

        // Update the timestamp.
        item.meta_mut().updated = Utc::now();

        markdown::write_item_to_file(&file_path, &item)?;

        let meta = item.meta();
        let idx_entry = IndexEntry {
            id: meta.id.clone(),
            title: meta.title.clone(),
            item_type: meta.item_type.clone(),
            tags: meta.tags.clone(),
            category: meta.category.clone(),
            source_url: meta.source_url.clone(),
            file_path,
            created: meta.created,
            updated: meta.updated,
        };
        self.index.update(idx_entry);

        tracing::debug!(id = %meta.id, "item updated");
        Ok(item)
    }

    /// Delete an item from disk and remove it from the index.
    pub fn delete_item(&mut self, id: &str) -> Result<()> {
        let entry = self
            .index
            .get_by_id(id)
            .ok_or_else(|| anyhow::anyhow!("item not found: {id}"))?;

        let file_path = entry.file_path.clone();

        std::fs::remove_file(&file_path)?;
        self.index.remove(id);

        tracing::debug!(id, "item deleted");
        Ok(())
    }

    /// Delete all items that share a given source URL. Returns the count of deleted items.
    pub fn delete_items_by_source_url(&mut self, url: &str) -> Result<usize> {
        let ids: Vec<String> = self
            .index
            .find_by_source_url(url)
            .iter()
            .map(|e| e.id.clone())
            .collect();

        let count = ids.len();
        for id in &ids {
            self.delete_item(id)?;
        }

        if count > 0 {
            tracing::info!(url, count, "deleted existing items for source URL");
        }
        Ok(count)
    }

    fn validate_content_length(&self, item: &Item) -> Result<()> {
        let content_len = match item {
            Item::Note(n) => n.content.len(),
            Item::Todo(t) => t.description.len(),
            Item::Document(d) => d.content.len(),
        };

        if content_len > self.max_content_length {
            bail!(
                "content length ({content_len}) exceeds maximum ({})",
                self.max_content_length
            );
        }
        Ok(())
    }

    /// Create a document, automatically splitting into multiple parts if content exceeds
    /// the maximum length. Pre-scans the content to build a heading outline, then splits
    /// at heading boundaries with breadcrumb metadata per chunk.
    pub fn create_document_split(
        &mut self,
        title: String,
        content: String,
        summary: Option<String>,
        tags: Vec<String>,
        category: Option<String>,
        source_url: Option<String>,
    ) -> Result<Vec<Item>> {
        let outline = split::prescan_outline(&content);
        let chunks = split::split_content_with_outline(&content, self.max_content_length, &outline);
        let total = chunks.len();

        if total == 1 {
            let chunk = chunks.into_iter().next().unwrap();
            let mut meta = ItemMeta::new(title, ItemType::Document);
            meta.tags = tags;
            meta.category = category;
            meta.source_url = source_url;

            let item = Item::Document(Document {
                meta,
                content: chunk.content,
                summary,
            });
            let created = self.create_item(item)?;
            return Ok(vec![created]);
        }

        tracing::info!(total, title = %title, "splitting document into multiple parts");

        let mut created_items = Vec::with_capacity(total);
        for (i, chunk) in chunks.into_iter().enumerate() {
            let part_num = i + 1;

            // Use the breadcrumb heading path as the part title when available.
            let part_title = if let Some(ref heading) = chunk.heading {
                format!("{title} — {heading}")
            } else {
                format!("{title} (Part {part_num} of {total})")
            };

            // Part 1 gets the user-provided summary plus a TOC of all parts.
            // Subsequent parts get a summary derived from their breadcrumb.
            let part_summary = if part_num == 1 {
                let mut s = summary.clone().unwrap_or_default();
                if !outline.toc.is_empty() {
                    if !s.is_empty() {
                        s.push_str("\n\n");
                    }
                    s.push_str("## Table of Contents\n\n");
                    s.push_str(&outline.toc);
                }
                Some(s)
            } else if let Some(ref heading) = chunk.heading {
                Some(format!(
                    "Part {part_num} of {total} from \"{title}\". Section: {heading}."
                ))
            } else {
                Some(format!(
                    "Part {part_num} of {total} from \"{title}\"."
                ))
            };

            let mut meta = ItemMeta::new(part_title, ItemType::Document);
            meta.tags = tags.clone();
            meta.category = category.clone();
            meta.source_url = source_url.clone();

            let item = Item::Document(Document {
                meta,
                content: chunk.content,
                summary: part_summary,
            });

            match self.create_item(item) {
                Ok(created) => created_items.push(created),
                Err(e) => {
                    tracing::error!(
                        part = part_num,
                        total,
                        error = %e,
                        "failed to create part, {} parts already created",
                        created_items.len()
                    );
                    bail!(
                        "failed to create part {part_num} of {total}: {e}. \
                         {} parts were created before the failure.",
                        created_items.len()
                    );
                }
            }
        }

        Ok(created_items)
    }
}

fn type_subdir(item_type: &ItemType) -> &'static str {
    match item_type {
        ItemType::Note => "notes",
        ItemType::Todo => "todos",
        ItemType::Document => "documents",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ItemMeta, Note, Todo, TodoStatus, Priority};

    fn test_config(dir: &std::path::Path) -> Config {
        Config {
            data_dir: dir.to_path_buf(),
            max_content_length: 10_000,
            http_bind: None,
            log_level: None,
            download_timeout_secs: 30,
        }
    }

    #[test]
    fn test_create_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let mut storage = Storage::new(&config).unwrap();

        let meta = ItemMeta::new("Test Note".to_string(), ItemType::Note);
        let id = meta.id.clone();
        let item = Item::Note(Note {
            meta,
            content: "Hello!".to_string(),
        });

        storage.create_item(item).unwrap();

        let loaded = storage.read_item(&id).unwrap();
        assert_eq!(loaded.meta().title, "Test Note");
        if let Item::Note(n) = loaded {
            assert_eq!(n.content, "Hello!");
        } else {
            panic!("expected Note");
        }
    }

    #[test]
    fn test_update() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let mut storage = Storage::new(&config).unwrap();

        let meta = ItemMeta::new("Original".to_string(), ItemType::Note);
        let id = meta.id.clone();
        let item = Item::Note(Note {
            meta,
            content: "v1".to_string(),
        });
        storage.create_item(item).unwrap();

        let mut loaded = storage.read_item(&id).unwrap();
        loaded.meta_mut().title = "Updated".to_string();
        if let Item::Note(ref mut n) = loaded {
            n.content = "v2".to_string();
        }
        storage.update_item(loaded).unwrap();

        let reloaded = storage.read_item(&id).unwrap();
        assert_eq!(reloaded.meta().title, "Updated");
        if let Item::Note(n) = reloaded {
            assert_eq!(n.content, "v2");
        }
    }

    #[test]
    fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let mut storage = Storage::new(&config).unwrap();

        let meta = ItemMeta::new("To Delete".to_string(), ItemType::Note);
        let id = meta.id.clone();
        let item = Item::Note(Note {
            meta,
            content: "bye".to_string(),
        });
        storage.create_item(item).unwrap();

        storage.delete_item(&id).unwrap();
        assert!(storage.read_item(&id).is_err());
    }

    #[test]
    fn test_content_length_validation() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = test_config(dir.path());
        config.max_content_length = 10;
        let mut storage = Storage::new(&config).unwrap();

        let meta = ItemMeta::new("Too Long".to_string(), ItemType::Note);
        let item = Item::Note(Note {
            meta,
            content: "a".repeat(11),
        });

        assert!(storage.create_item(item).is_err());
    }

    #[test]
    fn test_index_updated_on_crud() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let mut storage = Storage::new(&config).unwrap();

        let mut meta = ItemMeta::new("Tagged".to_string(), ItemType::Note);
        meta.tags = vec!["rust".to_string()];
        meta.category = Some("work".to_string());
        let id = meta.id.clone();
        let item = Item::Note(Note {
            meta,
            content: "content".to_string(),
        });

        storage.create_item(item).unwrap();

        assert_eq!(storage.index().find_by_tag("rust").len(), 1);
        assert_eq!(storage.index().find_by_category("work").len(), 1);

        storage.delete_item(&id).unwrap();

        assert_eq!(storage.index().find_by_tag("rust").len(), 0);
        assert_eq!(storage.index().find_by_category("work").len(), 0);
    }

    #[test]
    fn test_create_todo() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let mut storage = Storage::new(&config).unwrap();

        let meta = ItemMeta::new("Fix bug".to_string(), ItemType::Todo);
        let id = meta.id.clone();
        let item = Item::Todo(Todo {
            meta,
            description: "Fix the login bug".to_string(),
            status: TodoStatus::Pending,
            priority: Priority::High,
            due: None,
        });

        storage.create_item(item).unwrap();

        let loaded = storage.read_item(&id).unwrap();
        if let Item::Todo(t) = loaded {
            assert_eq!(t.meta.title, "Fix bug");
            assert_eq!(t.status, TodoStatus::Pending);
            assert_eq!(t.priority, Priority::High);
        } else {
            panic!("expected Todo");
        }
    }

    #[test]
    fn test_directories_created() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let storage = Storage::new(&config).unwrap();

        assert!(dir.path().join("notes").exists());
        assert!(dir.path().join("todos").exists());
        assert!(dir.path().join("documents").exists());
        assert!(dir.path().join("tmp").exists());
        assert_eq!(storage.tmp_dir(), dir.path().join("tmp"));
    }
}
