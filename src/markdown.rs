//! Markdown rendering.
//!
//! The document is parsed twice: once to build the outline (heading anchor ids
//! are generated here so the sidebar and the HTML always agree) and once to
//! produce the final HTML. Raw HTML embedded in the Markdown is *escaped*
//! instead of being passed through, which means a document (or an LLM answer)
//! can never inject script tags into the webview.

use std::collections::HashMap;

use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OutlineItem {
    pub level: u8,
    pub text: String,
    pub id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub html: String,
    pub outline: Vec<OutlineItem>,
    pub words: usize,
    pub title: Option<String>,
    pub front_matter: Option<String>,
    pub truncated: bool,
}

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

fn level_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// GitHub-flavoured slug used for heading anchors and deep links.
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else if !out.is_empty() {
            pending_dash = true;
        }
    }
    out
}

/// Splits a YAML front matter block from the document body.
fn split_front_matter(src: &str) -> (Option<String>, String) {
    let trimmed_start = src.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    if !trimmed_start.starts_with("---") {
        return (None, src.to_string());
    }
    let after_open = &trimmed_start[3..];
    if !after_open.starts_with('\n') && !after_open.starts_with("\r\n") {
        return (None, src.to_string());
    }
    let body_start = trimmed_start
        .find('\n')
        .map(|i| i + 1)
        .unwrap_or(trimmed_start.len());
    let mut offset = body_start;
    for line in trimmed_start[body_start..].split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if bare == "---" || bare == "..." {
            let meta = trimmed_start[body_start..offset].to_string();
            let body = trimmed_start[offset + line.len()..].to_string();
            return (Some(meta), body);
        }
        offset += line.len();
    }
    (None, src.to_string())
}

fn front_matter_title(meta: &str) -> Option<String> {
    for line in meta.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("title:") {
            let value = rest.trim().trim_matches(['"', '\'']).trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    opts
}
pub fn render(source: &str) -> Rendered {
    let truncated = source.len() > MAX_INPUT_BYTES;
    let source = if truncated {
        let mut end = MAX_INPUT_BYTES;
        while end > 0 && !source.is_char_boundary(end) {
            end -= 1;
        }
        &source[..end]
    } else {
        source
    };

    let (front_matter, body) = split_front_matter(source);
    let opts = options();

    // Pass 1 - outline + unique anchor ids.
    let mut headings: Vec<OutlineItem> = Vec::new();
    let mut text: Option<(u8, String)> = None;
    for event in Parser::new_ext(&body, opts) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => text = Some((level_num(level), String::new())),
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, value)) = text.take() {
                    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
                    headings.push(OutlineItem {
                        level,
                        text: value,
                        id: String::new(),
                    });
                }
            }
            Event::Text(value) | Event::Code(value) => {
                if let Some((_, acc)) = text.as_mut() {
                    acc.push_str(&value);
                }
            }
            Event::SoftBreak | Event::HardBreak | Event::Rule => {
                if let Some((_, acc)) = text.as_mut() {
                    acc.push(' ');
                }
            }
            _ => {}
        }
    }

    let mut seen: HashMap<String, usize> = HashMap::new();
    for item in headings.iter_mut() {
        let base = slugify(&item.text);
        let base = if base.is_empty() {
            format!("section-{}", seen.len() + 1)
        } else {
            base
        };
        let count = seen.entry(base.clone()).or_insert(0);
        item.id = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
    }

    // Pass 2 - HTML, with anchor ids and raw HTML neutralised.
    let mut cursor = 0usize;
    let mut events: Vec<Event> = Vec::new();
    for event in Parser::new_ext(&body, opts) {
        match event {
            // Untrusted HTML (documents and model output) is shown as text.
            Event::Html(raw) | Event::InlineHtml(raw) => events.push(Event::Text(raw)),
            Event::Start(Tag::Heading {
                level, classes, attrs, ..
            }) => {
                let id = headings
                    .get(cursor)
                    .map(|item| item.id.clone())
                    .unwrap_or_else(|| format!("section-{}", cursor + 1));
                cursor += 1;
                events.push(Event::Start(Tag::Heading {
                    level,
                    id: Some(id.into()),
                    classes,
                    attrs,
                }));
            }
            other => events.push(other),
        }
    }

    let mut html_out = String::with_capacity(body.len() + body.len() / 2);
    html::push_html(&mut html_out, events.into_iter());

    let title = front_matter
        .as_deref()
        .and_then(front_matter_title)
        .or_else(|| {
            headings
                .iter()
                .find(|item| item.level == 1 && !item.text.is_empty())
                .map(|item| item.text.clone())
        });

    Rendered {
        html: html_out,
        outline: headings
            .into_iter()
            .filter(|item| !item.text.is_empty())
            .collect(),
        words: body.split_whitespace().count(),
        title,
        front_matter,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_github_like() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  Multiple   spaces  "), "multiple-spaces");
        assert_eq!(slugify("C++ & Rust"), "c-rust");
        assert_eq!(slugify("日本語 heading"), "日本語-heading");
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn headings_get_unique_anchors() {
        let rendered = render("# Intro\n\n## Intro\n\n### Intro\n");
        let ids: Vec<&str> = rendered.outline.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["intro", "intro-1", "intro-2"]);
        assert!(rendered.html.contains(r#"<h1 id="intro">"#));
        assert!(rendered.html.contains(r#"<h3 id="intro-2">"#));
    }

    #[test]
    fn raw_html_is_escaped() {
        let rendered = render("<script>alert('xss')</script>\n\nHello <b>there</b>");
        assert!(!rendered.html.contains("<script>"));
        assert!(rendered.html.contains("&lt;script&gt;"));
        assert!(rendered.html.contains("&lt;b&gt;"));
    }

    #[test]
    fn gfm_tables_and_tasklists_render() {
        let rendered = render("| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] todo\n");
        assert!(rendered.html.contains("<table>"));
        assert!(rendered.html.contains(r#"type="checkbox""#));
        assert!(rendered.html.contains("checked"));
    }

    #[test]
    fn front_matter_is_extracted() {
        let rendered = render("---\ntitle: \"My Notes\"\nauthor: me\n---\n\n# Body\n");
        assert_eq!(rendered.title.as_deref(), Some("My Notes"));
        assert_eq!(
            rendered.front_matter.as_deref(),
            Some("title: \"My Notes\"\nauthor: me\n")
        );
        assert!(!rendered.html.contains("My Notes"));
        assert!(rendered.html.contains("<h1 id=\"body\">"));
    }

    #[test]
    fn title_falls_back_to_first_h1() {
        let rendered = render("intro\n\n# Real Title\n");
        assert_eq!(rendered.title.as_deref(), Some("Real Title"));
    }

    #[test]
    fn words_are_counted_and_strikethrough_works() {
        let rendered = render("one two three ~~gone~~");
        assert_eq!(rendered.words, 4);
        assert!(rendered.html.contains("<del>"));
    }

    #[test]
    fn oversized_input_is_truncated_on_char_boundary() {
        let source = "é".repeat(MAX_INPUT_BYTES / 2 + 10);
        let rendered = render(&source);
        assert!(rendered.truncated);
        assert!(rendered.html.ends_with("</p>\n"));
    }
}

