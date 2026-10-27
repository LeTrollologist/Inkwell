---
title: Field Notes — Inkwell
author: Inkwell
---

# Reading, without the noise

A calm place for long Markdown. Inkwell renders everything locally in Rust and keeps
the assistant one keystroke away.

> The best reading interface is the one that disappears while you read.

## Why a desktop reader?

Browsers are built for tabs, banners and notifications. A reader should be built for
**one thing at a time**:

- a comfortable measure and real typographic hierarchy
- an outline you can trust
- code that looks like code
- a chat panel that can see what you're reading

### The outline stays in sync

Scroll the page and the sidebar follows. Click any heading and the reader jumps there.

## A table of small delights

| Detail | Behaviour |
| --- | --- |
| Themes | Dark and light, remembered locally |
| Progress | Thin gradient bar at the top of the page |
| Source | `Ctrl+U` reveals the raw Markdown |
| Printing | `Ctrl+P`, with a clean print stylesheet |

## Code, highlighted locally

```rust
use pulldown_cmark::{html, Options, Parser};

pub fn render(source: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(source, options);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}
```

## Today

- [x] Open a Markdown file
- [x] Read it comfortably
- [ ] Ask a question about it
- [ ] Send the answer to a colleague

## Notes on privacy

Your API key lives in your user profile and never enters the webview. Documents are only
sent to the provider you pick, and only when **Document** is enabled in the composer.

---

*Rendered by `pulldown-cmark`. Styled with a little restraint.*
