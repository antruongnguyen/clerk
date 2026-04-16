use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};

use crate::models::{Document, Item, ItemMeta, ItemType, Note, Priority, Todo, TodoStatus};

/// Split raw file content into YAML frontmatter and the markdown body.
///
/// Expects the file to start with `---\n`, followed by YAML, then `---\n`,
/// followed by the body. Leading/trailing whitespace in the body is trimmed.
pub fn parse_frontmatter(content: &str) -> Result<(serde_yaml::Value, &str)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        bail!("file does not start with frontmatter delimiter '---'");
    }

    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    let end = after_first
        .find("\n---")
        .context("missing closing frontmatter delimiter '---'")?;

    let yaml_str = &after_first[..end];
    let body_start = end + 4; // skip "\n---"
    let body = if body_start < after_first.len() {
        after_first[body_start..].trim()
    } else {
        ""
    };

    let yaml: serde_yaml::Value =
        serde_yaml::from_str(yaml_str).context("invalid YAML in frontmatter")?;

    Ok((yaml, body))
}

/// Render YAML frontmatter and a body back into a markdown string.
pub fn serialize_frontmatter(meta: &serde_yaml::Value, body: &str) -> String {
    let yaml_str = serde_yaml::to_string(meta).unwrap_or_default();
    let mut out = String::with_capacity(yaml_str.len() + body.len() + 16);
    out.push_str("---\n");
    out.push_str(&yaml_str);
    out.push_str("---\n");
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    out
}

/// Read a markdown file from disk and parse it into an `Item`.
pub fn read_item_from_file(path: &Path) -> Result<Item> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (yaml, body) = parse_frontmatter(&content)?;

    let map = yaml
        .as_mapping()
        .context("frontmatter is not a YAML mapping")?;

    let type_val = map
        .get(serde_yaml::Value::String("type".into()))
        .and_then(|v| v.as_str())
        .context("frontmatter missing 'type' field")?;

    let meta = meta_from_yaml(map)?;

    match type_val {
        "note" => Ok(Item::Note(Note {
            meta,
            content: body.to_string(),
        })),
        "todo" => {
            let status = map
                .get(serde_yaml::Value::String("status".into()))
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            let priority = map
                .get(serde_yaml::Value::String("priority".into()))
                .and_then(|v| v.as_str())
                .unwrap_or("medium");
            let due = map
                .get(serde_yaml::Value::String("due".into()))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<NaiveDate>().ok());

            Ok(Item::Todo(Todo {
                meta,
                description: body.to_string(),
                status: parse_todo_status(status),
                priority: parse_priority(priority),
                due,
            }))
        }
        "document" => {
            let summary = map
                .get(serde_yaml::Value::String("summary".into()))
                .and_then(|v| v.as_str())
                .map(String::from);

            Ok(Item::Document(Document {
                meta,
                content: body.to_string(),
                summary,
            }))
        }
        other => bail!("unknown item type: {other}"),
    }
}

/// Serialize an `Item` and write it atomically to disk.
pub fn write_item_to_file(path: &Path, item: &Item) -> Result<()> {
    let (yaml, body) = item_to_yaml_and_body(item);
    let content = serialize_frontmatter(&yaml, &body);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }

    std::fs::write(path, &content)
        .with_context(|| format!("writing {}", path.display()))?;

    Ok(())
}

/// Generate a URL-safe slug from a title.
///
/// Lowercases, replaces non-alphanumeric characters with hyphens,
/// collapses consecutive hyphens, trims leading/trailing hyphens,
/// and truncates to 60 characters (on a hyphen boundary where possible).
pub fn generate_slug(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens.
    let mut collapsed = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push('-');
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }

    let trimmed = collapsed.trim_matches('-');

    if trimmed.len() <= 60 {
        return trimmed.to_string();
    }

    // Truncate at a hyphen boundary if possible.
    let truncated = &trimmed[..60];
    if let Some(last_hyphen) = truncated.rfind('-') {
        truncated[..last_hyphen].to_string()
    } else {
        truncated.to_string()
    }
}

/// Return a non-colliding filename in `dir` based on `slug`.
///
/// If `slug.md` exists, try `slug-2.md`, `slug-3.md`, etc.
pub fn resolve_collision(dir: &Path, slug: &str) -> String {
    let candidate = format!("{slug}.md");
    if !dir.join(&candidate).exists() {
        return slug.to_string();
    }

    let mut counter = 2u32;
    loop {
        let suffixed = format!("{slug}-{counter}");
        let candidate = format!("{suffixed}.md");
        if !dir.join(&candidate).exists() {
            return suffixed;
        }
        counter += 1;
    }
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn meta_from_yaml(map: &serde_yaml::Mapping) -> Result<ItemMeta> {
    let get_str = |key: &str| -> Option<String> {
        map.get(serde_yaml::Value::String(key.into()))
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    let id = get_str("id").context("frontmatter missing 'id' field")?;
    let title = get_str("title").context("frontmatter missing 'title' field")?;
    let type_str = get_str("type").context("frontmatter missing 'type' field")?;

    let item_type = match type_str.as_str() {
        "note" => ItemType::Note,
        "todo" => ItemType::Todo,
        "document" => ItemType::Document,
        other => bail!("unknown item type: {other}"),
    };

    let tags = map
        .get(serde_yaml::Value::String("tags".into()))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let category = get_str("category");

    let created = get_str("created")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now);

    let updated = get_str("updated")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now);

    Ok(ItemMeta {
        id,
        title,
        item_type,
        tags,
        category,
        created,
        updated,
    })
}

