pub mod index;
pub mod markdown;

use std::path::PathBuf;

use anyhow::{Result, bail};
use chrono::Utc;

use crate::config::Config;
use crate::models::{Item, ItemType};
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

        for subdir in &["notes", "todos", "documents"] {
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
        Storage::new(&config).unwrap();

        assert!(dir.path().join("notes").exists());
        assert!(dir.path().join("todos").exists());
        assert!(dir.path().join("documents").exists());
    }
}
