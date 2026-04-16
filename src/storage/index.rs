use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Result, Context};

use crate::models::ItemType;
use super::markdown;

/// A lightweight entry in the in-memory index.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub id: String,
    pub title: String,
    pub item_type: ItemType,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub source_url: Option<String>,
    pub file_path: PathBuf,
    pub created: chrono::DateTime<chrono::Utc>,
    pub updated: chrono::DateTime<chrono::Utc>,
}

/// In-memory index of all items, with secondary indexes on tags and categories.
#[derive(Debug, Clone)]
pub struct Index {
    /// Primary index: item ID -> entry.
    items: HashMap<String, IndexEntry>,
    /// Tag -> set of item IDs that carry that tag.
    tags_index: HashMap<String, HashSet<String>>,
    /// Category -> set of item IDs in that category.
    categories_index: HashMap<String, HashSet<String>>,
    /// Source URL -> set of item IDs created from that URL.
    source_url_index: HashMap<String, HashSet<String>>,
}

impl Index {
    /// Build an index by scanning the notes/, todos/, and documents/ subdirectories.
    pub fn build(data_dir: &Path) -> Result<Self> {
        let mut index = Self {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        let subdirs = ["notes", "todos", "documents"];
        for subdir in &subdirs {
            let dir = data_dir.join(subdir);
            if !dir.exists() {
                continue;
            }
            let entries = std::fs::read_dir(&dir)
                .with_context(|| format!("reading directory {}", dir.display()))?;
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    match markdown::read_item_from_file(&path) {
                        Ok(item) => {
                            let meta = item.meta();
                            let idx_entry = IndexEntry {
                                id: meta.id.clone(),
                                title: meta.title.clone(),
                                item_type: meta.item_type.clone(),
                                tags: meta.tags.clone(),
                                category: meta.category.clone(),
                                source_url: meta.source_url.clone(),
                                file_path: path.clone(),
                                created: meta.created,
                                updated: meta.updated,
                            };
                            index.add(idx_entry);
                        }
                        Err(e) => {
                            tracing::warn!(path = %path.display(), error = %e, "skipping unreadable file");
                        }
                    }
                }
            }
        }

