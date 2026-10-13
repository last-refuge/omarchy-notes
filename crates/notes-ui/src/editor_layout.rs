//! Bounded per-root-block shaping; full document geometry is retained.
use super::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    mem::size_of,
};

pub(super) const MAX_LAYOUT_CACHE_ROOTS: usize = 4096;
pub(super) const MAX_LAYOUT_WIDTH_VARIANTS: usize = 8;
pub(super) const MAX_LAYOUT_CACHE_ESTIMATED_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_GEOMETRY_WIDTHS: usize = 8;
pub(super) const MAX_GEOMETRY_ESTIMATED_BYTES: usize = 8 * 1024 * 1024;

#[cfg(feature = "visual-tests")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LayoutDiagnostics {
    pub cache_roots: usize,
    pub cache_entries: usize,
    pub cache_estimated_bytes: usize,
    pub shape_hits: u64,
    pub shape_misses: u64,
    pub cache_evictions: u64,
    pub geometry_entries: usize,
    pub geometry_estimated_bytes: usize,
    pub geometry_hits: u64,
    pub geometry_misses: u64,
    pub painted_lines: usize,
    pub total_lines: usize,
}
struct GeometryEntry {
    layout: Rc<Layout>,
    last_used: u64,
    estimated_bytes: usize,
}
/// Complete geometry can serve each repeated width probe until the editor's
/// existing layout invalidation flag clears the content generation.
#[derive(Default)]
pub(super) struct GeometryCache {
    entries: HashMap<u32, GeometryEntry>,
    clock: u64,
    estimated_bytes: usize,
    hits: u64,
    misses: u64,
}
fn geometry_bytes(layout: &Layout) -> usize {
    size_of::<Layout>()
        + 128
        + layout.lines.capacity() * size_of::<Line>()
        + layout.objects.capacity() * size_of::<Object>()
        + layout
            .objects
            .iter()
            .map(|o| o.key.capacity() + o.label.capacity())
            .sum::<usize>()
        + layout.checks.capacity() * size_of::<(String, Bounds<Pixels>, bool)>()
        + layout
            .checks
            .iter()
            .map(|(id, _, _)| id.capacity())
            .sum::<usize>()
        + (layout.borders.capacity() + layout.quotes.capacity()) * size_of::<Bounds<Pixels>>()
        + layout.labels.capacity() * size_of::<(String, Point<Pixels>)>()
        + layout
            .labels
            .iter()
            .map(|(label, _)| label.capacity())
            .sum::<usize>()
}
impl GeometryCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.estimated_bytes = 0;
    }
    #[cfg(feature = "visual-tests")]
    pub(super) fn diagnostics(&self, stats: &mut LayoutDiagnostics) {
        stats.geometry_entries = self.entries.len();
        stats.geometry_estimated_bytes = self.estimated_bytes;
        stats.geometry_hits = self.hits;
        stats.geometry_misses = self.misses;
    }
    pub(super) fn get(&mut self, width: Pixels, build: impl FnOnce() -> Layout) -> Rc<Layout> {
        self.clock += 1;
        let key = f32::from(width).to_bits();
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.clock;
            self.hits += 1;
            return entry.layout.clone();
        }
        self.misses += 1;
        let layout = Rc::new(build());
        let estimated_bytes = geometry_bytes(&layout);
        if estimated_bytes <= MAX_GEOMETRY_ESTIMATED_BYTES {
            self.entries.insert(
                key,
                GeometryEntry {
                    layout: layout.clone(),
                    last_used: self.clock,
                    estimated_bytes,
                },
            );
            self.estimated_bytes += estimated_bytes;
            while self.entries.len() > MAX_GEOMETRY_WIDTHS
                || self.estimated_bytes > MAX_GEOMETRY_ESTIMATED_BYTES
            {
                let Some(key) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.last_used)
                    .map(|(&key, _)| key)
                else {
                    break;
                };
                let removed = self.entries.remove(&key).unwrap();
                self.estimated_bytes -= removed.estimated_bytes;
            }
        }
        layout
    }
}
struct Variant {
    layout: Rc<Layout>,
    global_len: usize,
    last_used: u64,
}
struct Root {
    block: Block,
    variants: HashMap<u32, Variant>,
    estimated_bytes: usize,
}
#[derive(Default)]
pub(super) struct ShapeCache {
    entries: HashMap<String, Root>,
    lru: BTreeMap<u64, (String, u32)>,
    style: Option<(Theme, WritingStyle)>,
    clock: u64,
    estimated_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}
