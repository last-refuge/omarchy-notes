//! HTML is converted into allowed document structures; it is never rendered as HTML.
use crate::{MAX_HTML, safe_link};
use anyhow::{Result, ensure};
use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
    },
};
use notes_core::document::{Block, Cell, Document, Span, new_id};
use serde_json::json;
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Default)]
struct Node {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<usize>,
}
struct Tree {
    nodes: Vec<Node>,
    stack: Vec<usize>,
    overflow: bool,
}
struct Sink(RefCell<Tree>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut tree = self.0.borrow_mut();
        if tree.nodes.len() >= 20000 || tree.stack.len() > 64 {
            tree.overflow = true;
            return TokenSinkResult::Continue;
        }
        let mut push = |node: Node, container: bool| {
            let index = tree.nodes.len();
            let parent = *tree.stack.last().unwrap();
            tree.nodes.push(node);
            tree.nodes[parent].children.push(index);
            if container {
                tree.stack.push(index);
            }
        };
        match token {
            Token::CharacterTokens(text) => push(
                Node {
                    text: text.to_string(),
                    ..Default::default()
                },
                false,
            ),
            Token::TagToken(tag) if tag.kind == TagKind::StartTag => {
                let name = tag.name.to_string();
                // Common omitted end tags; this is an inert import tree, not a browser DOM.
                if matches!(name.as_str(), "p" | "li" | "tr" | "td" | "th")
                    && tree.nodes[*tree.stack.last().unwrap()].name == name
                {
                    tree.stack.pop();
                }
                let parent = *tree.stack.last().unwrap();
                let index = tree.nodes.len();
                let container = !tag.self_closing
                    && !matches!(
                        name.as_str(),
                        "br" | "img" | "hr" | "meta" | "link" | "input" | "wbr" | "source"
                    );
                tree.nodes.push(Node {
                    name,
                    attrs: tag
                        .attrs
                        .into_iter()
                        .map(|a| (a.name.local.to_string(), a.value.to_string()))
                        .collect(),
                    ..Default::default()
                });
                tree.nodes[parent].children.push(index);
                if container {
                    tree.stack.push(index);
                }
            }
            Token::TagToken(tag) => {
                if let Some(index) = tree
                    .stack
                    .iter()
                    .rposition(|i| tree.nodes[*i].name == tag.name.as_ref())
                {
                    tree.stack.truncate(index.max(1));
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
pub struct Imported {
    pub document: Document,
    pub warnings: Vec<String>,
}
pub fn import(text: &str) -> Result<Imported> {
    ensure!(
        text.len() <= MAX_HTML,
        "HTML exceeds the 4 MiB paste limit."
    );
    let sink = Sink(RefCell::new(Tree {
        nodes: vec![Node::default()],
        stack: vec![0],
        overflow: false,
    }));
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(text));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let tree = tokenizer.sink.0.into_inner();
    ensure!(
        !tree.overflow,
        "This HTML is too deeply nested or complex to paste."
    );
    let mut out = Converter {
        nodes: &tree.nodes,
        warnings: vec![],
    };
    let mut blocks = vec![];
    out.children(0, &mut blocks, &Span::text(""), None, 0);
    blocks.retain(|b| matches!(b.kind.as_str(), "table" | "quote") || !b.spans.is_empty());
    if blocks.is_empty() {
        blocks.push(Block::paragraph(""));
    }
    let document = Document::from_blocks(blocks);
    document.validate()?;
    Ok(Imported {
        document,
        warnings: out.warnings,
    })
}
struct Converter<'a> {
    nodes: &'a [Node],
    warnings: Vec<String>,
}
impl Converter<'_> {
    fn warn(&mut self, message: &str) {
        if !self.warnings.iter().any(|w| w == message) {
            self.warnings.push(message.into());
        }
    }
    fn children(
        &mut self,
        i: usize,
        out: &mut Vec<Block>,
        style: &Span,
        list: Option<&str>,
        indent: usize,
    ) {
        for child in self.nodes[i].children.clone() {
            self.node(child, out, style, list, indent);
        }
    }
    fn node(
        &mut self,
        i: usize,
        out: &mut Vec<Block>,
        style: &Span,
        list: Option<&str>,
        indent: usize,
    ) {
        let node = &self.nodes[i];
        let name = node.name.as_str();
        if matches!(
            name,
            "script"
                | "style"
                | "head"
                | "iframe"
                | "object"
                | "embed"
                | "svg"
                | "math"
                | "template"
        ) {
            self.warn("Active or unsupported embedded content was omitted.");
            return;
        }
        let mut style = style.clone();
        for (tag, mark) in [
            ("b", "bold"),
            ("strong", "bold"),
            ("i", "italic"),
            ("em", "italic"),
            ("u", "underline"),
            ("s", "strikethrough"),
            ("del", "strikethrough"),
            ("strike", "strikethrough"),
            ("mark", "highlight"),
            ("code", "code"),
        ] {
            if name == tag {
                style.set_mark(mark, true);
            }
        }
        if let Some(css) = node.attrs.get("style") {
            for rule in css.split(';') {
                let Some((key, value)) = rule.split_once(':') else {
                    continue;
                };
                let value = value.trim().to_ascii_lowercase();
                match key.trim().to_ascii_lowercase().as_str() {
                    "font-weight"
                        if value == "bold" || value.parse::<u16>().is_ok_and(|n| n >= 600) =>
                    {
                        style.set_mark("bold", true)
                    }
                    "font-style" if value == "italic" => style.set_mark("italic", true),
                    "text-decoration" | "text-decoration-line" => {
                        if value.contains("underline") {
                            style.set_mark("underline", true);
                        }
                        if value.contains("line-through") {
                            style.set_mark("strikethrough", true);
                        }
                    }
                    "background-color" if value != "transparent" => {
                        style.set_mark("highlight", true)
                    }
                    _ => {}
                }
            }
        }
        if name == "a"
            && let Some(href) = node.attrs.get("href")
        {
            if safe_link(href) {
                style.0["href"] = json!(href);
            } else {
                self.warn("An unsupported link was removed.");
            }
        }
        match name {
            "ul" | "ol" => {
                let kind = if name == "ol" { "ordered" } else { "bullet" };
                self.children(
                    i,
                    out,
                    &style,
                    Some(kind),
                    if list.is_some() {
                        (indent + 1).min(16)
                    } else {
                        indent
                    },
                );
            }
            "table" => {
                if let Some(table) = self.table(i) {
                    out.push(table);
                }
                out.push(Block::paragraph(""));
            }
            "blockquote" => {
                let mut quoted = Vec::new();
                self.children(i, &mut quoted, &style, None, 0);
                quoted.retain(|b| {
                    matches!(b.kind.as_str(), "table" | "quote") || !b.spans.is_empty()
                });
                if quoted.is_empty() {
                    quoted.push(Block::paragraph(""));
                }
                for block in &mut quoted {
                    if block.kind == "paragraph" {
                        block.set_kind("quote");
                    } else if block.kind == "quote" {
                        self.warn("Nested quotations were kept as single-level quote blocks.");
                    } else {
                        self.warn("Headings, lists and tables inside a quotation were kept as separate blocks.");
                    }
                }
                out.extend(quoted);
                out.push(Block::paragraph(""));
            }
            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "pre" => {
                let mut block = Block::paragraph("");
                if name.starts_with('h') && name.len() == 2 {
                    block.set_kind(match name {
                        "h1" => "h1",
                        "h2" => "h2",
                        _ => "h3",
                    });
                }
                if name == "li" {
                    block.set_kind(list.unwrap_or("bullet"));
                    block.attrs.insert("indent".into(), json!(indent));
                }
                out.push(block);
                self.children(i, out, &style, list, indent);
                out.push(Block::paragraph(""));
            }
            "br" => {
                self.text(out, &style, "\n");
            }
            "img" => {
                self.warn("HTML images were not downloaded. Paste the image or attach its file separately.");
                let alt = node
                    .attrs
                    .get("alt")
                    .filter(|s| !s.is_empty())
                    .map_or("Image", String::as_str)
                    .to_owned();
                self.text(out, &style, &format!("[{alt}]"));
            }
            "input" => {
                if node.attrs.get("type").is_some_and(|t| t == "checkbox")
                    && let Some(b) = out.last_mut()
                {
                    b.set_kind("check");
                    b.attrs.insert("indent".into(), json!(indent));
                    b.attrs
                        .insert("checked".into(), json!(node.attrs.contains_key("checked")));
                }
            }
            "" => {
                let text = node.text.replace(['\r', '\t'], " ");
                if !text.trim().is_empty() || out.last().is_some_and(|b| !b.spans.is_empty()) {
                    self.text(out, &style, &text);
                }
            }
            _ => self.children(i, out, &style, list, indent),
        }
    }
    fn text(&mut self, out: &mut Vec<Block>, style: &Span, text: &str) {
        if out.last().is_none_or(|b| b.kind == "table") {
            out.push(Block::paragraph(""));
        }
        out.last_mut().unwrap().spans.push(style.with_text(text));
    }
    fn table(&mut self, index: usize) -> Option<Block> {
        fn rows(nodes: &[Node], i: usize, out: &mut Vec<usize>) {
            for &c in &nodes[i].children {
                if nodes[c].name == "tr" {
                    out.push(c);
                } else if matches!(nodes[c].name.as_str(), "thead" | "tbody" | "tfoot") {
                    rows(nodes, c, out);
                }
            }
        }
        let mut rows_ = vec![];
        rows(self.nodes, index, &mut rows_);
        if rows_.is_empty() || rows_.len() > 1000 {
            self.warn("An unsupported table was omitted.");
            return None;
        }
        let mut cells = vec![];
        let mut occupied = std::collections::HashSet::new();
        let mut columns = 0;
        for (row, &r) in rows_.iter().enumerate() {
            let mut col = 0;
            for c in self.nodes[r].children.clone() {
                if !matches!(self.nodes[c].name.as_str(), "td" | "th") {
                    continue;
                }
                while occupied.contains(&(row, col)) {
                    col += 1;
                }
                if col >= 64 {
                    self.warn("A table exceeded the column limit.");
                    return None;
                }
                let node = &self.nodes[c];
                let rs = node
                    .attrs
                    .get("rowspan")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, rows_.len() - row);
                let cs = node
                    .attrs
                    .get("colspan")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 64 - col);
                let mut blocks = vec![];
                let mut style = Span::text("");
                if node.name == "th" {
                    style.set_mark("bold", true);
                }
                self.children(c, &mut blocks, &style, None, 0);
                blocks.retain(|b| b.kind == "table" || !b.spans.is_empty());
                if blocks.is_empty() {
                    blocks.push(Block::paragraph(""));
                }
                let mut attrs = BTreeMap::new();
                if rs > 1 {
                    attrs.insert("rowSpan".into(), json!(rs));
                }
                if cs > 1 {
                    attrs.insert("colSpan".into(), json!(cs));
                }
                cells.push(Cell {
                    row,
                    col,
                    blocks,
                    attrs,
                });
                for y in row..row + rs {
                    for x in col..col + cs {
                        occupied.insert((y, x));
                    }
                }
                col += cs;
                columns = columns.max(col);
            }
        }
        if columns == 0 {
            return None;
        }
        for row in 0..rows_.len() {
            for col in 0..columns {
                if !occupied.contains(&(row, col)) {
                    cells.push(Cell {
                        row,
                        col,
                        blocks: vec![Block::paragraph("")],
                        attrs: Default::default(),
                    });
                }
            }
        }
        cells.sort_by_key(|c| (c.row, c.col));
        Some(Block {
            id: new_id(),
            kind: "table".into(),
            spans: vec![],
            cells,
            attrs: BTreeMap::from([
                ("rows".into(), json!(rows_.len())),
                ("columns".into(), json!(columns)),
            ]),
        })
    }
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn export(doc: &Document) -> String {
    fn blocks(values: &[Block]) -> String {
        let mut out = String::new();
        let mut lists: Vec<&str> = vec![];
        fn close(out: &mut String, lists: &mut Vec<&str>, depth: usize) {
            while lists.len() > depth {
                let tag = lists.pop().unwrap();
                out.push_str(&format!("</li></{tag}>"));
            }
        }
        for b in values {
            let list = b.kind == "list";
            if list {
                let depth = b.number("indent", 0).min(16) + 1;
                let tag = if b.string("list") == "ordered" {
                    "ol"
                } else {
                    "ul"
                };
                close(&mut out, &mut lists, depth);
                if lists.len() == depth && lists.last() != Some(&tag) {
                    close(&mut out, &mut lists, depth - 1);
                }
                if lists.len() == depth {
                    out.push_str("</li><li>");
                }
                while lists.len() < depth {
                    out.push_str(&format!("<{tag}><li>"));
                    lists.push(tag);
                }
            } else {
                close(&mut out, &mut lists, 0);
            }

            if b.kind == "table" {
                out.push_str("<table>");
                for row in 0..b.number("rows", 1) {
                    out.push_str("<tr>");
                    for c in b.cells.iter().filter(|c| c.row == row) {
                        out.push_str(&format!(
                            "<td rowspan=\"{}\" colspan=\"{}\">{}</td>",
                            c.attrs.get("rowSpan").and_then(|v| v.as_u64()).unwrap_or(1),
                            c.attrs.get("colSpan").and_then(|v| v.as_u64()).unwrap_or(1),
                            blocks(&c.blocks)
                        ));
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
                continue;
            }
            let tag = if b.kind == "heading" {
                format!("h{}", b.number("level", 1))
            } else if b.kind == "quote" {
                "blockquote".into()
            } else {
                "p".into()
            };
            let tag = if list { "li" } else { &tag };
            if !list {
                out.push_str(&format!("<{tag}>"));
            }
            if b.string("list") == "check" {
                out.push_str(if b.checked() {
                    "<input type=\"checkbox\" checked>"
                } else {
                    "<input type=\"checkbox\">"
                });
            }
            for s in &b.spans {
                if !s.is_text() {
                    out.push_str(&escape(
                        s.0.get("alt")
                            .and_then(|v| v.as_str())
                            .unwrap_or("[Attached file]"),
                    ));
                    continue;
                }
                let mut text = escape(s.content()).replace('\n', "<br>");
                for (mark, tag) in [
                    ("bold", "strong"),
                    ("italic", "em"),
                    ("underline", "u"),
                    ("strikethrough", "s"),
                    ("highlight", "mark"),
                    ("code", "code"),
                ] {
                    if s.mark(mark) {
                        text = format!("<{tag}>{text}</{tag}>");
                    }
                }
                if let Some(href) =
                    s.0.get("href")
                        .and_then(|v| v.as_str())
                        .filter(|s| safe_link(s))
                {
                    text = format!("<a href=\"{}\">{text}</a>", escape(href));
                }
                out.push_str(&text);
            }
            if !list {
                out.push_str(&format!("</{tag}>"));
            }
        }
        close(&mut out, &mut lists, 0);
        out
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
        blocks(&doc.blocks)
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_html_is_inert_and_styles_survive() {
        let result=import("<h1>Title</h1><p>Hello <b>bold &amp; bright</b><script>secret()</script><a href='javascript:alert(1)'>unsafe</a><img src='https://x/track' alt='Diagram'></p>").unwrap();
        assert_eq!(result.document.title(), "Title");
        assert!(!result.document.plain_text().contains("secret"));
        assert!(
            result
                .document
                .leaves()
                .iter()
                .flat_map(|b| &b.spans)
                .any(|s| s.mark("bold"))
        );
        assert_eq!(result.warnings.len(), 3);
        assert!(!export(&result.document).contains("javascript:"));
    }
    #[test]
    fn merged_table_and_unicode_round_trip() {
        let imported=import("<p>café 👋</p><table><tr><th colspan='2'>Heading</th></tr><tr><td>One</td><td><em>Two</em></td></tr></table>").unwrap();
        let table = imported
            .document
            .blocks
            .iter()
            .find(|b| b.kind == "table")
            .unwrap();
        assert_eq!(table.number("columns", 0), 2);
        assert_eq!(table.cells[0].attrs["colSpan"], 2);
        let again = import(&export(&imported.document)).unwrap();
        assert!(again.document.plain_text().contains("café 👋"));
    }
    #[test]
    fn inline_spaces_and_nested_lists_survive_exchange() {
        let doc = import("<p><b>one</b> <i>two</i></p><ol><li>First<ul><li>Child</li></ul></li><li>Second</li></ol>").unwrap().document;
        assert_eq!(doc.blocks[0].text(), "one two");
        assert_eq!(doc.blocks[2].number("indent", 0), 1);
        let exported = export(&doc);
        let again = import(&exported).unwrap().document;
        assert_eq!(again.plain_text(), doc.plain_text());
        assert_eq!(again.blocks[2].number("indent", 0), 1);
        assert_eq!(exported.matches("<ol>").count(), 1);
    }
}
