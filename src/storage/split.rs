/// Pre-scanned heading entry with level, text, and byte offset.
#[derive(Debug, Clone)]
struct HeadingEntry {
    /// Heading level (1 for `#`, 2 for `##`, etc.)
    level: usize,
    /// Heading text without `#` prefix.
    text: String,
    /// Byte offset of the heading line in the original content.
    byte_offset: usize,
}

/// Pre-scanned document outline built from a first pass over the content.
pub struct DocumentOutline {
    /// All headings found, in order.
    headings: Vec<HeadingEntry>,
    /// Table of contents as a formatted string.
    pub toc: String,
}

/// A chunk of split content with metadata from the pre-scan.
pub struct ContentChunk {
    /// The text content of this chunk.
    pub content: String,
    /// Breadcrumb path of headings for this chunk (e.g. "Getting Started > Initial Setup").
    pub heading: Option<String>,
}

/// Pre-scan content to build an outline of all headings.
///
/// This is a single O(n) pass that collects heading positions and builds a
/// table of contents. The outline is then used by `split_content` to produce
/// better metadata per chunk.
pub fn prescan_outline(content: &str) -> DocumentOutline {
    let mut headings = Vec::new();
    let mut byte_offset = 0;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count();
            let text = trimmed[level..].trim().to_string();
            if !text.is_empty() && level <= 6 {
                headings.push(HeadingEntry {
                    level,
                    text,
                    byte_offset,
                });
            }
        }
        byte_offset += line.len() + 1; // +1 for \n
    }

    let toc = build_toc(&headings);

    DocumentOutline { headings, toc }
}

/// Build a table of contents string from headings.
fn build_toc(headings: &[HeadingEntry]) -> String {
    if headings.is_empty() {
        return String::new();
    }

    let mut lines = Vec::with_capacity(headings.len());
    for h in headings {
        let indent = "  ".repeat(h.level.saturating_sub(1));
        lines.push(format!("{indent}- {}", h.text));
    }
    lines.join("\n")
}

/// Find the breadcrumb path for a byte offset using the heading hierarchy.
///
/// Walks the heading list to find the active heading stack at the given offset.
/// Returns a path like "Getting Started > Initial Setup > Node.js".
fn breadcrumb_at(headings: &[HeadingEntry], byte_offset: usize) -> Option<String> {
    // Collect all headings that appear at or before this offset.
    let mut stack: Vec<&HeadingEntry> = Vec::new();

    for h in headings {
        if h.byte_offset > byte_offset {
            break;
        }
        // Pop any headings at the same level or deeper.
        while stack.last().is_some_and(|top| top.level >= h.level) {
            stack.pop();
        }
        stack.push(h);
    }

    if stack.is_empty() {
        return None;
    }

    Some(
        stack
            .iter()
            .map(|h| h.text.as_str())
            .collect::<Vec<_>>()
            .join(" > "),
    )
}

/// Convenience wrapper: pre-scan + split in one call.
#[cfg(test)]
pub fn split_content(content: &str, max_len: usize) -> Vec<ContentChunk> {
    let outline = prescan_outline(content);
    split_content_with_outline(content, max_len, &outline)
}

