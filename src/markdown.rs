//! Markdown rendering.
//!
//! The document is parsed twice: once to build the outline (heading anchor ids
//! are generated here so the sidebar and the HTML always agree) and once to
//! produce the final HTML. Raw HTML embedded in the Markdown is *escaped*
//! instead of being passed through, which means a document (or an LLM answer)
//! can never inject script tags into the webview.

use std::collections::HashMap;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use percent_encoding::percent_decode_str;
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
/// Largest single local image embedded into the rendered HTML.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
/// Total raw image bytes embedded per render (base64 grows this by ~33%).
const MAX_EMBEDDED_IMAGE_BYTES: usize = 24 * 1024 * 1024;
/// Inert 1x1 transparent GIF used when a document points at an unsafe image.
const BLOCKED_IMAGE: &str =
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";

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

/// Extracts a URL scheme while ignoring ASCII control/whitespace characters
/// inside it, mirroring how browsers normalise `java\nscript:`.
fn scheme_of(url: &str) -> Option<String> {
    let decoded = percent_decode_str(url).decode_utf8_lossy();
    let mut scheme = String::new();
    for ch in decoded.chars() {
        if ch == ':' {
            return if scheme.is_empty() { None } else { Some(scheme) };
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.') {
            scheme.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_whitespace() || ch.is_ascii_control() {
            continue;
        } else {
            return None;
        }
    }
    None
}

/// Only web/mail links are kept; `javascript:`, `data:`, `file:` and every
/// other scheme become an inert fragment. Relative links are left alone for
/// future in-document navigation.
fn sanitize_link(dest: &str) -> String {
    let url = dest.trim();
    if url.is_empty()
        || url.chars().any(|ch| ch == '\0' || (ch.is_control() && ch != '\t'))
    {
        return "#".to_string();
    }
    match scheme_of(url).as_deref() {
        None | Some("http") | Some("https") | Some("mailto") => url.to_string(),
        Some(_) => "#".to_string(),
    }
}

fn is_safe_data_image(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
        "data:image/gif;base64,",
        "data:image/webp;base64,",
        "data:image/avif;base64,",
        "data:image/bmp;base64,",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn image_mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

/// Reads a relative image beneath `base` and turns it into a data URL. The
/// canonical path must stay inside the document's folder, so `../` cannot be
/// used to pull arbitrary files into the page.
fn embed_relative_image(dest: &str, base: &Path, total: &mut usize) -> Option<String> {
    let without_suffix = dest.split(['?', '#']).next().unwrap_or(dest);
    let decoded = percent_decode_str(without_suffix).decode_utf8().ok()?;
    if decoded.is_empty()
        || decoded.starts_with('/')
        || decoded.starts_with('\\')
        || decoded.contains('\0')
    {
        return None;
    }

    let base = base.canonicalize().ok()?;
    let candidate = base.join(decoded.as_ref()).canonicalize().ok()?;
    if !candidate.starts_with(&base) || !candidate.is_file() {
        return None;
    }
    let mime = image_mime(&candidate)?;
    let size = std::fs::metadata(&candidate).ok()?.len();
    if size == 0 || size > MAX_IMAGE_BYTES {
        return None;
    }
    let size = size as usize;
    if total.saturating_add(size) > MAX_EMBEDDED_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&candidate).ok()?;
    *total += bytes.len();
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

fn sanitize_image(dest: &str, base: Option<&Path>, total: &mut usize) -> String {
    let url = dest.trim();
    if url.is_empty() || url.contains('\0') {
        return BLOCKED_IMAGE.to_string();
    }

    if let Some(base) = base {
        if scheme_of(url).is_none() && !url.starts_with('/') && !url.starts_with('\\') {
            if let Some(data) = embed_relative_image(url, base, total) {
                return data;
            }
        }
    }

    match scheme_of(url).as_deref() {
        None | Some("http") | Some("https") => url.to_string(),
        Some("data") if is_safe_data_image(url) => url.to_string(),
        _ => BLOCKED_IMAGE.to_string(),
    }
}
#[cfg(test)]
pub fn render(source: &str) -> Rendered {
    render_with(source, None)
}

/// Renders Markdown. When `base_dir` is supplied (a locally opened document),
/// relative image paths inside that folder are embedded as data URLs.
pub fn render_with(source: &str, base_dir: Option<&Path>) -> Rendered {
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

    // Pass 2 - HTML, with anchor ids, raw HTML neutralised and URLs sanitised.
    let mut cursor = 0usize;
    let mut embedded_total = 0usize;
    let mut events: Vec<Event> = Vec::new();
    for event in Parser::new_ext(&body, opts) {
        match event {
            // Untrusted HTML (documents and model output) is shown as text.
            Event::Html(raw) | Event::InlineHtml(raw) => events.push(Event::Text(raw)),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                events.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: sanitize_link(&dest_url).into(),
                    title,
                    id,
                }));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                events.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: sanitize_image(&dest_url, base_dir, &mut embedded_total).into(),
                    title,
                    id,
                }));
            }
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

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inkwell-md-{}-{tag}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unsafe_url_schemes_are_neutralised() {
        let payloads = [
            "[x](javascript:alert(1))",
            "[x](JaVaScRiPt:alert(1))",
            "[x](java%09script:alert(1))",
            "[x](%6aavascript:alert(1))",
            "[x](&#106;avascript:alert(1))",
            "[x](&#x6a;avascript:alert(1))",
            "[x](java&#x0A;script:alert(1))",
            "[x](data:text/html,<script>alert(1)</script>)",
            "[x](file:///etc/passwd)",
            "[x](vbscript:msgbox(1))",
        ];
        for payload in payloads {
            let rendered = render(payload);
            let lower = rendered.html.to_ascii_lowercase();
            assert!(!lower.contains("javascript:"), "{payload}");
            assert!(!lower.contains("vbscript:"), "{payload}");
            assert!(!lower.contains("data:text/html"), "{payload}");
            assert!(!lower.contains("file:///"), "{payload}");
            assert!(
                rendered.html.contains("href=\"#\""),
                "{payload} => {}",
                rendered.html
            );
        }
    }

    #[test]
    fn whitespace_obfuscated_schemes_are_not_links() {
        // CommonMark treats whitespace in an unquoted destination as the end
        // of the link, so this stays literal text rather than a URL.
        let rendered = render("[x](java\tscript:alert(1))");
        assert!(!rendered.html.to_ascii_lowercase().contains("javascript:"));
        assert!(!rendered.html.contains("<a "));
    }

    #[test]
    fn safe_links_survive() {
        let rendered = render(
            "[web](https://example.com/a?b=1) [mail](mailto:a@b.c) \
             [rel](docs/a.md) [frag](#intro)",
        );
        assert!(rendered.html.contains("href=\"https://example.com/a?b=1\""));
        assert!(rendered.html.contains("href=\"mailto:a@b.c\""));
        assert!(rendered.html.contains("href=\"docs/a.md\""));
        assert!(rendered.html.contains("href=\"#intro\""));
    }

    #[test]
    fn unsafe_images_use_the_inert_placeholder() {
        let rendered = render(
            "![bad](javascript:alert(1)) \
             ![svg](data:image/svg+xml;base64,PHN2Zz48c2NyaXB0Pjwvc2NyaXB0Pjwvc3ZnPg==) \
             ![entity](&#x64;ata:image/svg+xml;base64,PHN2Zz48c2NyaXB0Pjwvc2NyaXB0Pjwvc3ZnPg==)",
        );
        let lower = rendered.html.to_ascii_lowercase();
        assert!(!lower.contains("javascript:"));
        assert!(!lower.contains("svg+xml"));
        assert!(rendered.html.contains(BLOCKED_IMAGE));
    }

    #[test]
    fn raster_data_images_and_https_images_survive() {
        let rendered = render(
            "![ok](data:image/png;base64,iVBORw0KGgo=) \
             ![web](https://example.com/pic.png)",
        );
        assert!(rendered
            .html
            .contains("src=\"data:image/png;base64,iVBORw0KGgo=\""));
        assert!(rendered.html.contains("src=\"https://example.com/pic.png\""));
    }

    #[test]
    fn relative_images_are_embedded_from_the_document_folder() {
        let dir = temp_dir("embed");
        std::fs::write(dir.join("pic.png"), b"not-a-real-png").unwrap();
        let rendered = render_with("![alt](pic.png \"title\")", Some(&dir));
        assert!(rendered.html.contains("src=\"data:image/png;base64,"));
        assert!(rendered.html.contains("alt=\"alt\""));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relative_images_cannot_escape_the_document_folder() {
        let base = temp_dir("escape");
        let sub = base.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(base.join("secret.png"), b"secret").unwrap();
        let rendered = render_with("![x](../secret.png)", Some(&sub));
        assert!(!rendered.html.contains("data:image/png;base64,"));
        assert!(!rendered.html.contains("c2VjcmV0"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_and_non_image_assets_are_not_embedded() {
        let dir = temp_dir("missing");
        std::fs::write(dir.join("notes.txt"), b"hello").unwrap();
        let rendered = render_with("![a](missing.png) ![b](notes.txt)", Some(&dir));
        assert!(!rendered.html.contains("data:image/"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn html_injection_variants_are_escaped() {
        let payloads = [
            "<img src=x onerror=alert(1)>",
            "<svg onload=alert(1)>",
            "<iframe src=javascript:alert(1)>",
            "</script><script>alert(1)</script>",
            "<!--><script>alert(1)</script>-->",
            "<math><mtext><script>alert(1)</script></mtext></math>",
        ];
        for payload in payloads {
            let rendered = render(payload);
            let lower = rendered.html.to_ascii_lowercase();
            assert!(!lower.contains("<script"), "{payload}");
            assert!(!lower.contains("<iframe"), "{payload}");
            assert!(!lower.contains("<svg"), "{payload}");
            assert!(!lower.contains("<img"), "{payload}");
            assert!(!lower.contains("<math"), "{payload}");
        }
    }

    #[test]
    fn llm_output_is_escaped_and_sanitised() {
        let answer = "Use this:\n\n```html\n<script>alert('x')</script>\n```\n\n\
                      [click](javascript:alert(1))\n\n\
                      <img src=x onerror=alert(2)>";
        let rendered = render(answer);
        let lower = rendered.html.to_ascii_lowercase();
        assert!(!lower.contains("<script>alert"));
        assert!(!lower.contains("javascript:"));
        assert!(rendered.html.contains("&lt;script&gt;"));
        assert!(rendered.html.contains("&lt;img"));
    }

    #[test]
    fn malformed_and_hostile_documents_do_not_panic() {
        let deep = format!("{}text", "> ".repeat(5_000));
        let nested = format!("{}item", "- ".repeat(5_000));
        let samples = [
            "<",
            "<div",
            "```",
            "![",
            "[",
            "|a|b\n|-",
            "\0\u{1b}[31m",
            "<a href=\"javascript:alert(1)\"",
            deep.as_str(),
            nested.as_str(),
        ];
        for sample in samples {
            let _ = render(sample);
        }
    }

    #[test]
    fn front_matter_titles_are_plain_text() {
        let rendered =
            render("---\ntitle: \"<img src=x onerror=alert(1)>\"\n---\n\n# Body\n");
        assert_eq!(
            rendered.title.as_deref(),
            Some("<img src=x onerror=alert(1)>")
        );
        assert!(!rendered.html.contains("<img"));
        assert!(!rendered.html.contains("&lt;script"));
    }

}

