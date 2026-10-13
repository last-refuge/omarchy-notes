//! Inert Markdown conversion. Asset resolution/file interchange is a separate
//! workflow: this adapter exports visible labels and reports that limitation.
use crate::{MAX_HTML, html, safe_link};
use anyhow::{Result, ensure};
use notes_core::document::{Document, Span};

pub struct Exported {
    pub text: String,
    pub warnings: Vec<String>,
}

pub fn import(text: &str) -> Result<html::Imported> {
    ensure!(
        text.len() <= MAX_HTML,
        "Markdown exceeds the 4 MiB import limit."
    );
    let parser = pulldown_cmark::Parser::new_ext(
        text,
        pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_TASKLISTS,
    );
    let mut generated = String::new();
    pulldown_cmark::html::push_html(&mut generated, parser);
    html::import(&generated)
}

pub fn export(document: &Document) -> Result<Exported> {
    document.validate()?;
    let mut out = Exported {
        text: String::new(),
        warnings: vec![],
    };
    let mut ordered = [0usize; 17];
    for block in &document.blocks {
        if block.kind == "table" {
            out.text
                .push_str(&html::export(&Document::from_blocks(vec![block.clone()])));
            out.text.push_str("\n\n");
            continue;
        }
        let inline = block
            .spans
            .iter()
            .map(|s| inline(s, &mut out.warnings))
            .collect::<String>();
        let prefix = match block.kind.as_str() {
            "quote" => "> ".to_string(),
            "heading" => format!("{} ", "#".repeat(block.number("level", 1))),
            "list" => {
                let indent = block.number("indent", 0).min(16);
                ordered[indent] += 1;
                ordered[indent + 1..].fill(0);
                let marker = match block.string("list") {
                    "ordered" => format!("{}. ", ordered[indent]),
                    "check" => {
                        if block.checked() {
                            "- [x] ".into()
                        } else {
                            "- [ ] ".into()
                        }
                    }
                    _ => "- ".into(),
                };
                format!("{}{marker}", "    ".repeat(indent))
            }
            _ => String::new(),
        };
        if block.kind != "list" {
            ordered.fill(0);
        }
        out.text.push_str(&prefix);
        out.text.push_str(&inline);
        out.text.push_str("\n\n");
    }
    if !document.references("image").is_empty() || !document.references("attachment").is_empty() {
        out.warnings.push("This text export includes media labels, not their files. Keep the original library or a full backup.".into());
    }
    ensure!(
        out.text.len() <= MAX_HTML,
        "The Markdown export exceeds 4 MiB."
    );
    Ok(out)
}
fn literal(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c == '\n' {
                "<br>".into()
            } else if c.is_ascii_punctuation() {
                format!("&#{};", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}
fn attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn inline(span: &Span, warnings: &mut Vec<String>) -> String {
    if !span.is_text() {
        return literal(
            span.0
                .get("alt")
                .and_then(|v| v.as_str())
                .unwrap_or("[Attached file]"),
        );
    }
    let mut text = literal(span.content());
    // Inline HTML expresses all six marks without ambiguous delimiter nesting.
    for (mark, tag) in [
        ("bold", "strong"),
        ("italic", "em"),
        ("underline", "u"),
        ("strikethrough", "s"),
        ("highlight", "mark"),
        ("code", "code"),
    ] {
        if span.mark(mark) {
            text = format!("<{tag}>{text}</{tag}>");
        }
    }
    if let Some(href) = span.0.get("href").and_then(|v| v.as_str()) {
        if safe_link(href) {
            text = format!("<a href=\"{}\">{text}</a>", attribute(href));
        } else if !warnings
            .iter()
            .any(|w| w == "An unsupported link was exported as text.")
        {
            warnings.push("An unsupported link was exported as text.".into());
        }
    }
    text
}
