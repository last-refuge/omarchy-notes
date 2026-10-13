use crate::{Cancellation, Paper};
use anyhow::{Context, Result, ensure};
use cairo::{Context as Canvas, Format, ImageSurface};
use notes_core::document::{Block, Document, Span};
use pango::{FontDescription, Layout, LayoutLine, prelude::*};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};
const MARGIN: f64 = 54.;
const TOP: f64 = 86.;
const PAD: f64 = 9.;
const SCALE: f64 = pango::SCALE as f64;
const INK: (f64, f64, f64) = (0.16, 0.17, 0.19);
const MUTED: (f64, f64, f64) = (0.43, 0.44, 0.45);
const GOLD: (f64, f64, f64) = (0.56, 0.43, 0.23);

#[derive(Clone)]
struct Link {
    start: i32,
    end: i32,
    uri: String,
}
#[derive(Clone)]
enum Art {
    QuoteRule,
    Text {
        _layout: Layout,
        line: LayoutLine,
        baseline: f64,
        alignment: f64,
        links: Vec<Link>,
    },
    Image(ImageSurface, f64, f64),
    Box {
        width: f64,
        height: f64,
        header: bool,
    },
}
#[derive(Clone)]
struct Item {
    x: f64,
    y: f64,
    height: f64,
    art: Art,
}
#[derive(Clone, Default)]
struct Flow {
    items: Vec<Item>,
    height: f64,
}
impl Flow {
    fn append(&mut self, other: Flow) {
        for mut item in other.items {
            item.y += self.height;
            self.items.push(item);
        }
        self.height += other.height;
    }
}
pub(super) struct Page {
    items: Vec<Item>,
}
struct Cell {
    col: usize,
    cols: usize,
    row: usize,
    rows: usize,
    flow: Flow,
}