fn parse_todo_status(s: &str) -> TodoStatus {
    match s {
        "in_progress" => TodoStatus::InProgress,
        "done" => TodoStatus::Done,
        _ => TodoStatus::Pending,
    }
}

fn parse_priority(s: &str) -> Priority {
    match s {
        "low" => Priority::Low,
        "high" => Priority::High,
        _ => Priority::Medium,
    }
}

fn item_to_yaml_and_body(item: &Item) -> (serde_yaml::Value, String) {
    let meta = item.meta();
    let mut map = BTreeMap::new();

    map.insert("id".to_string(), serde_yaml::Value::String(meta.id.clone()));
    map.insert(
        "title".to_string(),
        serde_yaml::Value::String(meta.title.clone()),
    );

    let type_str = match meta.item_type {
        ItemType::Note => "note",
        ItemType::Todo => "todo",
        ItemType::Document => "document",
    };
    map.insert(
        "type".to_string(),
        serde_yaml::Value::String(type_str.to_string()),
    );

    if !meta.tags.is_empty() {
        let tags_seq: Vec<serde_yaml::Value> = meta
            .tags
            .iter()
            .map(|t| serde_yaml::Value::String(t.clone()))
            .collect();
        map.insert("tags".to_string(), serde_yaml::Value::Sequence(tags_seq));
    }

    if let Some(ref cat) = meta.category {
        map.insert("category".to_string(), serde_yaml::Value::String(cat.clone()));
    }

    let body = match item {
        Item::Note(n) => n.content.clone(),
        Item::Todo(t) => {
            let status_str = match t.status {
                TodoStatus::Pending => "pending",
                TodoStatus::InProgress => "in_progress",
                TodoStatus::Done => "done",
            };
            map.insert(
                "status".to_string(),
                serde_yaml::Value::String(status_str.to_string()),
            );

            let priority_str = match t.priority {
                Priority::Low => "low",
                Priority::Medium => "medium",
                Priority::High => "high",
            };
            map.insert(
                "priority".to_string(),
                serde_yaml::Value::String(priority_str.to_string()),
            );

            if let Some(ref due) = t.due {
                map.insert(
                    "due".to_string(),
                    serde_yaml::Value::String(due.to_string()),
                );
            }

            t.description.clone()
        }
        Item::Document(d) => {
            if let Some(ref summary) = d.summary {
                map.insert(
                    "summary".to_string(),
                    serde_yaml::Value::String(summary.clone()),
                );
            }
            d.content.clone()
        }
    };

    map.insert(
        "created".to_string(),
        serde_yaml::Value::String(meta.created.to_rfc3339()),
    );
    map.insert(
        "updated".to_string(),
        serde_yaml::Value::String(meta.updated.to_rfc3339()),
    );

    // Convert BTreeMap<String, Value> to serde_yaml::Value::Mapping.
    let mut mapping = serde_yaml::Mapping::new();
    for (k, v) in map {
        mapping.insert(serde_yaml::Value::String(k), v);
    }

    (serde_yaml::Value::Mapping(mapping), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\ntitle: Hello\ntype: note\n---\nBody text";
        let (yaml, body) = parse_frontmatter(content).unwrap();
        assert_eq!(yaml["title"].as_str().unwrap(), "Hello");
        assert_eq!(yaml["type"].as_str().unwrap(), "note");
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_parse_frontmatter_empty_body() {
        let content = "---\ntitle: Hello\n---\n";
        let (yaml, body) = parse_frontmatter(content).unwrap();
        assert_eq!(yaml["title"].as_str().unwrap(), "Hello");
        assert_eq!(body, "");
    }

    #[test]
    fn test_parse_frontmatter_no_delimiter() {
        let content = "Just some text";
        assert!(parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_parse_frontmatter_missing_closing() {
        let content = "---\ntitle: Hello\nno closing";
        assert!(parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_serialize_frontmatter_roundtrip() {
        let mut mapping = serde_yaml::Mapping::new();
        mapping.insert(
            serde_yaml::Value::String("title".into()),
            serde_yaml::Value::String("Test".into()),
        );
        let yaml = serde_yaml::Value::Mapping(mapping);
        let output = serialize_frontmatter(&yaml, "Body here");
        assert!(output.starts_with("---\n"));
        assert!(output.contains("title: Test"));
        assert!(output.contains("---\n"));
        assert!(output.ends_with("Body here\n"));
    }

    #[test]
    fn test_generate_slug_basic() {
        assert_eq!(generate_slug("Hello World"), "hello-world");
    }

    #[test]
    fn test_generate_slug_special_chars() {
        assert_eq!(generate_slug("Hello, World!!! How???"), "hello-world-how");
    }

    #[test]
    fn test_generate_slug_long_title() {
        let long = "a".repeat(100);
        let slug = generate_slug(&long);
        assert!(slug.len() <= 60);
    }

    #[test]
    fn test_generate_slug_truncates_on_hyphen_boundary() {
        // Create a title that would produce a slug longer than 60 chars
        // with hyphens that allow a clean break.
        let title = "this-is-a-somewhat-long-title-that-exceeds-sixty-characters-by-a-bit";
        let slug = generate_slug(title);
        assert!(slug.len() <= 60);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn test_resolve_collision_no_conflict() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve_collision(dir.path(), "hello"), "hello");
    }

    #[test]
    fn test_resolve_collision_with_conflict() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.md"), "x").unwrap();
        assert_eq!(resolve_collision(dir.path(), "hello"), "hello-2");
    }

    #[test]
    fn test_resolve_collision_multiple() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.md"), "x").unwrap();
        std::fs::write(dir.path().join("hello-2.md"), "x").unwrap();
        assert_eq!(resolve_collision(dir.path(), "hello"), "hello-3");
    }

    #[test]
    fn test_read_write_note_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");

        let meta = ItemMeta {
            id: "abc-123".to_string(),
            title: "Test Note".to_string(),
            item_type: ItemType::Note,
            tags: vec!["rust".to_string(), "test".to_string()],
            category: Some("engineering".to_string()),
            created: Utc::now(),
            updated: Utc::now(),
        };
        let item = Item::Note(Note {
            meta,
            content: "Hello, world!".to_string(),
        });

        write_item_to_file(&path, &item).unwrap();
        let loaded = read_item_from_file(&path).unwrap();

        let loaded_meta = loaded.meta();
        assert_eq!(loaded_meta.id, "abc-123");
        assert_eq!(loaded_meta.title, "Test Note");
        assert_eq!(loaded_meta.tags, vec!["rust", "test"]);
        assert_eq!(loaded_meta.category.as_deref(), Some("engineering"));

        if let Item::Note(n) = loaded {
            assert_eq!(n.content, "Hello, world!");
        } else {
            panic!("expected Note");
        }
    }

    #[test]
    fn test_read_write_todo_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");

        let meta = ItemMeta {
            id: "todo-1".to_string(),
            title: "Fix bug".to_string(),
            item_type: ItemType::Todo,
            tags: vec!["bug".to_string()],
            category: None,
            created: Utc::now(),
            updated: Utc::now(),
        };
        let item = Item::Todo(Todo {
            meta,
            description: "Fix the login bug".to_string(),
            status: TodoStatus::InProgress,
            priority: Priority::High,
            due: Some(NaiveDate::from_ymd_opt(2026, 4, 20).unwrap()),
        });

        write_item_to_file(&path, &item).unwrap();
        let loaded = read_item_from_file(&path).unwrap();

        if let Item::Todo(t) = loaded {
            assert_eq!(t.meta.id, "todo-1");
            assert_eq!(t.status, TodoStatus::InProgress);
            assert_eq!(t.priority, Priority::High);
            assert_eq!(t.due, Some(NaiveDate::from_ymd_opt(2026, 4, 20).unwrap()));
            assert_eq!(t.description, "Fix the login bug");
        } else {
            panic!("expected Todo");
        }
    }

    #[test]
    fn test_read_write_document_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");

        let meta = ItemMeta {
            id: "doc-1".to_string(),
            title: "Architecture".to_string(),
            item_type: ItemType::Document,
            tags: vec![],
            category: Some("engineering".to_string()),
            created: Utc::now(),
            updated: Utc::now(),
        };
        let item = Item::Document(Document {
            meta,
            content: "# Architecture\n\nSome content here.".to_string(),
            summary: Some("Overview of architecture".to_string()),
        });

        write_item_to_file(&path, &item).unwrap();
        let loaded = read_item_from_file(&path).unwrap();

        if let Item::Document(d) = loaded {
            assert_eq!(d.meta.id, "doc-1");
            assert_eq!(d.summary.as_deref(), Some("Overview of architecture"));
            assert_eq!(d.content, "# Architecture\n\nSome content here.");
        } else {
            panic!("expected Document");
        }
    }
}
