//! Real-font shaping-cache and masked-paint regression, with full model geometry.
use super::*;
use std::{path::Path, sync::Arc};

fn draw(cx: &mut HeadlessAppContext, h: WindowHandle<RichEditor>) -> anyhow::Result<()> {
    cx.run_until_parked();
    let cleanup = cx.update_window(h.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx)
    })?;
    cx.update(|cx| cleanup.clear(cx));
    cx.run_until_parked();
    Ok(())
}
fn same_geometry(a: &Layout, b: &Layout) {
    assert_eq!(a.height, b.height);
    assert_eq!(a.text_len, b.text_len);
    assert_eq!(a.lines.len(), b.lines.len());
    for (a, b) in a.lines.iter().zip(&b.lines) {
        assert_eq!(
            (a.origin, a.height, a.start, a.line.len()),
            (b.origin, b.height, b.start, b.line.len())
        );
    }
    assert_eq!(a.checks, b.checks);
    assert_eq!(a.borders, b.borders);
    assert_eq!(a.quotes, b.quotes);
    assert_eq!(a.labels, b.labels);
    assert_eq!(a.objects.len(), b.objects.len());
    for (a, b) in a.objects.iter().zip(&b.objects) {
        assert_eq!(
            (&a.key, a.start, a.bounds, a.image),
            (&b.key, b.start, b.bounds, b.image)
        );
    }
}
fn fresh_geometry(e: &RichEditor, w: &Window) {
    let cached = e.layout.as_ref().unwrap();
    let fresh = make_layout_cached(
        &e.model.document,
        cached.width,
        e.theme,
        e.typography,
        w,
        &mut ShapeCache::default(),
    );
    same_geometry(cached, &fresh);
    assert_eq!(cached.text_len, e.model.text().len());
    let usage = e.layout_diagnostics();
    assert!(usage.cache_roots <= shaped::MAX_LAYOUT_CACHE_ROOTS);
    assert!(usage.cache_estimated_bytes <= shaped::MAX_LAYOUT_CACHE_ESTIMATED_BYTES);
    assert!(usage.geometry_entries <= shaped::MAX_GEOMETRY_WIDTHS);
    assert!(usage.geometry_estimated_bytes <= shaped::MAX_GEOMETRY_ESTIMATED_BYTES);
}