pub(super) struct Engine<'a> {
    root: &'a Path,
    context: pango::Context,
    cancel: &'a Cancellation,
    width: f64,
    height: f64,
    image_bytes: usize,
    images: BTreeMap<String, ImageSurface>,
    pub attachment_labels: usize,
}
impl<'a> Engine<'a> {
    pub fn new(root: &'a Path, canvas: &Canvas, paper: Paper, cancel: &'a Cancellation) -> Self {
        // A separate map/context for each background export; no mutable default font map.
        let context = pangocairo::FontMap::new().create_context();
        pangocairo::functions::context_set_resolution(&context, 72.);
        pangocairo::functions::update_context(canvas, &context);
        let (width, height) = paper.dimensions();
        Self {
            root,
            context,
            cancel,
            width,
            height,
            image_bytes: 0,
            images: BTreeMap::new(),
            attachment_labels: 0,
        }
    }
    fn body_width(&self) -> f64 {
        self.width - MARGIN * 2.
    }
    fn bottom(&self) -> f64 {
        self.height - 66.
    }
    fn body_height(&self) -> f64 {
        self.bottom() - TOP
    }
    fn text(&self, spans: &[Span], width: f64, size: f64, bold: bool, sans: bool) -> Result<Flow> {
        self.cancel.check()?;
        let mut markup = String::new();
        let mut links = vec![];
        let mut offset = 0;
        for span in spans {
            let text = span.content();
            ensure!(
                !text.contains('\0'),
                "This note contains a NUL character that cannot be printed. Remove it before exporting."
            );
            let mut attrs = vec![];
            if bold || span.mark("bold") {
                attrs.push("weight=\"bold\"");
            }
            if span.mark("italic") {
                attrs.push("style=\"italic\"");
            }
            if span.mark("underline") {
                attrs.push("underline=\"single\"");
            }
            if span.mark("strikethrough") {
                attrs.push("strikethrough=\"true\"");
            }
            if span.mark("highlight") {
                attrs.push("background=\"#f0e2b9\"");
            }
            if span.mark("code") {
                attrs.push("font_family=\"monospace\" background=\"#f1f0ed\"");
            }
            if let Some(uri) = span.0.get("href").and_then(|v| v.as_str()).filter(|u| {
                ["https://", "http://", "mailto:"]
                    .iter()
                    .any(|p| u.starts_with(p))
                    && !u.chars().any(char::is_control)
            }) {
                // Avoid duplicate underline attributes if a link is explicitly underlined.
                if !span.mark("underline") {
                    attrs.push("underline=\"single\"");
                }
                attrs.push("foreground=\"#795c30\"");
                links.push(Link {
                    start: offset,
                    end: offset + text.len() as i32,
                    uri: uri.to_owned(),
                });
            }
            markup.push_str(&format!(
                "<span {}>{}</span>",
                attrs.join(" "),
                escape(text)
            ));
            offset += text.len() as i32;
        }
        // parse_markup returns errors instead of logging-and-dropping text on invalid markup.
        let (attrs, text, _) =
            pango::parse_markup(&markup, '\0').context("This text cannot be laid out for PDF.")?;
        let layout = Layout::new(&self.context);
        let mut font = FontDescription::new();
        font.set_family(if sans {
            "Noto Sans, sans-serif"
        } else {
            "Noto Serif, serif"
        });
        font.set_absolute_size(size * SCALE);
        layout.set_font_description(Some(&font));
        layout.set_width((width * SCALE) as i32);
        layout.set_wrap(pango::WrapMode::WordChar);
        layout.set_text(&text);
        layout.set_attributes(Some(&attrs));
        ensure!(
            layout.unknown_glyphs_count() == 0,
            "A font for some characters is missing. Install a font for this language, then export again."
        );
        let mut flow = Flow::default();
        let mut iter = layout.iter();
        let mut first_link = 0;
        loop {
            let line = iter
                .line_readonly()
                .context("Couldn’t shape this paragraph.")?;
            let (_, logical) = iter.line_extents();
            let height = (logical.height() as f64 / SCALE).max(size * 1.48);
            while first_link < links.len() && links[first_link].end <= line.start_index() {
                first_link += 1;
            }
            let line_links = links[first_link..]
                .iter()
                .take_while(|link| link.start < line.start_index() + line.length())
                .cloned()
                .collect();
            flow.items.push(Item {
                x: logical.x() as f64 / SCALE,
                y: flow.height,
                height,
                art: Art::Text {
                    _layout: layout.clone(),
                    line,
                    alignment: logical.x() as f64 / SCALE,
                    baseline: (iter.baseline() - logical.y()) as f64 / SCALE,
                    links: line_links,
                },
            });
            flow.height += height;
            if !iter.next_line() {
                break;
            }
        }
        Ok(flow)
    }
    fn plain(&self, text: &str, width: f64, size: f64, bold: bool, sans: bool) -> Result<Flow> {
        self.text(&[Span::text(text)], width, size, bold, sans)
    }
    fn object(&mut self, span: &Span, width: f64) -> Result<Flow> {
        self.cancel.check()?;
        if let Some(hash) = span.0.get("image").and_then(|v| v.as_str()) {
            let image = if let Some(image) = self.images.get(hash) {
                image.clone()
            } else {
                let bytes = notes_storage::media::read_blob(self.root, hash).context(
                    "An image is missing or damaged. Restore or remove it before exporting.",
                )?;
                let (image, _) = notes_storage::media::decode(&bytes)?;
                // Bound retained rasters. Downsampling still gives >300 dpi for a full-width image.
                let mut rgba = image.thumbnail(2400, 2400).to_rgba8();
                let (w, h) = rgba.dimensions();
                self.image_bytes += rgba.len();
                ensure!(
                    self.image_bytes <= 64 * 1024 * 1024,
                    "This note exceeds the 64 MiB PDF image budget. Export a smaller note."
                );
                for pixel in rgba.pixels_mut() {
                    let [r, g, b, a] = pixel.0;
                    let premultiply = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
                    pixel.0 =
                        u32::from_be_bytes([a, premultiply(r), premultiply(g), premultiply(b)])
                            .to_ne_bytes();
                }
                let surface = ImageSurface::create_for_data(
                    rgba.into_raw(),
                    Format::ARgb32,
                    w as i32,
                    h as i32,
                    w as i32 * 4,
                )?;
                self.images.insert(hash.to_owned(), surface.clone());
                surface
            };
            let scale = (width / image.width() as f64)
                .min((self.body_height() - 40.) / image.height() as f64)
                .min(0.75);
            let w = image.width() as f64 * scale;
            let h = image.height() as f64 * scale;
            let mut flow = Flow {
                items: vec![Item {
                    x: 0.,
                    y: 5.,
                    height: h + 10.,
                    art: Art::Image(image, w, h),
                }],
                height: h + 12.,
            };
            if let Some(alt) = span
                .0
                .get("alt")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                flow.append(self.plain(alt, width, 8., false, true)?);
            }
            Ok(flow)
        } else {
            let id = span.0["attachment"]
                .as_str()
                .context("Invalid attachment.")?;
            let info = notes_storage::media::attachment_info(self.root, id)?;
            ensure!(
                notes_storage::media::read_blob(self.root, &info.hash)?.len() as u64 == info.size,
                "An attachment is missing or damaged."
            );
            self.attachment_labels += 1;
            let mut text = self.plain(&info.name, width - PAD * 2., 10., true, true)?;
            text.append(self.plain(
                &format!(
                    "Attached file · {} · {} bytes · contents not included",
                    info.mime, info.size
                ),
                width - PAD * 2.,
                8.,
                false,
                true,
            )?);
            let height = text.height + PAD * 2.;
            let mut flow = Flow {
                items: vec![Item {
                    x: 0.,
                    y: 0.,
                    height,
                    art: Art::Box {
                        width,
                        height,
                        header: true,
                    },
                }],
                height,
            };
            for mut item in text.items {
                item.x += PAD;
                item.y += PAD;
                flow.items.push(item);
            }
            Ok(flow)
        }
    }
    fn block(&mut self, block: &Block, width: f64, number: usize) -> Result<Flow> {
        ensure!(
            block.kind != "table",
            "Nested tables cannot be exported yet. Move the inner table outside its cell."
        );
        let heading = block.kind == "heading";
        let size = if heading {
            match block.number("level", 1) {
                1 => 26.,
                2 => 17.,
                _ => 13.,
            }
        } else {
            11.
        };
        let indent = if block.kind == "quote" {
            18.
        } else if block.kind == "list" {
            19. + block.number("indent", 0) as f64 * 12.
        } else {
            0.
        };
        ensure!(
            width - indent >= 30.,
            "This table or list is too narrow to print. Reduce its columns or indentation."
        );
        let mut flow = Flow::default();
        let mut text = vec![];
        let flush = |text: &mut Vec<Span>, flow: &mut Flow, this: &Self| -> Result<()> {
            if !text.is_empty() {
                flow.append(this.text(text, width - indent, size, heading, false)?);
                text.clear();
            }
            Ok(())
        };
        for span in &block.spans {
            if span.is_text() {
                text.push(span.clone());
            } else {
                flush(&mut text, &mut flow, self)?;
                flow.append(self.object(span, width - indent)?);
            }
        }
        flush(&mut text, &mut flow, self)?;
        if flow.items.is_empty() {
            flow = self.plain("", width - indent, size, heading, false)?;
        }
        for item in &mut flow.items {
            item.x += indent;
        }
        if block.kind == "quote" {
            // One segment per text/object item retains existing pagination
            // boundaries; a long quotation never becomes one unbreakable item.
            let rules = flow
                .items
                .iter()
                .map(|item| Item {
                    x: 2.,
                    y: item.y,
                    height: item.height,
                    art: Art::QuoteRule,
                })
                .collect::<Vec<_>>();
            flow.items.extend(rules);
        }
        if block.kind == "list" {
            let marker = match block.string("list") {
                "ordered" => format!("{number}."),
                "check" => if block.checked() { "☑" } else { "☐" }.into(),
                _ => "•".into(),
            };
            for mut item in self.plain(&marker, 20., 10., false, true)?.items {
                item.x += indent - 19.;
                flow.items.push(item);
            }
        }
        flow.height += if heading {
            10.
        } else if block.kind == "list" {
            3.
        } else {
            9.
        };
        Ok(flow)
    }
    fn next_page(&self, pages: &mut Vec<Page>, y: &mut f64) -> Result<()> {
        self.cancel.check()?;
        ensure!(
            pages.len() < 500,
            "PDF export is limited to 500 pages. Export a smaller note."
        );
        pages.push(Page { items: vec![] });
        *y = TOP;
        Ok(())
    }
    fn place(&self, pages: &mut [Page], flow: &Flow, x: f64, y: f64) {
        for mut item in flow.items.clone() {
            item.x += x;
            item.y += y;
            pages.last_mut().unwrap().items.push(item);
        }
    }
    fn paragraph(&self, pages: &mut Vec<Page>, y: &mut f64, flow: Flow) -> Result<()> {
        // Normal paragraphs stay together if short. Long paragraphs split at visual lines.
        let keep = flow.height.min(48.);
        if *y + keep > self.bottom() && *y > TOP {
            self.next_page(pages, y)?;
        }
        // Group items whose vertical intervals overlap (e.g. list marker or attachment box).
        let chunks = chunks(flow);
        let mut index = 0;
        while index < chunks.len() {
            let chunk = &chunks[index];
            ensure!(
                chunk.height <= self.body_height(),
                "An image, heading or attachment label is taller than a page."
            );
            if *y + chunk.height > self.bottom() {
                self.next_page(pages, y)?;
            }
            // Avoid leaving a single final line on the following page.
            if chunks.len() - index == 2
                && *y > TOP
                && *y + chunks[index].height + chunks[index + 1].height > self.bottom()
            {
                self.next_page(pages, y)?;
            }
            self.place(pages, chunk, MARGIN, *y);
            *y += chunk.height;
            index += 1;
        }
        Ok(())
    }
    pub fn paginate(&mut self, document: &Document) -> Result<Vec<Page>> {
        let mut pages = vec![Page { items: vec![] }];
        let mut y = TOP;
        let mut counters = [0usize; 17];
        let mut headings = vec![];
        for block in &document.blocks {
            self.cancel.check()?;
            if block.kind == "table" {
                self.table(block, &mut pages, &mut y, &mut headings)?;
                counters.fill(0);
                continue;
            }
            let indent = block.number("indent", 0).min(16);
            if block.string("list") == "ordered" {
                counters[indent] += 1;
                counters[indent + 1..].fill(0);
            } else if block.kind != "list" {
                counters.fill(0);
            }
            let flow = self.block(block, self.body_width(), counters[indent])?;
            if block.kind == "heading" {
                headings.push(flow);
            } else {
                let keep: f64 = chunks(flow.clone()).iter().take(2).map(|f| f.height).sum();
                self.headings(&mut pages, &mut y, &mut headings, keep)?;
                self.paragraph(&mut pages, &mut y, flow)?;
            }
        }
        self.headings(&mut pages, &mut y, &mut headings, 0.)?;
        Ok(pages)
    }
    fn headings(
        &self,
        pages: &mut Vec<Page>,
        y: &mut f64,
        headings: &mut Vec<Flow>,
        next: f64,
    ) -> Result<()> {
        let height: f64 = headings.iter().map(|h| h.height).sum();
        if height > 0.
            && *y > TOP
            && *y + height + next.min(self.body_height() - height) > self.bottom()
        {
            self.next_page(pages, y)?;
        }
        for flow in headings.drain(..) {
            self.paragraph(pages, y, flow)?;
        }
        Ok(())
    }
    fn table(
        &mut self,
        block: &Block,
        pages: &mut Vec<Page>,
        y: &mut f64,
        headings: &mut Vec<Flow>,
    ) -> Result<()> {
        let rows = block.number("rows", 1);
        let cols = block.number("columns", 1);
        ensure!(
            cols <= 8,
            "PDF tables support up to eight columns. Split this table before exporting."
        );
        let cw = self.body_width() / cols as f64;
        let mut cells = vec![];
        let mut heights = vec![PAD * 2. + 16.; rows];
        let mut occupied = vec![false; rows * cols];
        for cell in &block.cells {
            let nr = cell
                .attrs
                .get("rowSpan")
                .and_then(|n| n.as_u64())
                .unwrap_or(1) as usize;
            let nc = cell
                .attrs
                .get("colSpan")
                .and_then(|n| n.as_u64())
                .unwrap_or(1) as usize;
            for r in cell.row..cell.row + nr {
                for c in cell.col..cell.col + nc {
                    ensure!(
                        !occupied[r * cols + c],
                        "This table has overlapping cells. Repair the table before exporting."
                    );
                    occupied[r * cols + c] = true;
                }
            }
            let mut flow = Flow::default();
            let mut number = 0;
            for b in &cell.blocks {
                if b.string("list") == "ordered" {
                    number += 1;
                } else {
                    number = 0;
                }
                flow.append(self.block(b, cw * nc as f64 - PAD * 2., number)?);
            }
            if nr == 1 {
                heights[cell.row] = heights[cell.row].max(flow.height + PAD * 2.);
            }
            cells.push(Cell {
                col: cell.col,
                cols: nc,
                row: cell.row,
                rows: nr,
                flow,
            });
        }
        for cell in cells.iter().filter(|c| c.rows > 1) {
            let current: f64 = heights[cell.row..cell.row + cell.rows].iter().sum();
            let missing = (cell.flow.height + PAD * 2. - current).max(0.);
            heights[cell.row + cell.rows - 1] += missing;
        }
        // First row repeats only when it is independent of vertical merges.
        let header = if cells.iter().any(|c| c.row == 0 && c.rows > 1) {
            None
        } else {
            Some(self.band(&cells, &heights, 0, 1, cw, true))
        };
        // Reserve the first two rows (including all connected vertical merges).
        let mut first_end = rows.min(2);
        loop {
            let expanded = cells
                .iter()
                .filter(|c| c.row < first_end)
                .map(|c| c.row + c.rows)
                .max()
                .unwrap_or(first_end)
                .max(first_end);
            if first_end == expanded {
                break;
            }
            first_end = expanded;
        }
        let mut first_height: f64 = heights[..first_end].iter().sum();
        if first_end == 2
            && cells.iter().filter(|c| c.row < 2).all(|c| c.rows == 1)
            && first_height > self.body_height()
        {
            first_height = heights[0] + 60.;
        }
        self.headings(pages, y, headings, first_height)?;
        if *y > TOP && *y + first_height.min(self.body_height()) > self.bottom() {
            self.next_page(pages, y)?;
        }
        let mut row = 0;
        while row < rows {
            self.cancel.check()?;
            let mut end = row + 1;
            loop {
                let expanded = cells
                    .iter()
                    .filter(|c| c.row >= row && c.row < end)
                    .map(|c| c.row + c.rows)
                    .max()
                    .unwrap_or(end)
                    .max(end);
                if end == expanded {
                    break;
                }
                end = expanded;
            }
            let mut band = self.band(&cells, &heights, row, end, cw, row == 0);
            let header_height = header.as_ref().filter(|_| row > 0).map_or(0., |h| h.height);
            ensure!(
                header_height < self.body_height() / 2.,
                "The table header is too tall to repeat. Shorten its first row."
            );
            if band.height <= self.body_height() - header_height {
                if *y + band.height > self.bottom() {
                    self.next_page(pages, y)?;
                    if row > 0
                        && let Some(h) = &header
                    {
                        self.place(pages, h, MARGIN, *y);
                        *y += h.height;
                    }
                }
                self.place(pages, &band, MARGIN, *y);
                *y += band.height;
            } else {
                ensure!(
                    end == row + 1 && row > 0,
                    "A merged table row or header is taller than a page. Split the row before exporting."
                );
                // Split a long unmerged row independently at whole text/image/card boundaries in each cell.
                let row_cells: Vec<_> = cells.iter().filter(|c| c.row == row).collect();
                let mut queues: Vec<VecDeque<Flow>> = row_cells
                    .iter()
                    .map(|c| chunks(c.flow.clone()).into())
                    .collect();
                while queues.iter().any(|q| !q.is_empty()) {
                    let needed = queues
                        .iter()
                        .map(|q| q.iter().take(2).map(|f| f.height).sum::<f64>())
                        .fold(0., f64::max)
                        + PAD * 2.;
                    if *y + needed > self.bottom() {
                        self.next_page(pages, y)?;
                        if let Some(h) = &header {
                            self.place(pages, h, MARGIN, *y);
                            *y += h.height;
                        }
                    }
                    let available = self.bottom() - *y - PAD * 2.;
                    let mut selected = vec![];
                    let mut height: f64 = 16.;
                    for queue in &mut queues {
                        let mut parts = vec![];
                        let mut taken = 0.;
                        while queue.front().is_some_and(|f| taken + f.height <= available) {
                            let part = queue.pop_front().unwrap();
                            taken += part.height;
                            parts.push(part);
                        }
                        // Keep two final slices together instead of stranding one line.
                        if queue.len() == 1 && parts.len() >= 2 {
                            queue.push_front(parts.pop().unwrap());
                        }
                        let mut flow = Flow::default();
                        for part in parts {
                            flow.append(part);
                        }
                        ensure!(
                            queue.is_empty() || !flow.items.is_empty(),
                            "An object in this table row is too tall to fit. Resize it before exporting."
                        );
                        height = height.max(flow.height);
                        selected.push(flow);
                    }
                    band = Flow {
                        items: vec![],
                        height: height + PAD * 2.,
                    };
                    for (cell, flow) in row_cells.iter().zip(selected) {
                        add_cell(
                            &mut band,
                            flow,
                            cell.col as f64 * cw,
                            0.,
                            cell.cols as f64 * cw,
                            height + PAD * 2.,
                            false,
                        );
                    }
                    self.place(pages, &band, MARGIN, *y);
                    *y += band.height;
                }
            }
            row = end;
        }
        *y += 14.;
        Ok(())
    }
    fn band(
        &self,
        cells: &[Cell],
        heights: &[f64],
        start: usize,
        end: usize,
        cw: f64,
        header: bool,
    ) -> Flow {
        let height = heights[start..end].iter().sum();
        let mut flow = Flow {
            items: vec![],
            height,
        };
        // Whole band provides borders/background for missing (empty) cells too.
        flow.items.push(Item {
            x: 0.,
            y: 0.,
            height,
            art: Art::Box {
                width: self.body_width(),
                height,
                header,
            },
        });
        for cell in cells.iter().filter(|c| c.row >= start && c.row < end) {
            let y: f64 = heights[start..cell.row].iter().sum();
            let h: f64 = heights[cell.row..cell.row + cell.rows].iter().sum();
            add_cell(
                &mut flow,
                cell.flow.clone(),
                cell.col as f64 * cw,
                y,
                cell.cols as f64 * cw,
                h,
                header,
            );
        }
        flow
    }
    pub fn paint_page(
        &self,
        canvas: &Canvas,
        page: &Page,
        index: usize,
        count: usize,
        title: &str,
    ) -> Result<()> {
        canvas.set_source_rgb(1., 1., 1.);
        canvas.paint()?;
        color(canvas, GOLD);
        canvas.rectangle(MARGIN, 39., 18., 2.);
        canvas.fill()?;
        let brand = self.plain("OMARCHY NOTES", 160., 8., true, true)?;
        self.paint_flow(canvas, &brand, MARGIN + 28., 33., MUTED)?;
        if index > 0 {
            let short: String = title.chars().take(42).collect();
            let title = self.plain(&short, self.body_width() - 205., 8., false, true)?;
            // Header stays one line, independent of how long the original title is.
            if let Some(item) = title.items.first() {
                self.paint_item(canvas, item, MARGIN + 205., 33., MUTED)?;
            }
        }
        color(canvas, (0.86, 0.85, 0.82));
        canvas.set_line_width(0.5);
        canvas.move_to(MARGIN, self.height - 46.);
        canvas.line_to(self.width - MARGIN, self.height - 46.);
        canvas.stroke()?;
        let footer = self.plain(&format!("{}  /  {count}", index + 1), 100., 8., false, true)?;
        self.paint_flow(
            canvas,
            &footer,
            self.width - MARGIN - 44.,
            self.height - 33.,
            MUTED,
        )?;
        for item in &page.items {
            self.cancel.check()?;
            self.paint_item(canvas, item, 0., 0., INK)?;
        }
        canvas.status()?;
        Ok(())
    }
    fn paint_flow(
        &self,
        canvas: &Canvas,
        flow: &Flow,
        x: f64,
        y: f64,
        ink: (f64, f64, f64),
    ) -> Result<()> {
        for item in &flow.items {
            self.paint_item(canvas, item, x, y, ink)?;
        }
        Ok(())
    }
    fn paint_item(
        &self,
        canvas: &Canvas,
        item: &Item,
        x: f64,
        y: f64,
        ink: (f64, f64, f64),
    ) -> Result<()> {
        let x = x + item.x;
        let y = y + item.y;
        canvas.save()?;
        color(canvas, ink);
        match &item.art {
            Art::QuoteRule => {
                color(canvas, GOLD);
                canvas.rectangle(x, y, 1.5, item.height);
                canvas.fill()?;
            }
            Art::Text {
                line,
                baseline,
                alignment,
                links,
                ..
            } => {
                canvas.move_to(x, y + baseline);
                pangocairo::functions::show_layout_line(canvas, line);
                for link in links {
                    let start = link.start.max(line.start_index());
                    let end = link.end.min(line.start_index() + line.length());
                    if start >= end {
                        continue;
                    }
                    for pair in line.x_ranges(start, end).as_chunks::<2>().0 {
                        let left = x - alignment + pair[0] as f64 / SCALE;
                        let width = (pair[1] - pair[0]) as f64 / SCALE;
                        let uri = link.uri.replace('\\', "\\\\").replace('\'', "\\'");
                        canvas.tag_begin(
                            "Link",
                            &format!("uri='{uri}' rect=[{left} {y} {width} {}]", item.height),
                        );
                        canvas.tag_end("Link");
                    }
                }
            }
            Art::Image(image, width, height) => {
                canvas.translate(x, y);
                canvas.scale(width / image.width() as f64, height / image.height() as f64);
                canvas.set_source_surface(image, 0., 0.)?;
                canvas.paint()?;
            }
            Art::Box {
                width,
                height,
                header,
            } => {
                color(
                    canvas,
                    if *header {
                        (0.96, 0.95, 0.92)
                    } else {
                        (1., 1., 1.)
                    },
                );
                canvas.rectangle(x, y, *width, *height);
                canvas.fill_preserve()?;
                color(canvas, (0.85, 0.84, 0.80));
                canvas.set_line_width(0.5);
                canvas.stroke()?;
            }
        }
        canvas.restore()?;
        Ok(())
    }
}
fn add_cell(flow: &mut Flow, content: Flow, x: f64, y: f64, width: f64, height: f64, header: bool) {
    flow.items.push(Item {
        x,
        y,
        height,
        art: Art::Box {
            width,
            height,
            header,
        },
    });
    for mut item in content.items {
        item.x += x + PAD;
        item.y += y + PAD;
        flow.items.push(item);
    }
}
fn color(canvas: &Canvas, rgb: (f64, f64, f64)) {
    canvas.set_source_rgb(rgb.0, rgb.1, rgb.2);
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
// Turn overlapping paint items into indivisible vertical slices, preserving paragraph gaps.
fn chunks(mut flow: Flow) -> Vec<Flow> {
    flow.items.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut out = vec![];
    let mut start = 0.;
    let mut end = 0.;
    let mut items = vec![];
    for item in flow.items {
        if !items.is_empty() && item.y >= end - 0.001 {
            out.push(Flow {
                items,
                height: item.y - start,
            });
            items = vec![];
            start = item.y;
        }
        end = end.max(item.y + item.height);
        let mut relative = item;
        relative.y -= start;
        items.push(relative);
    }
    if !items.is_empty() {
        out.push(Flow {
            items,
            height: (flow.height - start).max(end - start),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use notes_core::document::Cell as DocCell;
    use serde_json::json;
    fn table(rows: usize, columns: usize) -> Block {
        let mut b = Block::paragraph("");
        b.kind = "table".into();
        b.attrs.insert("rows".into(), json!(rows));
        b.attrs.insert("columns".into(), json!(columns));
        b
    }
    fn cell(row: usize, col: usize, text: &str) -> DocCell {
        DocCell {
            row,
            col,
            blocks: vec![Block::paragraph(text)],
            attrs: BTreeMap::new(),
        }
    }
    #[test]
    fn long_paragraph_pages_stay_inside_printable_bounds() {
        let canvas = Canvas::new(ImageSurface::create(Format::ARgb32, 1, 1).unwrap()).unwrap();
        let cancel = Cancellation::default();
        let mut engine = Engine::new(Path::new("."), &canvas, Paper::Letter, &cancel);
        let doc = Document {
            blocks: vec![Block::paragraph(
                &"A paragraph with enough room for each word. ".repeat(450),
            )],
            ..Default::default()
        };
        let pages = engine.paginate(&doc).unwrap();
        assert!(pages.len() > 3);
        for page in pages {
            assert!(page.items.len() >= 2);
            for item in page.items {
                assert!(item.y >= TOP);
                assert!(item.y + item.height <= engine.bottom() + 0.01);
                assert!(item.x >= MARGIN);
            }
        }
    }
    #[test]
    fn merged_cells_keep_geometry_and_overlaps_fail() {
        let canvas = Canvas::new(ImageSurface::create(Format::ARgb32, 1, 1).unwrap()).unwrap();
        let cancel = Cancellation::default();
        let mut engine = Engine::new(Path::new("."), &canvas, Paper::Letter, &cancel);
        let mut b = table(2, 2);
        let mut merged = cell(0, 0, "Two rows");
        merged.attrs.insert("rowSpan".into(), json!(2));
        b.cells = vec![merged, cell(0, 1, "Top"), cell(1, 1, "Bottom")];
        let mut doc = Document {
            blocks: vec![b],
            ..Default::default()
        };
        let pages = engine.paginate(&doc).unwrap();
        assert_eq!(pages.len(), 1);
        let texts: Vec<_> = pages[0]
            .items
            .iter()
            .filter(|i| matches!(i.art, Art::Text { .. }))
            .collect();
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0].y, texts[1].y);
        assert!(texts[2].y > texts[1].y);
        doc.blocks[0].cells.push(cell(1, 0, "Overlap"));
        assert!(engine.paginate(&doc).is_err());
    }
    #[test]
    fn nested_tables_and_oversized_merged_rows_fail_without_flattening() {
        let canvas = Canvas::new(ImageSurface::create(Format::ARgb32, 1, 1).unwrap()).unwrap();
        let cancel = Cancellation::default();
        let mut engine = Engine::new(Path::new("."), &canvas, Paper::Letter, &cancel);
        let mut b = table(1, 1);
        let mut c = cell(0, 0, "");
        c.blocks = vec![table(1, 1)];
        b.cells.push(c);
        assert!(
            engine
                .paginate(&Document {
                    blocks: vec![b],
                    ..Default::default()
                })
                .is_err()
        );
        let mut b = table(2, 1);
        let mut c = cell(0, 0, &"A tall merged row. ".repeat(1000));
        c.attrs.insert("rowSpan".into(), json!(2));
        b.cells.push(c);
        assert!(
            engine
                .paginate(&Document {
                    blocks: vec![b],
                    ..Default::default()
                })
                .is_err()
        );
    }
}