impl ShapeCache {
    #[cfg(feature = "visual-tests")]
    pub(super) fn diagnostics(&self) -> LayoutDiagnostics {
        LayoutDiagnostics {
            cache_roots: self.entries.len(),
            cache_entries: self.lru.len(),
            cache_estimated_bytes: self.estimated_bytes,
            shape_hits: self.hits,
            shape_misses: self.misses,
            cache_evictions: self.evictions,
            ..Default::default()
        }
    }
    fn remove(&mut self, id: &str) {
        if let Some(root) = self.entries.remove(id) {
            self.estimated_bytes = self.estimated_bytes.saturating_sub(root.estimated_bytes);
            for variant in root.variants.into_values() {
                self.lru.remove(&variant.last_used);
            }
        }
    }
    fn remove_variant(&mut self, id: &str, width: u32) {
        if let Some(root) = self.entries.get_mut(id) {
            let before = root.estimated_bytes;
            if let Some(variant) = root.variants.remove(&width) {
                self.lru.remove(&variant.last_used);
                self.evictions += 1;
            }
            if root.variants.is_empty() {
                self.entries.remove(id);
                self.estimated_bytes = self.estimated_bytes.saturating_sub(before);
            } else {
                root.estimated_bytes = estimate(
                    &root.block,
                    root.variants.values().map(|v| v.layout.as_ref()),
                );
                self.estimated_bytes =
                    self.estimated_bytes.saturating_sub(before) + root.estimated_bytes;
            }
        }
    }
    fn bound(&mut self, inserted: &str) {
        if let Some(root) = self.entries.get(inserted)
            && root.variants.len() > MAX_LAYOUT_WIDTH_VARIANTS
            && let Some((&width, _)) = root.variants.iter().min_by_key(|(_, v)| v.last_used)
        {
            self.remove_variant(inserted, width);
        }
        while self.entries.len() > MAX_LAYOUT_CACHE_ROOTS {
            let Some(id) = self
                .entries
                .iter()
                .min_by_key(|(_, root)| root.variants.values().map(|v| v.last_used).max())
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.evictions += self.entries[&id].variants.len() as u64;
            self.remove(&id);
        }
        while self.estimated_bytes > MAX_LAYOUT_CACHE_ESTIMATED_BYTES {
            let Some((id, width)) = self.lru.first_key_value().map(|(_, key)| key.clone()) else {
                break;
            };
            self.remove_variant(&id, width);
        }
    }
}
fn empty_layout(width: Pixels) -> Layout {
    Layout {
        width,
        text_len: 0,
        height: px(0.),
        lines: vec![],
        objects: vec![],
        checks: vec![],
        borders: vec![],
        quotes: vec![],
        labels: vec![],
    }
}
// Estimate retained block/map/decorations overhead from source bytes; directly
// count exposed glyph/run/vector capacities. GPUI shared caches, native font
// allocations and GPU raster memory are excluded, so this is not a byte-accurate
// whole-process allocation bound. The root-entry count is an additional hard cap.
fn estimate<'a>(block: &Block, layouts: impl Iterator<Item = &'a Layout>) -> usize {
    fn spans(block: &Block) -> usize {
        block.spans.len()
            + block
                .cells
                .iter()
                .flat_map(|c| c.blocks.iter())
                .map(spans)
                .sum::<usize>()
    }
    let source = serde_json::to_vec(block).map_or(0, |json| json.len());
    let decorations = spans(block) * size_of::<DecorationRun>();
    let mut bytes = source.saturating_mul(3) + 1024;
    let mut glyphs = HashSet::new();
    for layout in layouts {
        bytes += layout.lines.capacity() * size_of::<Line>()
            + layout.objects.capacity() * size_of::<Object>()
            + layout.checks.capacity() * size_of::<(String, Bounds<Pixels>, bool)>()
            + (layout.borders.capacity() + layout.quotes.capacity()) * size_of::<Bounds<Pixels>>()
            + layout.labels.capacity() * size_of::<(String, Point<Pixels>)>()
            + decorations;
        for line in &layout.lines {
            let unwrapped = &line.line.unwrapped_layout;
            bytes += line.line.text.len() * 2
                + size_of::<WrappedLine>()
                + size_of::<WrappedLineLayout>()
                + line.line.wrap_boundaries.capacity() * size_of::<WrapBoundary>();
            if glyphs.insert(std::sync::Arc::as_ptr(unwrapped)) {
                bytes += size_of::<LineLayout>()
                    + unwrapped.runs.capacity() * size_of::<ShapedRun>()
                    + unwrapped
                        .runs
                        .iter()
                        .map(|run| run.glyphs.capacity() * size_of::<ShapedGlyph>())
                        .sum::<usize>();
            }
        }
    }
    bytes
}
pub(super) fn make_layout_cached(
    document: &Document,
    width: Pixels,
    theme: Theme,
    typography: WritingStyle,
    window: &Window,
    cache: &mut ShapeCache,
) -> Layout {
    let style = (theme, typography);
    if cache.style != Some(style) {
        cache.entries.clear();
        cache.lru.clear();
        cache.estimated_bytes = 0;
        cache.style = Some(style);
    }
    let live: HashSet<_> = document.blocks.iter().map(|b| b.id.as_str()).collect();
    let removed: Vec<_> = cache
        .entries
        .keys()
        .filter(|id| !live.contains(id.as_str()))
        .cloned()
        .collect();
    for id in removed {
        cache.remove(&id);
    }
    let mut out = empty_layout(width);
    let mut global = 0;
    let mut ordered = [0usize; 17];
    for block in &document.blocks {
        cache.clock += 1;
        let clock = cache.clock;
        let width_key = f32::from(width).to_bits();
        if cache
            .entries
            .get(&block.id)
            .is_some_and(|root| root.block != *block)
        {
            cache.remove(&block.id);
        }
        let hit = cache
            .entries
            .get_mut(&block.id)
            .and_then(|root| root.variants.get_mut(&width_key));
        let (root, global_len) = if let Some(entry) = hit {
            cache.lru.remove(&entry.last_used);
            entry.last_used = clock;
            cache.lru.insert(clock, (block.id.clone(), width_key));
            cache.hits += 1;
            (entry.layout.clone(), entry.global_len)
        } else {
            cache.misses += 1;
            let (layout, global_len) = shape_root(block, width, theme, typography, window);
            let shaped = Rc::new(layout);
            let estimated_bytes = estimate(block, std::iter::once(shaped.as_ref()));
            if estimated_bytes <= MAX_LAYOUT_CACHE_ESTIMATED_BYTES {
                let root = cache
                    .entries
                    .entry(block.id.clone())
                    .or_insert_with(|| Root {
                        block: block.clone(),
                        variants: HashMap::new(),
                        estimated_bytes: 0,
                    });
                let before = root.estimated_bytes;
                root.variants.insert(
                    width_key,
                    Variant {
                        layout: shaped.clone(),
                        global_len,
                        last_used: clock,
                    },
                );
                root.estimated_bytes = estimate(
                    &root.block,
                    root.variants.values().map(|v| v.layout.as_ref()),
                );
                cache.lru.insert(clock, (block.id.clone(), width_key));
                cache.estimated_bytes =
                    cache.estimated_bytes.saturating_sub(before) + root.estimated_bytes;
                cache.bound(&block.id);
            }
            (shaped, global_len)
        };
        let label = if block.kind == "list" {
            let indent = block.number("indent", 0).min(16);
            ordered[indent] = if block.string("list") == "ordered" {
                ordered[indent] + 1
            } else {
                0
            };
            ordered[indent + 1..].fill(0);
            (block.string("list") == "ordered").then(|| format!("{}.", ordered[indent]))
        } else {
            ordered.fill(0);
            None
        };
        let y = out.height;
        out.lines.extend(root.lines.iter().map(|line| Line {
            line: line.line.clone(),
            origin: line.origin + point(px(0.), y),
            height: line.height,
            start: line.start + global,
        }));
        out.objects.extend(root.objects.iter().map(|object| {
            let mut object = object.clone();
            object.start += global;
            object.bounds.origin.y += y;
            object
        }));
        out.checks
            .extend(root.checks.iter().map(|(id, bounds, checked)| {
                let mut bounds = *bounds;
                bounds.origin.y += y;
                (id.clone(), bounds, *checked)
            }));
        out.borders.extend(root.borders.iter().map(|bounds| {
            let mut bounds = *bounds;
            bounds.origin.y += y;
            bounds
        }));
        out.quotes.extend(root.quotes.iter().map(|bounds| {
            let mut bounds = *bounds;
            bounds.origin.y += y;
            bounds
        }));
        out.labels.extend(
            root.labels
                .iter()
                .enumerate()
                .map(|(index, (text, origin))| {
                    (
                        if index == 0 {
                            label.clone().unwrap_or_else(|| text.clone())
                        } else {
                            text.clone()
                        },
                        *origin + point(px(0.), y),
                    )
                }),
        );
        out.height += root.height;
        global += global_len;
    }
    out.text_len = global.saturating_sub(1);
    out.height = out.height.max(px(220.));
    out
}
#[cfg(test)]
pub(super) fn make_layout(
    document: &Document,
    width: Pixels,
    theme: Theme,
    typography: WritingStyle,
    window: &Window,
) -> Layout {
    make_layout_cached(
        document,
        width,
        theme,
        typography,
        window,
        &mut ShapeCache::default(),
    )
}