        tracing::info!(items = index.items.len(), "index built");
        Ok(index)
    }

    /// Insert an entry into the index, updating all secondary indexes.
    pub fn add(&mut self, entry: IndexEntry) {
        let id = entry.id.clone();

        for tag in &entry.tags {
            self.tags_index
                .entry(tag.clone())
                .or_default()
                .insert(id.clone());
        }

        if let Some(ref cat) = entry.category {
            self.categories_index
                .entry(cat.clone())
                .or_default()
                .insert(id.clone());
        }

        if let Some(ref url) = entry.source_url {
            self.source_url_index
                .entry(url.clone())
                .or_default()
                .insert(id.clone());
        }

        self.items.insert(id, entry);
    }

    /// Remove an entry by ID, cleaning up secondary indexes.
    pub fn remove(&mut self, id: &str) -> Option<IndexEntry> {
        let entry = self.items.remove(id)?;

        for tag in &entry.tags {
            if let Some(set) = self.tags_index.get_mut(tag) {
                set.remove(id);
                if set.is_empty() {
                    self.tags_index.remove(tag);
                }
            }
        }

        if let Some(ref cat) = entry.category
            && let Some(set) = self.categories_index.get_mut(cat)
        {
            set.remove(id);
            if set.is_empty() {
                self.categories_index.remove(cat);
            }
        }

        if let Some(ref url) = entry.source_url
            && let Some(set) = self.source_url_index.get_mut(url)
        {
            set.remove(id);
            if set.is_empty() {
                self.source_url_index.remove(url);
            }
        }

        Some(entry)
    }

    /// Replace an existing entry. Removes the old one first, then adds the new one.
    pub fn update(&mut self, entry: IndexEntry) {
        self.remove(&entry.id.clone());
        self.add(entry);
    }

    /// Look up an entry by ID.
    pub fn get_by_id(&self, id: &str) -> Option<&IndexEntry> {
        self.items.get(id)
    }

    /// Find all entries that carry a given tag.
    pub fn find_by_tag(&self, tag: &str) -> Vec<&IndexEntry> {
        self.tags_index
            .get(tag)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.items.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find all entries in a given category.
    pub fn find_by_category(&self, category: &str) -> Vec<&IndexEntry> {
        self.categories_index
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.items.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find all entries created from a given source URL.
    pub fn find_by_source_url(&self, url: &str) -> Vec<&IndexEntry> {
        self.source_url_index
            .get(url)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.items.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find all entries of a given item type.
    pub fn find_by_type(&self, item_type: &ItemType) -> Vec<&IndexEntry> {
        self.items
            .values()
            .filter(|e| &e.item_type == item_type)
            .collect()
    }

    /// Return all tags with their item counts, sorted alphabetically.
    pub fn all_tags(&self) -> Vec<(String, usize)> {
        let mut tags: Vec<(String, usize)> = self
            .tags_index
            .iter()
            .map(|(tag, ids)| (tag.clone(), ids.len()))
            .collect();
        tags.sort_by(|a, b| a.0.cmp(&b.0));
        tags
    }

    /// Return all categories with their item counts, sorted alphabetically.
    pub fn all_categories(&self) -> Vec<(String, usize)> {
        let mut cats: Vec<(String, usize)> = self
            .categories_index
            .iter()
            .map(|(cat, ids)| (cat.clone(), ids.len()))
            .collect();
        cats.sort_by(|a, b| a.0.cmp(&b.0));
        cats
    }

    /// Find items related to the given item by shared tags.
    ///
    /// Returns entries sorted by number of overlapping tags (descending),
    /// excluding the item itself.
    pub fn find_related(&self, id: &str) -> Vec<&IndexEntry> {
        let entry = match self.items.get(id) {
            Some(e) => e,
            None => return Vec::new(),
        };

        // Count how many tags each other item shares with the given item.
        let mut overlap: HashMap<&str, usize> = HashMap::new();
        for tag in &entry.tags {
            if let Some(ids) = self.tags_index.get(tag) {
                for other_id in ids {
                    if other_id != id {
                        *overlap.entry(other_id.as_str()).or_default() += 1;
                    }
                }
            }
        }

        let mut related: Vec<(&str, usize)> = overlap.into_iter().collect();
        related.sort_by(|a, b| b.1.cmp(&a.1));

        related
            .into_iter()
            .filter_map(|(rid, _)| self.items.get(rid))
            .collect()
    }

    /// Return all entries.
    pub fn all_items(&self) -> Vec<&IndexEntry> {
        self.items.values().collect()
    }

    /// Find entries by title (case-insensitive exact match) and item type.
    #[allow(dead_code)] // Available for future title-based lookup
    pub fn find_by_title(&self, title: &str, item_type: &ItemType) -> Option<&IndexEntry> {
        let lower = title.to_lowercase();
        self.items
            .values()
            .find(|e| e.item_type == *item_type && e.title.to_lowercase() == lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Item, ItemMeta, ItemType, Note, Document};
    use crate::storage::markdown;

    fn make_note(id: &str, title: &str, tags: &[&str], category: Option<&str>) -> IndexEntry {
        IndexEntry {
            id: id.to_string(),
            title: title.to_string(),
            item_type: ItemType::Note,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            category: category.map(String::from),
            source_url: None,
            file_path: PathBuf::from(format!("notes/{id}.md")),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_add_and_get() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        let entry = make_note("n1", "First Note", &["rust", "test"], Some("engineering"));
        index.add(entry);

        assert!(index.get_by_id("n1").is_some());
        assert_eq!(index.get_by_id("n1").unwrap().title, "First Note");
    }

    #[test]
    fn test_remove() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        let entry = make_note("n1", "First", &["rust"], Some("eng"));
        index.add(entry);
        assert!(index.get_by_id("n1").is_some());

        index.remove("n1");
        assert!(index.get_by_id("n1").is_none());
        assert!(index.find_by_tag("rust").is_empty());
        assert!(index.find_by_category("eng").is_empty());
    }

    #[test]
    fn test_find_by_tag() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "A", &["rust", "test"], None));
        index.add(make_note("n2", "B", &["rust"], None));
        index.add(make_note("n3", "C", &["python"], None));

        assert_eq!(index.find_by_tag("rust").len(), 2);
        assert_eq!(index.find_by_tag("python").len(), 1);
        assert_eq!(index.find_by_tag("go").len(), 0);
    }

    #[test]
    fn test_find_by_category() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "A", &[], Some("work")));
        index.add(make_note("n2", "B", &[], Some("work")));
        index.add(make_note("n3", "C", &[], Some("personal")));

        assert_eq!(index.find_by_category("work").len(), 2);
        assert_eq!(index.find_by_category("personal").len(), 1);
        assert_eq!(index.find_by_category("other").len(), 0);
    }

    #[test]
    fn test_find_by_type() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "A", &[], None));
        index.add(IndexEntry {
            id: "d1".to_string(),
            title: "Doc".to_string(),
            item_type: ItemType::Document,
            tags: vec![],
            category: None,
            source_url: None,
            file_path: PathBuf::from("documents/d1.md"),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        });

        assert_eq!(index.find_by_type(&ItemType::Note).len(), 1);
        assert_eq!(index.find_by_type(&ItemType::Document).len(), 1);
        assert_eq!(index.find_by_type(&ItemType::Todo).len(), 0);
    }

    #[test]
    fn test_all_tags() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "A", &["rust", "test"], None));
        index.add(make_note("n2", "B", &["rust"], None));

        let tags = index.all_tags();
        assert_eq!(tags.len(), 2);
        // Sorted alphabetically
        assert_eq!(tags[0], ("rust".to_string(), 2));
        assert_eq!(tags[1], ("test".to_string(), 1));
    }

    #[test]
    fn test_all_categories() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "A", &[], Some("work")));
        index.add(make_note("n2", "B", &[], Some("work")));
        index.add(make_note("n3", "C", &[], Some("personal")));

        let cats = index.all_categories();
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[0], ("personal".to_string(), 1));
        assert_eq!(cats[1], ("work".to_string(), 2));
    }

    #[test]
    fn test_find_related() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "A", &["rust", "test", "async"], None));
        index.add(make_note("n2", "B", &["rust", "test"], None));
        index.add(make_note("n3", "C", &["rust"], None));
        index.add(make_note("n4", "D", &["python"], None));

        let related = index.find_related("n1");
        // n2 shares 2 tags (rust, test), n3 shares 1 (rust), n4 shares 0
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].id, "n2"); // most overlap
        assert_eq!(related[1].id, "n3");
    }

    #[test]
    fn test_build_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();

        // Write a note file.
        let meta = ItemMeta {
            id: "test-id".to_string(),
            title: "Disk Note".to_string(),
            item_type: ItemType::Note,
            tags: vec!["tag1".to_string()],
            category: Some("cat1".to_string()),
            source_url: None,
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        };
        let item = Item::Note(Note {
            meta,
            content: "Hello from disk".to_string(),
        });
        let file_path = notes_dir.join("disk-note.md");
        markdown::write_item_to_file(&file_path, &item).unwrap();

        // Write a document file.
        let docs_dir = dir.path().join("documents");
        std::fs::create_dir_all(&docs_dir).unwrap();
        let doc_meta = ItemMeta {
            id: "doc-id".to_string(),
            title: "Disk Doc".to_string(),
            item_type: ItemType::Document,
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            category: Some("cat1".to_string()),
            source_url: None,
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        };
        let doc_item = Item::Document(Document {
            meta: doc_meta,
            content: "Document content".to_string(),
            summary: Some("A summary".to_string()),
        });
        markdown::write_item_to_file(&docs_dir.join("disk-doc.md"), &doc_item).unwrap();

        let index = Index::build(dir.path()).unwrap();

        assert_eq!(index.all_items().len(), 2);
        assert!(index.get_by_id("test-id").is_some());
        assert!(index.get_by_id("doc-id").is_some());
        assert_eq!(index.find_by_tag("tag1").len(), 2);
        assert_eq!(index.find_by_tag("tag2").len(), 1);
        assert_eq!(index.find_by_category("cat1").len(), 2);
    }

    #[test]
    fn test_update_entry() {
        let mut index = Index {
            items: HashMap::new(),
            tags_index: HashMap::new(),
            categories_index: HashMap::new(),
            source_url_index: HashMap::new(),
        };

        index.add(make_note("n1", "Old Title", &["old-tag"], Some("old-cat")));
        assert_eq!(index.find_by_tag("old-tag").len(), 1);

        // Update: change tags and category.
        let updated = make_note("n1", "New Title", &["new-tag"], Some("new-cat"));
        index.update(updated);

        assert_eq!(index.get_by_id("n1").unwrap().title, "New Title");
        assert_eq!(index.find_by_tag("old-tag").len(), 0);
        assert_eq!(index.find_by_tag("new-tag").len(), 1);
        assert_eq!(index.find_by_category("old-cat").len(), 0);
        assert_eq!(index.find_by_category("new-cat").len(), 1);
    }
}