/// Split content using a pre-built outline for heading metadata.
pub fn split_content_with_outline(
    content: &str,
    max_len: usize,
    outline: &DocumentOutline,
) -> Vec<ContentChunk> {
    if max_len == 0 || content.len() <= max_len {
        return vec![ContentChunk {
            heading: breadcrumb_at(&outline.headings, 0),
            content: content.to_string(),
        }];
    }

    // Split into sections by heading boundaries.
    let sections = split_by_headings(content);

    // Track byte offset for breadcrumb lookups.
    let mut byte_offset: usize = 0;
    let mut chunks: Vec<ContentChunk> = Vec::new();
    let mut current = String::new();
    let mut current_offset: usize = 0;

    for section in &sections {
        // If a single section exceeds the limit, split it further by paragraphs.
        if section.len() > max_len {
            if !current.is_empty() {
                chunks.push(ContentChunk {
                    heading: breadcrumb_at(&outline.headings, current_offset),
                    content: current,
                });
                current = String::new();
            }
            let sub_chunks = split_by_paragraphs(section, max_len);
            // Each sub-chunk inherits the same section's byte offset for breadcrumb.
            for sc in sub_chunks {
                chunks.push(ContentChunk {
                    heading: breadcrumb_at(&outline.headings, byte_offset)
                        .or(sc.heading),
                    content: sc.content,
                });
            }
            byte_offset += section.len() + 2; // +2 for \n\n between sections
            continue;
        }

        let separator = if current.is_empty() { "" } else { "\n\n" };
        let new_len = current.len() + separator.len() + section.len();

        if new_len <= max_len {
            if current.is_empty() {
                current_offset = byte_offset;
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(section);
        } else {
            if !current.is_empty() {
                chunks.push(ContentChunk {
                    heading: breadcrumb_at(&outline.headings, current_offset),
                    content: current,
                });
            }
            current = section.to_string();
            current_offset = byte_offset;
        }

        byte_offset += section.len() + 2;
    }

    if !current.is_empty() {
        chunks.push(ContentChunk {
            heading: breadcrumb_at(&outline.headings, current_offset),
            content: current,
        });
    }

    chunks
}

/// Extract the first markdown heading text (without the `#` prefix) from content.
pub fn extract_first_heading(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let text = trimmed.trim_start_matches('#').trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Split content into sections at heading boundaries.
fn split_by_headings(content: &str) -> Vec<String> {
    let mut sections: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        let is_heading = line.trim_start().starts_with('#')
            && line
                .trim_start()
                .chars()
                .nth(1)
                .is_some_and(|c| c == '#' || c == ' ');

        if is_heading && !current.is_empty() {
            sections.push(current.trim_end().to_string());
            current = String::new();
        }

        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.is_empty() {
        sections.push(current.trim_end().to_string());
    }

    sections
}

/// Split text by paragraph boundaries, returning ContentChunks with extracted headings.
fn split_by_paragraphs(text: &str, max_len: usize) -> Vec<ContentChunk> {
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks: Vec<ContentChunk> = Vec::new();
    let mut current = String::new();

    for para in paragraphs {
        if para.len() > max_len {
            if !current.is_empty() {
                chunks.push(ContentChunk {
                    heading: extract_first_heading(&current),
                    content: current,
                });
                current = String::new();
            }
            let sub = split_by_lines(para, max_len);
            for s in sub {
                chunks.push(ContentChunk {
                    heading: extract_first_heading(&s),
                    content: s,
                });
            }
            continue;
        }

        let separator = if current.is_empty() { "" } else { "\n\n" };
        let new_len = current.len() + separator.len() + para.len();

        if new_len <= max_len {
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(para);
        } else {
            if !current.is_empty() {
                chunks.push(ContentChunk {
                    heading: extract_first_heading(&current),
                    content: current,
                });
            }
            current = para.to_string();
        }
    }

    if !current.is_empty() {
        chunks.push(ContentChunk {
            heading: extract_first_heading(&current),
            content: current,
        });
    }

    chunks
}

/// Split text by line boundaries.
fn split_by_lines(text: &str, max_len: usize) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in lines {
        if line.len() > max_len {
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
            }
            let mut sub = split_by_chars(line, max_len);
            chunks.append(&mut sub);
            continue;
        }

        let separator = if current.is_empty() { "" } else { "\n" };
        let new_len = current.len() + separator.len() + line.len();

        if new_len <= max_len {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        } else {
            if !current.is_empty() {
                chunks.push(current);
            }
            current = line.to_string();
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Split text at character boundaries as a last resort.
fn split_by_chars(text: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = std::cmp::min(start + max_len, text.len());
        let end = if end < text.len() {
            let mut e = end;
            while !text.is_char_boundary(e) {
                e -= 1;
            }
            e
        } else {
            end
        };
        chunks.push(text[start..end].to_string());
        start = end;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_split_needed() {
        let result = split_content("Short content.", 100);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "Short content.");
    }

    #[test]
    fn test_split_on_headings() {
        let content = "# Section One\n\nContent one.\n\n# Section Two\n\nContent two.";
        let result = split_content(content, 30);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].heading.as_deref(), Some("Section One"));
        assert_eq!(result[1].heading.as_deref(), Some("Section Two"));
    }

    #[test]
    fn test_breadcrumb_hierarchy() {
        let content = "# Top\n\nIntro.\n\n## Sub A\n\nContent A.\n\n## Sub B\n\nContent B.";
        let result = split_content(content, 30);
        // Sub sections should show breadcrumbs like "Top > Sub A"
        let has_breadcrumb = result.iter().any(|c| {
            c.heading
                .as_deref()
                .is_some_and(|h| h.contains(" > "))
        });
        assert!(has_breadcrumb, "Expected breadcrumb paths in headings: {:?}",
            result.iter().map(|c| c.heading.as_deref()).collect::<Vec<_>>());
    }

    #[test]
    fn test_prescan_outline_toc() {
        let content = "# A\n\nBody.\n\n## B\n\nBody.\n\n# C\n\nBody.";
        let outline = prescan_outline(content);
        assert!(outline.toc.contains("- A"));
        assert!(outline.toc.contains("  - B"));
        assert!(outline.toc.contains("- C"));
    }

    #[test]
    fn test_headings_combined_when_small() {
        let content = "# A\n\nSmall.\n\n# B\n\nTiny.";
        let result = split_content(content, 200);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, content);
    }

    #[test]
    fn test_split_on_paragraphs_fallback() {
        let content = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
        let result = split_content(content, 35);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "Paragraph one.\n\nParagraph two.");
        assert_eq!(result[1].content, "Paragraph three.");
    }

    #[test]
    fn test_split_on_chars_fallback() {
        let content = "a".repeat(25);
        let result = split_content(&content, 10);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].content.len(), 10);
    }

    #[test]
    fn test_empty_content() {
        let result = split_content("", 100);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "");
    }

    #[test]
    fn test_extract_heading() {
        assert_eq!(
            extract_first_heading("# Hello World\n\nBody"),
            Some("Hello World".to_string())
        );
        assert_eq!(extract_first_heading("No heading"), None);
    }

    #[test]
    fn test_preserves_all_content() {
        let content = "# A\n\nPara one.\n\n# B\n\nPara two.\n\n# C\n\nPara three.";
        let result = split_content(content, 25);
        let reconstructed = result
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert_eq!(reconstructed, content);
    }

    #[test]
    fn test_breadcrumb_at() {
        let headings = vec![
            HeadingEntry { level: 1, text: "Top".into(), byte_offset: 0 },
            HeadingEntry { level: 2, text: "Sub".into(), byte_offset: 20 },
            HeadingEntry { level: 1, text: "Next".into(), byte_offset: 50 },
        ];
        assert_eq!(breadcrumb_at(&headings, 0), Some("Top".into()));
        assert_eq!(breadcrumb_at(&headings, 25), Some("Top > Sub".into()));
        assert_eq!(breadcrumb_at(&headings, 55), Some("Next".into()));
    }
}