fn glyph_probes(e: &RichEditor) -> serde_json::Value {
    let mut probes = vec![];
    for line in &e.layout.as_ref().unwrap().lines {
        let text = line.line.text.as_ref();
        if !["Emoji probe", "Combining probe", "CJK probe", "RTL probe"]
            .iter()
            .any(|prefix| text.starts_with(prefix))
        {
            continue;
        }
        let missing: Vec<_> = line.line.unwrapped_layout.runs.iter().flat_map(|run| run.glyphs.iter().filter(|g| g.id.0 == 0).map(move |glyph| {
            serde_json::json!({"glyph_id":glyph.id.0,"font_id":run.font_id.0,"byte_index":glyph.index,"scalar":text.get(glyph.index..).and_then(|s|s.chars().next()).map(|c|c.to_string())})
        })).collect();
        probes.push(serde_json::json!({"text":text,"glyphs":line.line.unwrapped_layout.runs.iter().map(|r|r.glyphs.len()).sum::<usize>(),"glyph_zero":missing}));
    }
    serde_json::json!({"font":format!("{:?}",e.typography.font),"size":e.typography.size,"probes":probes,"interpretation":"Glyph ID 0 records missing-glyph evidence for these shaped runs; this is not native font fallback or script-language input acceptance evidence."})
}
struct ResizeView {
    editor: Entity<RichEditor>,
    viewport: Size<Pixels>,
}
impl Render for ResizeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .w(self.viewport.width)
                .h(self.viewport.height)
                .child(self.editor.clone()),
        )
    }
}
fn resize_composition_workflow(cx: &mut HeadlessAppContext) -> anyhow::Result<()> {
    let h = cx.open_window(size(px(1024.), px(768.)), |w, cx| {
        cx.new(|cx| ResizeView {
            editor: cx.new(|cx| {
                RichEditor::new(
                    Document {
                        blocks: (0..80)
                            .map(|i| {
                                Block::paragraph(&format!(
                                    "A paragraph {i} to keep the composing caret in view."
                                ))
                            })
                            .collect(),
                        ..Default::default()
                    },
                    Theme::reference(),
                    "resize-check".into(),
                    w,
                    cx,
                )
            }),
            viewport: size(px(800.), px(700.)),
        })
    })?;
    fn draw(cx: &mut HeadlessAppContext, h: WindowHandle<ResizeView>) -> anyhow::Result<()> {
        cx.run_until_parked();
        let cleanup = cx.update_window(h.into(), |_, w, cx| {
            w.refresh();
            w.draw(cx)
        })?;
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        Ok(())
    }
    draw(cx, h)?;
    h.update(cx, |view, w, cx| {
        view.editor.update(cx, |e, cx| {
            e.edit(
                |m| {
                    m.select(m.text().len(), false);
                    Ok(())
                },
                w,
                cx,
            );
            w.focus(&e.focus, cx);
            e.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx);
        })
    })?;
    draw(cx, h)?;
    draw(cx, h)?;
    for size in [size(px(640.), px(480.)), size(px(640.), px(360.))] {
        h.update(cx, |view, _, cx| {
            view.viewport = size;
            cx.notify();
        })?;
        draw(cx, h)?;
        draw(cx, h)?;
        h.update(cx, |view, _, cx| {
            let e = view.editor.read(cx);
            assert!(e.model.marked.is_some());
            let (point, height) = e.layout.as_ref().unwrap().caret(e.model.global(&e.model.selection.head));
            let caret = Bounds::new(e.bounds.origin + point, size_fn(px(1.), height));
            let viewport = e.scroll.bounds();
            assert!(caret.top() >= viewport.top() && caret.bottom() <= viewport.bottom(), "composing caret must remain visible after width/height or height-only resize: {caret:?} in {viewport:?}");
        })?;
    }
    h.update(cx, |view, w, cx| {
        view.editor.update(cx, |e, cx| e.unmark_text(w, cx))
    })?;
    draw(cx, h)?;
    draw(cx, h)?;
    h.update(cx, |view, _, cx| {
        view.editor.update(cx, |e, cx| {
            e.scroll.set_offset(point(px(0.), px(-100.)));
            e.reveal = false;
            cx.notify();
        })
    })?;
    draw(cx, h)?;
    let reading = h.update(cx, |view, _, cx| view.editor.read(cx).scroll.offset())?;
    h.update(cx, |view, _, cx| {
        view.viewport = size(px(640.), px(420.));
        cx.notify();
    })?;
    draw(cx, h)?;
    draw(cx, h)?;
    h.update(cx, |view, _, cx| {
        assert_eq!(
            view.editor.read(cx).scroll.offset(),
            reading,
            "ordinary reading resize must not jump to an offscreen caret"
        )
    })?;
    h.update(cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    Ok(())
}