fn shape_root(
    block: &Block,
    width: Pixels,
    theme: Theme,
    typography: WritingStyle,
    window: &Window,
) -> (Layout, usize) {
    let mut layout = Layout {
        width,
        text_len: 0,
        objects: vec![],
        height: px(0.),
        lines: vec![],
        checks: vec![],
        borders: vec![],
        quotes: vec![],
        labels: vec![],
    };
    fn layout_blocks(
        blocks: &[Block],
        origin: Point<Pixels>,
        width: Pixels,
        global: &mut usize,
        out: &mut Layout,
        style: (Theme, WritingStyle),
        w: &Window,
    ) -> Pixels {
        let (theme, typography) = style;
        let x = origin.x;
        let mut y = origin.y;
        let mut ordered = [0usize; 17];
        for block in blocks {
            if block.kind == "list" {
                let indent = block.number("indent", 0).min(16);
                ordered[indent] = if block.string("list") == "ordered" {
                    ordered[indent] + 1
                } else {
                    0
                };
                ordered[indent + 1..].fill(0);
            } else {
                ordered.fill(0);
            }
            if block.kind == "table" {
                let columns = block.number("columns", 1);
                let rows = block.number("rows", 1);
                let cell_w = width / columns as f32;
                let mut row_heights = vec![px(42.); rows];
                let mut cells = vec![];
                for cell in &block.cells {
                    let start = (
                        out.lines.len(),
                        out.checks.len(),
                        out.borders.len(),
                        out.labels.len(),
                        out.objects.len(),
                    );
                    let col_span = cell
                        .attrs
                        .get("colSpan")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as usize;
                    let row_span = cell
                        .attrs
                        .get("rowSpan")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as usize;
                    let cell_x = x + cell_w * cell.col as f32;
                    let h = layout_blocks(
                        &cell.blocks,
                        point(cell_x + px(12.), px(12.)),
                        (cell_w * col_span as f32 - px(24.)).max(px(20.)),
                        global,
                        out,
                        style,
                        w,
                    ) + px(4.);
                    for height in &mut row_heights[cell.row..cell.row + row_span] {
                        *height = (*height).max(h / row_span as f32);
                    }
                    cells.push((
                        cell.row,
                        col_span,
                        row_span,
                        cell_x,
                        start,
                        (
                            out.lines.len(),
                            out.checks.len(),
                            out.borders.len(),
                            out.labels.len(),
                            out.objects.len(),
                        ),
                    ));
                }
                for (row, col_span, row_span, cell_x, start, end) in cells {
                    let top = y + row_heights[..row].iter().copied().sum::<Pixels>();
                    for l in &mut out.lines[start.0..end.0] {
                        l.origin.y += top;
                    }
                    for (_, b, _) in &mut out.checks[start.1..end.1] {
                        b.origin.y += top;
                    }
                    for object in &mut out.objects[start.4..end.4] {
                        object.bounds.origin.y += top;
                    }
                    for b in &mut out.borders[start.2..end.2] {
                        b.origin.y += top;
                    }
                    for (_, p) in &mut out.labels[start.3..end.3] {
                        p.y += top;
                    }
                    out.borders.push(Bounds::new(
                        point(cell_x, top),
                        size(
                            cell_w * col_span as f32,
                            row_heights[row..row + row_span].iter().copied().sum(),
                        ),
                    ));
                }
                y += row_heights.iter().copied().sum::<Pixels>() + px(22.);
                continue;
            }
            let quote_top = y;
            let indent = if block.kind == "quote" {
                px(24.)
            } else if block.kind == "list" {
                px(30. + block.number("indent", 0) as f32 * 22.)
            } else {
                px(0.)
            };
            let size = (if block.kind == "heading" {
                match block.number("level", 1) {
                    1 => 32.,
                    2 => 26.,
                    _ => 22.,
                }
            } else {
                19.
            }) * typography.size as f32
                / 19.;
            let height = px(size * 1.55);
            if block.kind == "list" {
                if block.string("list") == "check" {
                    out.checks.push((
                        block.id.clone(),
                        Bounds::new(
                            point(x + indent - px(28.), y + px(6.)),
                            size_fn(px(19.), px(19.)),
                        ),
                        block.checked(),
                    ));
                } else {
                    out.labels.push((
                        if block.string("list") == "ordered" {
                            format!("{}.", ordered[block.number("indent", 0).min(16)])
                        } else {
                            "•".into()
                        },
                        point(x + indent - px(24.), y),
                    ));
                }
            }
            let mut segments: Vec<Vec<notes_core::document::Span>> = vec![];
            let mut text_spans = vec![];
            for span in &block.spans {
                if span.is_text() {
                    text_spans.push(span.clone());
                } else {
                    if !text_spans.is_empty() {
                        segments.push(std::mem::take(&mut text_spans));
                    }
                    segments.push(vec![span.clone()]);
                }
            }
            if !text_spans.is_empty() || segments.is_empty() {
                segments.push(text_spans);
            }
            for spans in segments {
                if spans.first().is_some_and(|s| !s.is_text()) {
                    let span = &spans[0];
                    let is_image = span.0.get("image").is_some();
                    let key = if is_image {
                        format!("image:{}", span.0["image"].as_str().unwrap_or(""))
                    } else {
                        format!("attachment:{}", span.0["attachment"].as_str().unwrap_or(""))
                    };
                    let available = (width - indent).max(px(40.));
                    let (w, h) = if is_image {
                        let native_w = span
                            .0
                            .get("width")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(640.)
                            .clamp(1., 12000.) as f32;
                        let native_h = span
                            .0
                            .get("height")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(360.)
                            .clamp(1., 12000.) as f32;
                        let scale = (f32::from(available) / native_w)
                            .min(360. / native_h)
                            .min(1.);
                        (
                            px(native_w * scale).max(px(40.)),
                            px(native_h * scale).max(px(40.)),
                        )
                    } else {
                        (available.min(px(520.)), px(74.))
                    };
                    y += px(8.);
                    out.objects.push(Object {
                        key,
                        start: *global,
                        bounds: Bounds::new(point(x + indent, y), size_fn(w, h)),
                        image: is_image,
                        label: span
                            .0
                            .get("alt")
                            .and_then(|v| v.as_str())
                            .unwrap_or(if is_image { "Image" } else { "Attachment" })
                            .to_owned(),
                    });
                    y += h + px(12.);
                    *global += 3;
                    continue;
                }
                let text: String = spans.iter().map(|s| s.content()).collect();
                let mut runs = vec![];
                for span in &spans {
                    let mut font = font(if span.mark("code") {
                        "monospace"
                    } else {
                        typography.font.family()
                    });
                    if span.mark("bold") || block.kind == "heading" {
                        font.weight = FontWeight::BOLD;
                    }
                    if span.mark("italic") {
                        font.style = FontStyle::Italic;
                    }
                    let link = span.0.get("href").is_some();
                    runs.push(TextRun {
                        len: span.content().len(),
                        font,
                        color: if link { theme.accent } else { theme.text },
                        background_color: span.mark("highlight").then_some(theme.selected),
                        underline: (span.mark("underline") || link).then_some(UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: false,
                        }),
                        strikethrough: span.mark("strikethrough").then_some(StrikethroughStyle {
                            thickness: px(1.),
                            color: None,
                        }),
                    });
                }
                let display = if text.is_empty() {
                    " ".to_owned()
                } else {
                    text.clone()
                };
                if runs.is_empty() {
                    runs.push(TextRun {
                        len: display.len(),
                        font: font(typography.font.family()),
                        color: theme.text,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                let lines = w
                    .text_system()
                    .shape_text(
                        display.into(),
                        px(size),
                        &runs,
                        Some((width - indent).max(px(20.))),
                        None,
                    )
                    .unwrap_or_default();
                let mut local = 0;
                for line in lines {
                    let h = line.size(height).height;
                    let len = line.len();
                    out.lines.push(Line {
                        line: Rc::new(line),
                        origin: point(x + indent, y),
                        height,
                        start: *global + local,
                    });
                    y += h;
                    local += len + 1;
                }
                *global += text.len();
            }
            *global += 1;
            if block.kind == "quote" {
                out.quotes.push(Bounds::new(
                    point(x + px(2.), quote_top + px(3.)),
                    size_fn(px(2.), (y - quote_top - px(6.)).max(px(18.))),
                ));
            }
            y += if block.kind == "list" {
                px(4.)
            } else {
                px(16.)
            };
        }
        y
    }
    let mut global = 0;
    layout.height = layout_blocks(
        std::slice::from_ref(block),
        point(px(0.), px(0.)),
        width,
        &mut global,
        &mut layout,
        (theme, typography),
        window,
    );
    layout.text_len = global.saturating_sub(1);
    (layout, global)
}
