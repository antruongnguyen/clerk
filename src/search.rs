use crate::models::ItemType;
use crate::storage::index::{Index, IndexEntry};

/// A scored search result referencing an index entry.
struct ScoredEntry<'a> {
    entry: &'a IndexEntry,
    score: u32,
}

/// Search items by query string, with optional type/tag/category filters.
///
/// Matching is case-insensitive substring across title, tags, and category.
/// For entries that match metadata, their file content is also checked.
/// Results are sorted by relevance: title match > tag match > category match > content match.
pub fn search_items<'a>(
    index: &'a Index,
    query: &str,
    type_filter: Option<&ItemType>,
    tag_filter: Option<&[String]>,
    category_filter: Option<&str>,
) -> Vec<&'a IndexEntry> {
    let query_lower = query.to_lowercase();
    let mut scored: Vec<ScoredEntry<'a>> = Vec::new();

    for entry in index.all_items() {
        // Apply type filter.
        if let Some(tf) = type_filter
            && &entry.item_type != tf
        {
            continue;
        }

        // Apply tag filter (all specified tags must be present).
        if let Some(tags) = tag_filter {
            let entry_tags_lower: Vec<String> =
                entry.tags.iter().map(|t| t.to_lowercase()).collect();
            if !tags
                .iter()
                .all(|t| entry_tags_lower.contains(&t.to_lowercase()))
            {
                continue;
            }
        }

        // Apply category filter.
        if let Some(cat) = category_filter {
            match &entry.category {
                Some(c) if c.to_lowercase() == cat.to_lowercase() => {}
                _ => continue,
            }
        }

        // Score by matching the query.
        let mut score = 0u32;

        // Title match (highest weight).
        if entry.title.to_lowercase().contains(&query_lower) {
            score += 100;
        }

        // Tag match.
        for tag in &entry.tags {
            if tag.to_lowercase().contains(&query_lower) {
                score += 50;
                break;
            }
        }

        // Category match.
        if let Some(ref cat) = entry.category
            && cat.to_lowercase().contains(&query_lower)
        {
            score += 25;
        }

        // Content match (read from disk only if no metadata match yet).
        if score == 0
            && let Ok(content) = std::fs::read_to_string(&entry.file_path)
            && content.to_lowercase().contains(&query_lower)
        {
            score += 10;
        }

        if score > 0 {
            scored.push(ScoredEntry { entry, score });
        }
    }

    // Sort by score descending, then by title ascending as tiebreaker.
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.entry.title.cmp(&b.entry.title))
    });

    scored.into_iter().map(|s| s.entry).collect()
}

/// List items with optional filters and pagination.
pub fn list_items<'a>(
    index: &'a Index,
    type_filter: Option<&ItemType>,
    tag_filter: Option<&[String]>,
    category_filter: Option<&str>,
    status_filter: Option<&str>,
    limit: usize,
    offset: usize,
) -> (Vec<&'a IndexEntry>, usize) {
    let mut results: Vec<&IndexEntry> = Vec::new();

    for entry in index.all_items() {
        if let Some(tf) = type_filter
            && &entry.item_type != tf
        {
            continue;
        }

        if let Some(tags) = tag_filter {
            let entry_tags_lower: Vec<String> =
                entry.tags.iter().map(|t| t.to_lowercase()).collect();
            if !tags
                .iter()
                .all(|t| entry_tags_lower.contains(&t.to_lowercase()))
            {
                continue;
            }
        }

        if let Some(cat) = category_filter {
            match &entry.category {
                Some(c) if c.to_lowercase() == cat.to_lowercase() => {}
                _ => continue,
            }
        }

        // Status filter: only applies to todos — read the file to check.
        if let Some(status) = status_filter {
            if entry.item_type != ItemType::Todo {
                continue;
            }
            let file_path = &entry.file_path;
            if let Ok(crate::models::Item::Todo(ref t)) =
                crate::storage::markdown::read_item_from_file(file_path)
            {
                let status_str = match t.status {
                    crate::models::TodoStatus::Pending => "pending",
                    crate::models::TodoStatus::InProgress => "in_progress",
                    crate::models::TodoStatus::Done => "done",
                };
                if status_str != status {
                    continue;
                }
            } else {
                continue;
            }
        }

        results.push(entry);
    }

    // Sort by updated descending (most recent first).
    results.sort_by(|a, b| b.updated.cmp(&a.updated));

    let total = results.len();

    let paginated: Vec<&IndexEntry> = results.into_iter().skip(offset).take(limit).collect();

    (paginated, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Item, ItemMeta, ItemType, Note, Priority, Todo, TodoStatus};
    use crate::storage::index::Index;
    use crate::storage::markdown;

    fn setup_test_storage() -> (tempfile::TempDir, Index) {
        let dir = tempfile::tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        let todos_dir = dir.path().join("todos");
        let docs_dir = dir.path().join("documents");
        std::fs::create_dir_all(&notes_dir).unwrap();
        std::fs::create_dir_all(&todos_dir).unwrap();
        std::fs::create_dir_all(&docs_dir).unwrap();

        let note_meta = ItemMeta {
            id: "n1".to_string(),
            title: "Rust Async Patterns".to_string(),
            item_type: ItemType::Note,
            tags: vec!["rust".to_string(), "async".to_string()],
            category: Some("engineering".to_string()),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        };
        let note = Item::Note(Note {
            meta: note_meta,
            content: "Exploring async/await in Rust.".to_string(),
        });
        let note_path = notes_dir.join("rust-async-patterns.md");
        markdown::write_item_to_file(&note_path, &note).unwrap();

        let todo_meta = ItemMeta {
            id: "t1".to_string(),
            title: "Fix login bug".to_string(),
            item_type: ItemType::Todo,
            tags: vec!["bug".to_string(), "auth".to_string()],
            category: Some("engineering".to_string()),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        };
        let todo = Item::Todo(Todo {
            meta: todo_meta,
            description: "The login form rejects valid passwords.".to_string(),
            status: TodoStatus::Pending,
            priority: Priority::High,
            due: None,
        });
        let todo_path = todos_dir.join("fix-login-bug.md");
        markdown::write_item_to_file(&todo_path, &todo).unwrap();

        let index = Index::build(dir.path()).unwrap();
        (dir, index)
    }

    #[test]
    fn test_search_by_title() {
        let (_dir, index) = setup_test_storage();
        let results = search_items(&index, "Rust", None, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "n1");
    }

    #[test]
    fn test_search_by_tag() {
        let (_dir, index) = setup_test_storage();
        let results = search_items(&index, "bug", None, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "t1");
    }

    #[test]
    fn test_search_by_content() {
        let (_dir, index) = setup_test_storage();
        let results = search_items(&index, "passwords", None, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "t1");
    }

    #[test]
    fn test_search_with_type_filter() {
        let (_dir, index) = setup_test_storage();
        let results = search_items(
            &index,
            "engineering",
            Some(&ItemType::Note),
            None,
            None,
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "n1");
    }

    #[test]
    fn test_list_items_pagination() {
        let (_dir, index) = setup_test_storage();
        let (results, total) = list_items(&index, None, None, None, None, 1, 0);
        assert_eq!(total, 2);
        assert_eq!(results.len(), 1);

        let (results, _) = list_items(&index, None, None, None, None, 1, 1);
        assert_eq!(results.len(), 1);
    }
}