pub fn layout_cache_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut body = Document {
        blocks: vec![],
        ..Default::default()
    };
    for paragraph in 0..500 {
        body.blocks.push(Block::paragraph(&format!(
            "Paragraph {paragraph}. A quiet place keeps its words and geometry together."
        )));
    }
    let mut list = vec![];
    for number in 0..3 {
        let mut block = Block::paragraph(&format!("Ordered thought {number}"));
        block.set_kind("ordered");
        list.push(block.id.clone());
        body.blocks.push(block);
    }
    let mut check = Block::paragraph("Keep the checklist reachable");
    check.set_kind("check");
    let check_id = check.id.clone();
    body.blocks.push(check);
    let fixture =
        Document::from_json(include_str!("../../../tests/fixtures/qt-merged-table.json"))?;
    let table = fixture
        .blocks
        .iter()
        .find(|b| b.kind == "table")
        .unwrap()
        .clone();
    let cell_id = table.cells[0].blocks[0].id.clone();
    body.blocks.push(table);
    body.blocks
        .push(Block::paragraph("é 👩‍👩‍👧‍👦 中文 العربية mixed RTL and LTR"));
    for probe in [
        "Emoji probe 👩‍👩‍👧‍👦 🙂 🇺🇸",
        "Combining probe é ö",
        "CJK probe 日本語 中文",
        "RTL probe العربية שלום mixed Latin",
    ] {
        body.blocks.push(Block::paragraph(probe));
    }
    let first = body.blocks[0].id.clone();
    let stable = body.blocks[200].id.clone();
    let roots = body.blocks.len();
    let scratch = tempfile::tempdir()?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::assets::load_fonts(cx).unwrap();
        bind_keys(cx);
    });
    let h = cx.open_window(size(px(1024.), px(768.)), |w, cx| {
        cx.new(|cx| {
            RichEditor::new(
                body,
                Theme::reference(),
                scratch.path().to_string_lossy().into_owned(),
                w,
                cx,
            )
        })
    })?;
    draw(&mut cx, h)?;
    let serif_probe = h.update(&mut cx, |e, _, _| glyph_probes(e))?;
    let (before, retained) = h.update(&mut cx, |e, w, _| {
        fresh_geometry(e, w);
        let usage = e.layout_diagnostics();
        assert!(
            usage.shape_misses >= roots as u64,
            "initial width negotiation may shape several configurations"
        );
        assert_eq!(usage.cache_roots, roots);
        assert!(
            usage.painted_lines < 40 && usage.total_lines > 500,
            "only viewport text is painted: {usage:?}"
        );
        let start = e.model.global(&notes_editor::Position {
            block: stable.clone(),
            offset: 0,
        });
        let line = e
            .layout
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .find(|l| l.start == start)
            .unwrap()
            .line
            .clone();
        (usage, line)
    })?;
    h.update(&mut cx, |e, w, cx| {
        e.edit(
            |m| {
                m.select(
                    m.global(&notes_editor::Position {
                        block: first.clone(),
                        offset: 0,
                    }),
                    false,
                );
                m.insert("New words. ", 0)
            },
            w,
            cx,
        )
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |e, w, _| {
        fresh_geometry(e, w);
        let usage = e.layout_diagnostics();
        assert!(
            (1..=shaped::MAX_LAYOUT_WIDTH_VARIANTS as u64)
                .contains(&(usage.shape_misses - before.shape_misses)),
            "only the changed root reshapes across negotiated widths: {usage:?}"
        );
        assert!(usage.shape_hits - before.shape_hits >= roots as u64 - 1);
        let start = e.model.global(&notes_editor::Position {
            block: stable.clone(),
            offset: 0,
        });
        let line = &e
            .layout
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .find(|l| l.start == start)
            .unwrap()
            .line;
        assert!(
            Rc::ptr_eq(&retained, line),
            "unchanged glyph geometry is reused with updated global offsets"
        );
    })?;
    // Mark changes, checklist state, ordered-list context and merged-cell edits.
    for (id, operation) in [
        (first.clone(), 0),
        (check_id, 1),
        (list[0].clone(), 2),
        (cell_id, 3),
    ] {
        let before = h.update(&mut cx, |e, _, _| e.layout_diagnostics())?;
        h.update(&mut cx, |e, w, cx| {
            e.edit(
                |m| {
                    let start = m.global(&notes_editor::Position {
                        block: id.clone(),
                        offset: 0,
                    });
                    m.select(start, false);
                    match operation {
                        0 => {
                            m.select(start + 3, true);
                            m.mark("bold")
                        }
                        1 => {
                            m.toggle_check(&id);
                            Ok(())
                        }
                        2 => m.style("bullet"),
                        _ => m.insert("Cell edit. ", 0),
                    }
                },
                w,
                cx,
            )
        })?;
        draw(&mut cx, h)?;
        h.update(&mut cx, |e, w, _| {
            fresh_geometry(e, w);
            assert!(
                (1..=shaped::MAX_LAYOUT_WIDTH_VARIANTS as u64)
                    .contains(&(e.layout_diagnostics().shape_misses - before.shape_misses))
            );
        })?;
    }
    // Width and typography/theme invalidate all root shapes, then stabilize.
    h.update(&mut cx, |e, w, _| {
        for width in [px(320.), px(720.)] {
            let before = e.shape_cache.diagnostics();
            let cached = make_layout_cached(
                &e.model.document,
                width,
                e.theme,
                e.typography,
                w,
                &mut e.shape_cache,
            );
            let fresh = make_layout_cached(
                &e.model.document,
                width,
                e.theme,
                e.typography,
                w,
                &mut ShapeCache::default(),
            );
            same_geometry(&cached, &fresh);
            assert_eq!(
                e.shape_cache.diagnostics().shape_misses - before.shape_misses,
                roots as u64
            );
        }
    })?;
    h.update(&mut cx, |e, _, cx| {
        e.set_typography(
            WritingStyle {
                font: notes_core::preferences::NoteFont::Sans,
                size: 24,
            },
            cx,
        );
        e.set_theme(Theme::light(), cx);
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |e, w, _| fresh_geometry(e, w))?;
    let sans_probe = h.update(&mut cx, |e, _, _| glyph_probes(e))?;
    std::fs::write(
        directory.join("glyph-probes.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"serif":serif_probe,"sans":sans_probe}))?,
    )?;
    let before_scroll = h.update(&mut cx, |e, _, cx| {
        e.scroll.set_offset(point(px(0.), px(-8_000.)));
        cx.notify();
        e.layout_diagnostics()
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |e, w, _| {
        fresh_geometry(e, w);
        let usage = e.layout_diagnostics();
        assert_eq!(
            usage.shape_misses, before_scroll.shape_misses,
            "scrolling does not reshape"
        );
        assert_eq!(
            usage.shape_hits, before_scroll.shape_hits,
            "unchanged width probes reuse assembled geometry without walking root shapes"
        );
        assert_eq!(
            usage.geometry_misses, before_scroll.geometry_misses,
            "scrolling does not reassemble complete geometry"
        );
        assert!(usage.painted_lines < 40 && usage.total_lines > 500);
        e.model.select_all();
        assert_eq!(
            Document {
                schema: e.model.document.schema,
                blocks: e.model.fragment().unwrap(),
                ..Default::default()
            }
            .plain_text(),
            e.model.document.plain_text(),
            "offscreen selection retains the complete document"
        );
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("cached-scrolled.png"))?;
    h.update(&mut cx, |e, w, _| {
        for width in (200..=400).step_by(20) {
            e.ensure_layout(px(width as f32), w);
            fresh_geometry(e, w);
        }
        assert_eq!(
            e.layout_diagnostics().geometry_entries,
            shaped::MAX_GEOMETRY_WIDTHS
        );
    })?;
    // Force count-based eviction and then remove stale identities. Geometry and
    // model text remain complete even when a shape is outside the cache budget.
    h.update(&mut cx, |e, _, cx| {
        e.model = Editor::new(Document {
            blocks: (0..=shaped::MAX_LAYOUT_CACHE_ROOTS)
                .map(|i| Block::paragraph(&format!("Bounded root {i}")))
                .collect(),
            ..Default::default()
        });
        e.layout = None;
        cx.notify();
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |e, w, _| {
        fresh_geometry(e, w);
        let usage = e.layout_diagnostics();
        assert_eq!(usage.total_lines, shaped::MAX_LAYOUT_CACHE_ROOTS + 1);
        assert_eq!(usage.cache_roots, shaped::MAX_LAYOUT_CACHE_ROOTS);
        assert!(
            usage.cache_entries
                <= shaped::MAX_LAYOUT_CACHE_ROOTS * shaped::MAX_LAYOUT_WIDTH_VARIANTS
        );
        assert!(usage.cache_evictions > 0);
    })?;
    h.update(&mut cx, |e, w, cx| {
        e.edit(
            |m| {
                m.select_all();
                m.insert("A smaller document", 0)
            },
            w,
            cx,
        )
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |e, w, _| {
        fresh_geometry(e, w);
        assert_eq!(e.layout_diagnostics().cache_roots, 1);
        assert!(
            e.layout_diagnostics().geometry_entries < shaped::MAX_GEOMETRY_WIDTHS,
            "document invalidation clears the previous complete-geometry variants"
        );
    })?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    resize_composition_workflow(&mut cx)?;
    println!(
        "Passed real-font root-shape reuse, content/mark/check/list/merged-table invalidation, width/font/theme invalidation, masked scrolling, complete offscreen selection, 4,096-root eviction, stale-root cleanup and composing width/height resize with ordinary reading preserved. The 64 MiB accounting is an estimate, excluding GPUI/native/GPU shared allocations. Images: {}",
        directory.display()
    );
    Ok(())
}
