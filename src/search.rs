//! Local document retrieval for the assistant's Search mode.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// Breadcrumb of ATX headings above the passage.
    pub heading: String,
    /// 1-based line number where the passage starts.
    pub line: usize,
    pub snippet: String,
    pub score: f64,
}

const MAX_PASSAGE_CHARS: usize = 1_400;
const SNIPPET_CHARS: usize = 340;
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "what", "when", "where", "which",
    "how", "why", "are", "was", "were", "you", "your", "can", "does", "did", "into", "about",
    "have", "has", "not", "but", "all", "any", "its", "a", "an", "of", "to", "in", "on", "is",
];

/// Splits a document into heading-scoped passages and ranks them against the
/// query with a small TF-style score. This runs locally and is deterministic:
/// it is retrieval for the prompt, not a search engine.
pub fn search(markdown: &str, query: &str, limit: usize) -> Vec<SearchHit> {
    let terms = query_terms(query);
    let phrase = quoted_phrase(query);
    if terms.is_empty() && phrase.is_none() {
        return Vec::new();
    }
    let limit = limit.clamp(1, 20);

    let mut hits = Vec::new();
    let mut headings: Vec<String> = Vec::new();
    let mut in_fence = false;
    let mut passage = String::new();
    let mut passage_line = 1usize;
    let mut passage_heading = String::new();

    let flush =
        |passage: &mut String, line: usize, heading: &str, hits: &mut Vec<SearchHit>| {
            let text = passage.trim();
            if text.is_empty() {
                return;
            }
            if let Some(score) = score_text(text, heading, &terms, phrase.as_deref()) {
                if score > 0.0 {
                    hits.push(SearchHit {
                        heading: heading.to_string(),
                        line,
                        snippet: make_snippet(text, &terms, phrase.as_deref()),
                        score,
                    });
                }
            }
            passage.clear();
        };

    for (index, raw_line) in markdown.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            if passage.is_empty() {
                passage_line = line_no;
                passage_heading = headings.join(" > ");
            }
            passage.push_str(raw_line);
            passage.push('\n');
            continue;
        }

        if !in_fence {
            if let Some((level, title)) = atx_heading(raw_line) {
                flush(&mut passage, passage_line, &passage_heading, &mut hits);
                headings.truncate(level.saturating_sub(1));
                headings.push(title);
                passage_line = line_no;
                passage_heading = headings.join(" > ");
                continue;
            }
        }

        if trimmed.is_empty() {
            flush(&mut passage, passage_line, &passage_heading, &mut hits);
            continue;
        }

        if passage.is_empty() {
            passage_line = line_no;
            passage_heading = headings.join(" > ");
        }
        if passage.chars().count() + raw_line.chars().count() > MAX_PASSAGE_CHARS {
            flush(&mut passage, passage_line, &passage_heading, &mut hits);
            passage_line = line_no;
            passage_heading = headings.join(" > ");
        }
        passage.push_str(raw_line);
        passage.push('\n');
    }
    flush(&mut passage, passage_line, &passage_heading, &mut hits);

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.line.cmp(&b.line))
    });
    hits.truncate(limit);
    hits
}

fn atx_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let title = rest.trim().trim_end_matches('#').trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some((hashes, title))
    }
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = query
        .split(|ch: char| !ch.is_alphanumeric())
        .map(|token| token.to_ascii_lowercase())
        .filter(|token| token.len() >= 2 && !STOP_WORDS.contains(&token.as_str()))
        .collect();
    terms.sort();
    terms.dedup();
    terms
}

fn quoted_phrase(query: &str) -> Option<String> {
    let start = query.find('"')?;
    let rest = &query[start + 1..];
    let end = rest.find('"')?;
    let phrase = rest[..end].trim().to_ascii_lowercase();
    if phrase.is_empty() {
        None
    } else {
        Some(phrase)
    }
}

fn score_text(text: &str, heading: &str, terms: &[String], phrase: Option<&str>) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    let heading_lower = heading.to_ascii_lowercase();
    let mut score = 0.0;
    let mut matched = false;

    for term in terms {
        let body_count = lower.matches(term.as_str()).count();
        let heading_count = heading_lower.matches(term.as_str()).count();
        if body_count + heading_count > 0 {
            matched = true;
            score += body_count as f64 * (1.0 + term.len() as f64 / 12.0);
            score += heading_count as f64 * 3.0;
        }
    }
    if let Some(phrase) = phrase {
        if lower.contains(phrase) || heading_lower.contains(phrase) {
            matched = true;
            score += 4.0;
        }
    }
    if !matched {
        return None;
    }
    let words = text.split_whitespace().count().max(1) as f64;
    Some(score / words.sqrt())
}

fn make_snippet(text: &str, terms: &[String], phrase: Option<&str>) -> String {
    let lower = text.to_ascii_lowercase();
    let mut index = phrase.and_then(|p| lower.find(p)).unwrap_or(usize::MAX);
    for term in terms {
        if let Some(found) = lower.find(term.as_str()) {
            index = index.min(found);
        }
    }
    if index == usize::MAX {
        index = 0;
    }

    let chars: Vec<char> = text.chars().collect();
    let char_index = lower[..index.min(lower.len())].chars().count();
    let start = char_index.saturating_sub(SNIPPET_CHARS / 3);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let mut snippet: String = chars[start..end].iter().collect();
    snippet = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < chars.len() {
        snippet.push('…');
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Installation\n\nRun the installer and follow the wizard.\n\n## Configuration\n\nThe API key is stored in the credential store. Configure the token in Settings.\n\n# Troubleshooting\n\nIf the window does not open, check WebView2.\n";

    #[test]
    fn finds_passages_with_heading_context() {
        let hits = search(DOC, "API key credential", 5);
        assert!(!hits.is_empty());
        assert!(hits[0].heading.contains("Configuration"));
        assert!(hits[0].snippet.to_lowercase().contains("key"));
        assert_eq!(hits[0].line, 7);
    }

    #[test]
    fn respects_limit_and_scores() {
        let hits = search(DOC, "installer window", 1);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn ignores_common_words_and_empty_queries() {
        assert!(search(DOC, "the and of", 5).is_empty());
        assert!(search(DOC, "", 5).is_empty());
    }

    #[test]
    fn supports_quoted_phrases() {
        let hits = search(DOC, "\"credential store\"", 5);
        assert!(!hits.is_empty());
        assert!(hits[0].snippet.contains("credential store"));
    }

    #[test]
    fn fences_do_not_create_headings() {
        let doc = "# Code\n\n```\n# not a heading\ninstaller token\n```\n";
        let hits = search(doc, "token", 5);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].heading, "Code");
    }
}

