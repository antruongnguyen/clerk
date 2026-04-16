/// Split content into chunks that each fit within `max_len` characters.
///
/// Splitting priority:
/// 1. Paragraph boundaries (`\n\n`)
/// 2. Line boundaries (`\n`)
/// 3. Character boundary (last resort)
///
/// Each returned chunk is guaranteed to be ≤ `max_len` characters.
pub fn split_content(content: &str, max_len: usize) -> Vec<String> {
    if max_len == 0 || content.len() <= max_len {
        return vec![content.to_string()];
    }

    let paragraphs: Vec<&str> = content.split("\n\n").collect();
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in paragraphs {
        // If this single paragraph already exceeds the limit, split it further.
        if para.len() > max_len {
            // Flush what we have so far.
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
            }
            // Split the oversized paragraph by lines.
            let mut sub_chunks = split_by_lines(para, max_len);
            chunks.append(&mut sub_chunks);
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
            // Adding this paragraph would exceed the limit. Flush and start new chunk.
            if !current.is_empty() {
                chunks.push(current);
            }
            current = para.to_string();
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Split a single block of text by line boundaries.
fn split_by_lines(text: &str, max_len: usize) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in lines {
        // If a single line exceeds the limit, split it by characters.
        if line.len() > max_len {
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
            }
            let mut sub_chunks = split_by_chars(line, max_len);
            chunks.append(&mut sub_chunks);
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
        // Ensure we don't split in the middle of a multi-byte UTF-8 character.
        let end = if end < text.len() {
            // Walk back to a char boundary.
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
        let content = "Short content.";
        let result = split_content(content, 100);
        assert_eq!(result, vec!["Short content."]);
    }

    #[test]
    fn test_split_on_paragraphs() {
        let content = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
        // Each paragraph is ~15 chars. Limit of 35 should fit 2 paragraphs per chunk.
        let result = split_content(content, 35);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "Paragraph one.\n\nParagraph two.");
        assert_eq!(result[1], "Paragraph three.");
    }

    #[test]
    fn test_split_on_lines_fallback() {
        // One "paragraph" with multiple lines, no \n\n.
        let content = "Line one\nLine two\nLine three\nLine four";
        let result = split_content(content, 20);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "Line one\nLine two");
        assert_eq!(result[1], "Line three\nLine four");
    }

    #[test]
    fn test_split_on_chars_fallback() {
        let content = "a".repeat(25);
        let result = split_content(&content, 10);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].len(), 10);
        assert_eq!(result[1].len(), 10);
        assert_eq!(result[2].len(), 5);
    }

    #[test]
    fn test_empty_content() {
        let result = split_content("", 100);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_exact_limit() {
        let content = "a".repeat(100);
        let result = split_content(&content, 100);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_preserves_all_content() {
        let content = "Para one with some text.\n\nPara two also has text.\n\nPara three final.";
        let result = split_content(content, 30);
        // Reconstruct from chunks and verify no data loss.
        let reconstructed = result.join("\n\n");
        assert_eq!(reconstructed, content);
    }

    #[test]
    fn test_zero_max_len() {
        let content = "Some content";
        let result = split_content(content, 0);
        assert_eq!(result, vec!["Some content"]);
    }

    #[test]
    fn test_mixed_paragraph_sizes() {
        let short = "Short.";
        let long = "a".repeat(80);
        let content = format!("{short}\n\n{long}\n\n{short}");
        // short(6)+\n\n(2)+long(80)=88 fits in 90.
        // 88+\n\n(2)+short(6)=96 exceeds 90, so last short becomes a new chunk.
        let result = split_content(&content, 90);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], format!("{short}\n\n{long}"));
        assert_eq!(result[1], short);
    }
}
