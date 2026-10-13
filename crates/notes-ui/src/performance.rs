//! Opt-in diagnostics. Fixtures and reports are isolated from the user's notebook.
//! Headless `Window::draw` builds a scene; only native profiler samples include
//! platform submission. Neither path measures display presentation.
use super::*;
use crate::rich_editor::{Changed, EditorRegions, RegionStats, RichEditor};
use anyhow::{Context as _, Result, ensure};
use notes_core::document::{Block as RichBlock, Cell, Document, Span};
use notes_storage::{Store, Worker};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
#[path = "foreground_queue_probe.rs"]
mod queue_probe;
use queue_probe::{QueueProbe, QueueProbeConfig};

const NOTE_COUNT: usize = 10_000;
const SEED: u64 = 20260923;

struct Fixture {
    root: PathBuf,
    typical: Vec<String>,
    large: String,
    long: String,
    body_bytes: usize,
    creation_ms: f64,
}

fn block_id(note: usize, block: usize) -> String {
    format!("00000000-0000-4000-8000-{:012x}", note * 100_000 + block)
}
fn paragraph(note: usize, block: usize, text: &str) -> RichBlock {
    let mut p = RichBlock::paragraph(text);
    p.id = block_id(note, block);
    p
}
fn document(blocks: Vec<RichBlock>) -> Document {
    Document {
        schema: 1,
        blocks,
        extra: BTreeMap::new(),
    }
}
fn large_document() -> Document {
    let mut blocks = vec![];
    for i in 0..3000 {
        let mut p = paragraph(
            NOTE_COUNT,
            i,
            &format!("Paragraph {i} of a long research note with "),
        );
        let mut emphasis = Span::text("some emphasis");
        emphasis.set_mark("italic", true);
        p.spans.extend([
            emphasis,
            Span::text(" and enough text to wrap across the column."),
        ]);
        blocks.push(p);
        if i % 150 == 0 {
            let cells = (0..3)
                .flat_map(|row| {
                    (0..3).map(move |col| Cell {
                        row,
                        col,
                        blocks: vec![paragraph(NOTE_COUNT + 1, i * 10 + row * 3 + col, "cell")],
                        attrs: BTreeMap::new(),
                    })
                })
                .collect();
            blocks.push(RichBlock {
                id: block_id(NOTE_COUNT + 2, i),
                kind: "table".into(),
                spans: vec![],
                cells,
                attrs: BTreeMap::from([("rows".into(), json!(3)), ("columns".into(), json!(3))]),
            });
        }
    }
    document(blocks)
}
fn fixture(root: PathBuf) -> Result<Fixture> {
    let started = Instant::now();
    let mut store = Store::open(&root)?;
    let mut typical = vec![];
    let mut body_bytes = 0;
    let words = [
        "meeting",
        "roadmap",
        "budget",
        "customer",
        "release",
        "design",
        "search",
        "planning",
        "research",
        "migration",
        "sqlite",
        "wayland",
        "lisbon",
        "kyoto",
    ];
    let mut state = SEED;
    for i in 0..NOTE_COUNT - 2 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let count = 3 + (state % 13) as usize;
        let mut blocks = vec![paragraph(i, 0, &format!("Synthetic research note {i:05}"))];
        blocks[0].kind = "heading".into();
        blocks[0].attrs.insert("level".into(), json!(1));
        for j in 0..count {
            let mut sentence = String::new();
            for _ in 0..24 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                sentence.push_str(words[(state % words.len() as u64) as usize]);
                sentence.push(' ');
            }
            sentence.push_str("é 👩‍👩‍👧‍👦 日本語 العربية.");
            blocks.push(paragraph(i, j + 1, &sentence));
        }
        if i % 5 == 0 {
            for j in 0..4 {
                let mut b = paragraph(i, 30 + j, "Capture and revisit this synthetic thought");
                b.kind = "list".into();
                b.attrs.extend([
                    ("list".into(), json!("check")),
                    ("checked".into(), json!(j % 2 == 0)),
                    ("indent".into(), json!(0)),
                ]);
                blocks.push(b);
            }
        }
        if i % 7 == 0 {
            blocks.push(paragraph(i, 40, "#research #release"));
        }
        let body = document(blocks);
        body_bytes += serde_json::to_vec(&body)?.len();
        let note = store.create(body, None)?;
        if typical.len() < 100 {
            typical.push(note.id);
        }
    }
    let body = large_document();
    body_bytes += serde_json::to_vec(&body)?.len();
    let large = store.create(body, None)?.id;
    let body = document(vec![paragraph(
        NOTE_COUNT + 3,
        0,
        &format!(
            "https://example.invalid/{} é 日本語 العربية",
            "long-token".repeat(25_000)
        ),
    )]);
    body_bytes += serde_json::to_vec(&body)?.len();
    let long = store.create(body, None)?.id;
    Ok(Fixture {
        root,
        typical,
        large,
        long,
        body_bytes,
        creation_ms: ms(started.elapsed()),
    })
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}
fn rss_bytes() -> Result<u64> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let line = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .context("VmRSS unavailable")?;
    Ok(line
        .split_whitespace()
        .nth(1)
        .context("VmRSS missing value")?
        .parse::<u64>()?
        * 1024)
}
fn samples(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let total: f64 = values.iter().sum();
    let percentile = |p: f64| {
        sorted
            .get(((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1))
            .copied()
    };
    json!({"samples": sorted.len(), "total_ms":total,"mean_ms":(!values.is_empty()).then(||total/values.len() as f64),"p50_ms": percentile(0.5), "p95_ms": percentile(0.95), "p99_ms": percentile(0.99), "max_ms": sorted.last(),"sample_values_ms":values})
}
fn session_has_uncommitted_work(session: &writing::Session, cx: &App) -> bool {
    session.pending.is_some()
        || session.revision != session.saved
        || session.error.is_some()
        || session.needs_upgrade
        || session.has_unobserved_edit(cx)
        || session.input_is_composing(cx)
        || session.has_pending_work(cx)
        || session
            .active_editor()
            .is_some_and(|editor| editor.read(cx).error.is_some())
}

fn usage(s: &Shell, cx: &App) -> Value {
    let live = s.live.as_ref().unwrap();
    let mut editors = 0;
    let mut history_bytes = 0;
    let mut undo_steps = 0;
    let mut redo_steps = 0;
    let mut limited = 0;
    let mut layout_roots = 0;
    let mut layout_entries = 0;
    let mut layout_bytes = 0;
    let mut geometry_entries = 0;
    let mut geometry_bytes = 0;
    let mut geometry_hits = 0;
    let mut geometry_misses = 0;
    let mut shape_hits = 0;
    let mut shape_misses = 0;
    let mut cache_evictions = 0;
    let mut regions = EditorRegions::default();
    let pending_sessions = live
        .notes
        .values()
        .filter(|n| session_has_uncommitted_work(n, cx))
        .count();
    for session in live.notes.values() {
        if let Some(editor) = session.active_editor() {
            editors += 1;
            let history = editor.read(cx).model.history_usage();
            history_bytes += history.allocated_bytes;
            undo_steps += history.undo_steps;
            redo_steps += history.redo_steps;
            limited += usize::from(history.limit_reached);
            let layout = editor.read(cx).layout_diagnostics();
            layout_roots += layout.cache_roots;
            layout_entries += layout.cache_entries;
            layout_bytes += layout.cache_estimated_bytes;
            shape_hits += layout.shape_hits;
            shape_misses += layout.shape_misses;
            cache_evictions += layout.cache_evictions;
            geometry_entries += layout.geometry_entries;
            geometry_bytes += layout.geometry_estimated_bytes;
            geometry_hits += layout.geometry_hits;
            geometry_misses += layout.geometry_misses;
            add_regions(&mut regions, editor.read(cx).performance_regions());
        }
    }
    json!({"library_notes": live.notes.len(), "session_owned_editors": editors,"pending_save_or_media_sessions":pending_sessions, "history_owned_heap_bytes": history_bytes, "undo_steps": undo_steps, "redo_steps": redo_steps, "history_limited_editors": limited, "open_windows": cx.windows().len(),"retained_editors_layout_cache_roots":layout_roots,"retained_editors_layout_width_entries":layout_entries,"retained_editors_layout_cache_estimated_bytes":layout_bytes,"retained_editors_shape_hits":shape_hits,"retained_editors_shape_misses":shape_misses,"retained_editors_cache_evictions":cache_evictions,"retained_editors_geometry_entries":geometry_entries,"retained_editors_geometry_estimated_bytes":geometry_bytes,"retained_editors_geometry_hits":geometry_hits,"retained_editors_geometry_misses":geometry_misses,"retained_editors_region_totals":region_values(regions,EditorRegions::default())})
}
fn add_regions(sum: &mut EditorRegions, r: EditorRegions) {
    fn add(a: &mut RegionStats, b: RegionStats) {
        a.calls = a.calls.saturating_add(b.calls);
        a.total_ns = a.total_ns.saturating_add(b.total_ns);
        a.max_ns = a.max_ns.max(b.max_ns);
    }
    add(&mut sum.edit, r.edit);
    add(&mut sum.render, r.render);
    add(&mut sum.layout, r.layout);
    add(&mut sum.prepaint, r.prepaint);
    add(&mut sum.paint, r.paint);
    sum.a11y_active |= r.a11y_active;
}
fn region_values(end: EditorRegions, start: EditorRegions) -> Value {
    fn delta(end: RegionStats, start: RegionStats) -> Value {
        let calls = end.calls.saturating_sub(start.calls);
        let ns = end.total_ns.saturating_sub(start.total_ns);
        json!({"calls":calls,"total_ns":ns,"total_ms":ns as f64/1e6,"mean_ms":(calls>0).then(||ns as f64/1e6/calls as f64),"cumulative_max_ms":end.max_ns as f64/1e6})
    }
    json!({"edit":delta(end.edit,start.edit),"render":delta(end.render,start.render),"layout":delta(end.layout,start.layout),"prepaint":delta(end.prepaint,start.prepaint),"paint":delta(end.paint,start.paint),"a11y_active":end.a11y_active})
}
fn derived_values(s: &Shell) -> [u64; 5] {
    let d = &s.live.as_ref().unwrap().derived_timings;
    [
        d.changed_calls,
        d.changed_ns,
        d.snapshot_ns,
        d.row_ns,
        d.catalog_ns,
    ]
}
fn derived_delta(end: [u64; 5], start: [u64; 5]) -> Value {
    let values: Vec<_> = end
        .iter()
        .zip(start)
        .map(|(&a, b)| a.saturating_sub(b))
        .collect();
    let calls = values[0];
    let region = |ns: u64| json!({"total_ns":ns,"total_ms":ns as f64/1e6,"per_changed_call_ms":(calls>0).then(||ns as f64/1e6/calls as f64)});
    json!({"changed_calls":calls,"changed_inclusive":region(values[1]),"snapshot":region(values[2]),"row_inclusive":region(values[3]),"catalog":region(values[4]),"scope":"Changed callback includes snapshot and row; row includes catalog. These fields overlap and must not be summed."})
}
fn idle_tail(periodic: &[Value]) -> Value {
    let Some(end) = periodic.last() else {
        return json!({"observed_seconds":0,"unforced_draw_samples":null,"status":"unverified: no periodic samples"});
    };
    let stable = |sample: &Value| {
        sample["usage"]["pending_save_or_media_sessions"] == 0
            && sample["selected_editor"]["model_revision"]
                == end["selected_editor"]["model_revision"]
            && sample["selected_editor"]["synthetic_note_id"]
                == end["selected_editor"]["synthetic_note_id"]
            && sample["input_samples"] == end["input_samples"]
    };
    if !stable(end) {
        return json!({"observed_seconds":0,"unforced_draw_samples":null,"status":"unverified: pending work at sampling end"});
    }
    let mut first = periodic.len() - 1;
    while first > 0 && stable(&periodic[first - 1]) {
        first -= 1;
    }
    let start = &periodic[first];
    json!({"observed_seconds":end["second"].as_u64().unwrap_or(0).saturating_sub(start["second"].as_u64().unwrap_or(0)),"observed_elapsed_ms":end["actual_elapsed_ms"].as_f64().zip(start["actual_elapsed_ms"].as_f64()).map(|(a,b)|a-b),"unforced_draw_samples":end["draw_samples"].as_u64().unwrap_or(0).saturating_sub(start["draw_samples"].as_u64().unwrap_or(0)),"scope":"Final periodic span with unchanged selected model revision/input-sample count and no pending save/media sessions. No frames are forced by the sampler."})
}
struct NativePhaseCapture {
    input: gpui::profiler::InputLatencySnapshot,
    selected: Value,
    planned: Duration,
    timer_woke: Duration,
    captured: Duration,
}
fn selected_progress(s: &Shell, cx: &App) -> Value {
    s.library
        .selected
        .as_ref()
        .and_then(|id| {
            s.live.as_ref()?.notes.get(id)?.active_editor().map(|editor| {
                json!({"synthetic_note_id":id,"model_revision":editor.read(cx).model.revision})
            })
        })
        .unwrap_or(Value::Null)
}
fn selected_editor(s: &Shell, cx: &App) -> Value {
    s.library.selected.as_ref().and_then(|id| {
        s.live.as_ref()?.notes.get(id)?.active_editor().map(|editor| {
            let e = editor.read(cx);
            let layout = e.layout_diagnostics();
            let text = e.model.text();
            let collapsed = e.model.selection.anchor==e.model.selection.head;
            let caret = e.model.global(&e.model.selection.head);
            json!({"synthetic_note_id":id,"model_revision":e.model.revision,"root_paragraph_blocks":e.model.document.blocks.iter().filter(|b|b.kind=="paragraph").count(),"root_table_blocks":e.model.document.blocks.iter().filter(|b|b.kind=="table").count(),"text_utf8_bytes":text.len(),"leading_x_count":text.chars().take_while(|&c|c=='x').count(),"trailing_x_count":text.chars().rev().take_while(|&c|c=='x').count(),"selection_collapsed":collapsed,"caret_global_utf8":caret,"caret_at_end":collapsed&&caret==text.len(),"region_totals":region_values(e.performance_regions(),EditorRegions::default()),"layout":{"cache_roots":layout.cache_roots,"cache_width_entries":layout.cache_entries,"cache_estimated_bytes":layout.cache_estimated_bytes,"shape_hits":layout.shape_hits,"shape_misses":layout.shape_misses,"cache_evictions":layout.cache_evictions,"geometry_entries":layout.geometry_entries,"geometry_estimated_bytes":layout.geometry_estimated_bytes,"geometry_hits":layout.geometry_hits,"geometry_misses":layout.geometry_misses,"painted_lines":layout.painted_lines,"total_lines":layout.total_lines}})
        })
    }).unwrap_or(Value::Null)
}
fn environment(window: &Window) -> Value {
    json!({
        "scale_factor": window.scale_factor(),
        "actual_logical_viewport": [f32::from(window.viewport_size().width),f32::from(window.viewport_size().height)],
        "gpu": window.gpu_specs().map(|gpu| json!({"device_name":gpu.device_name,"driver_name":gpu.driver_name,"driver_info":gpu.driver_info,"software_emulated":gpu.is_software_emulated})),
        "session_type": std::env::var("XDG_SESSION_TYPE").ok(),
        "desktop": std::env::var("XDG_CURRENT_DESKTOP").ok(),
        "compositor_version": std::env::var("OMARCHY_PERFORMANCE_COMPOSITOR").ok(),
        "wayland_display_available": std::env::var_os("WAYLAND_DISPLAY").is_some()
    })
}
fn metadata(f: &Fixture, mode: &str) -> Value {
    let cpu = std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("model name"))
            .map(str::to_owned)
    });
    json!({"mode": mode, "source_revision": std::env::var("OMARCHY_PERFORMANCE_REVISION").unwrap_or_else(|_| "unrecorded".into()),"binary_sha256":std::env::var("OMARCHY_PERFORMANCE_BINARY_SHA256").ok(),"source_condition":std::env::var("OMARCHY_PERFORMANCE_SOURCE_CONDITION").unwrap_or_else(|_|"unrecorded; revision alone does not establish a clean source tree".into()), "profile": if cfg!(debug_assertions) { "debug" } else { "release" }, "gpui_pin": "a84689073d296dfd39987bc7dd478e43ef76d83a", "cpu": cpu, "logical_viewport": [1440,960], "fixture": {"seed": SEED, "notes": NOTE_COUNT, "body_json_bytes": f.body_bytes, "large_paragraphs": 3000, "large_tables": 20, "long_token_chars": 250_000, "media_corpus": "not included; 1 GB attachment gate remains unverified", "creation_ms_excluded": f.creation_ms}, "history_limit_bytes_per_editor": notes_editor::MAX_HISTORY_BYTES, "history_limit_steps": notes_editor::MAX_HISTORY_STEPS})
}
fn write_report(directory: &Path, name: &str, report: &Value) -> Result<()> {
    std::fs::write(directory.join(name), serde_json::to_vec_pretty(report)?)?;
    Ok(())
}
fn draw(cx: &mut HeadlessAppContext, h: AnyWindowHandle) -> Result<f64> {
    let started = Instant::now();
    let cleanup = h.update(cx, |_, w, cx| {
        w.refresh();
        w.draw(cx)
    })?;
    let elapsed = ms(started.elapsed());
    cx.update(|cx| cleanup.clear(cx));
    cx.run_until_parked();
    Ok(elapsed)
}
fn settle(cx: &mut HeadlessAppContext, shell: &Entity<Shell>, h: AnyWindowHandle) -> Result<()> {
    cx.update(|cx| shell.update(cx, |s, cx| s.save_real_notes(cx)));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let done = h.update(cx, |_, w, cx| {
            shell.update(cx, |s, cx| {
                s.poll_storage(w, cx);
                s.live.as_ref().unwrap().notes.values().all(|n| {
                    n.pending.is_none() && n.revision == n.saved && !n.has_pending_work(cx)
                })
            })
        })?;
        cx.run_until_parked();
        if done {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Timed out waiting for diagnostic draft commits"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn select(
    cx: &mut HeadlessAppContext,
    shell: &Entity<Shell>,
    h: AnyWindowHandle,
    id: &str,
) -> Result<Entity<RichEditor>> {
    h.update(cx, |_, w, cx| {
        shell.update(cx, |s, cx| {
            s.command(Command::Select(id.into()), w, cx);
            s.live.as_ref().unwrap().notes[id]
                .active_editor()
                .cloned()
                .context("Selection did not mount an editor")
        })
    })?
}
fn headless(directory: &Path, f: Fixture) -> Result<()> {
    let mut report = metadata(&f, "real-font headless CPU draw diagnostics");
    report["incomplete"] = json!(true);
    report["phase"] = json!("open");
    let initial_rss = rss_bytes()?;
    let started = Instant::now();
    let (worker, library) = Worker::start(f.root.clone(), false)?;
    report["storage_open_and_eager_load_ms"] = json!(ms(started.elapsed()));
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::assets::load_fonts(cx).unwrap();
        super::bind_keys(cx);
        crate::search::bind_keys(cx);
    });
    let started = Instant::now();
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| {
            Shell::new_live(
                Appearance::Reference,
                f.root.clone(),
                worker,
                library,
                w,
                cx,
            )
        })
    })?;
    let shell = h.root(&mut cx)?;
    let main: AnyWindowHandle = h.into();
    report["scale_factor"] = json!(main.update(&mut cx, |_, w, _| w.scale_factor())?);
    let editor = select(&mut cx, &shell, main, &f.typical[0])?;
    draw(&mut cx, main)?;
    report["shell_creation_to_first_cpu_draw_ms"] = json!(ms(started.elapsed()));
    let started = Instant::now();
    let screenshot = cx.capture_screenshot(main)?;
    report["gpu_raster_dimensions"] = json!(screenshot.dimensions());
    screenshot.save(directory.join("fixture.png"))?;
    report["gpu_render_and_readback_ms"] = json!(ms(started.elapsed()));
    report["initial_rss_bytes"] = json!(initial_rss);
    report["rss_after_open_bytes"] = json!(rss_bytes()?);
    report["usage_after_open"] = cx.update(|cx| usage(shell.read(cx), cx));
    write_report(directory, "headless.partial.json", &report)?;
    println!("Measured storage open and first CPU draw");
    let mut typing = vec![];
    let mut typing_cycles = vec![];
    let typical_regions_before = cx.update(|cx| editor.read(cx).performance_regions());
    report["phase"] = json!("typical edits");
    for index in 0..1000 {
        let started = Instant::now();
        let cleanup = main.update(&mut cx, |_, w, cx| {
            w.dispatch_keystroke(
                Keystroke {
                    modifiers: Modifiers::default(),
                    key: "x".into(),
                    key_char: Some("x".into()),
                },
                cx,
            );
            w.draw(cx)
        })?;
        typing.push(ms(started.elapsed()));
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        typing_cycles.push(ms(started.elapsed()));
        if (index + 1) % 100 == 0 {
            report["typical_input_dispatch_to_cpu_draw_end"] = samples(&typing);
            report["typical_complete_foreground_cycle"] = samples(&typing_cycles);
            write_report(directory, "headless.partial.json", &report)?;
            println!("Measured {} typical edit cycles", index + 1);
        }
    }
    report["typical_input_dispatch_to_cpu_draw_end"] = samples(&typing);
    report["typical_complete_foreground_cycle"] = samples(&typing_cycles);
    report["typical_region_delta"] = cx.update(|cx| {
        region_values(
            editor.read(cx).performance_regions(),
            typical_regions_before,
        )
    });
    write_report(directory, "headless.partial.json", &report)?;
    println!("Measured 1,000 typical edit cycles");
    ensure!(
        cx.update(|cx| editor
            .read(cx)
            .model
            .document
            .plain_text()
            .matches('x')
            .count())
            >= 1000,
        "Diagnostic typing failed to reach the selected editor"
    );
    drop(editor);
    settle(&mut cx, &shell, main)?;
    report["phase"] = json!("note switches");
    let mut switches = vec![];
    for id in &f.typical {
        let started = Instant::now();
        let editor = select(&mut cx, &shell, main, id)?;
        draw(&mut cx, main)?;
        ensure!(
            cx.update(|cx| editor.read(cx).model.marked.is_none()),
            "Unexpected composing state"
        );
        switches.push(ms(started.elapsed()));
    }
    report["ordinary_note_switch_to_cpu_draw_end"] = samples(&switches);
    write_report(directory, "headless.partial.json", &report)?;
    println!("Measured 100 note switches");
    report["phase"] = json!("large note");
    let started = Instant::now();
    let large = select(&mut cx, &shell, main, &f.large)?;
    draw(&mut cx, main)?;
    report["large_note_switch_to_first_cpu_draw_ms"] = json!(ms(started.elapsed()));
    let mut large_edits = vec![];
    let mut large_cycles = vec![];
    let large_regions_before = cx.update(|cx| large.read(cx).performance_regions());
    let derived_before = cx.update(|cx| derived_values(shell.read(cx)));
    let mut derived_batch_before = derived_before;
    let mut derived_batches = vec![];
    for i in 0..100 {
        let started = Instant::now();
        let cleanup = main.update(&mut cx, |_, w, cx| {
            large.update(cx, |e, cx| {
                e.model.select(if i % 2 == 0 { 0 } else { 200_000 }, false);
                e.replace_text_in_range(None, "z", w, cx);
            });
            w.draw(cx)
        })?;
        large_edits.push(ms(started.elapsed()));
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        large_cycles.push(ms(started.elapsed()));
        if (i + 1) % 10 == 0 {
            let current = cx.update(|cx| derived_values(shell.read(cx)));
            derived_batches.push(json!({"end_edit":i+1,"callback_delta":derived_delta(current,derived_batch_before)}));
            derived_batch_before = current;
            report["large_model_input_to_cpu_draw_end"] = samples(&large_edits);
            report["large_complete_foreground_cycle"] = samples(&large_cycles);
            report["large_derived_callback_delta"] = derived_delta(current, derived_before);
            report["large_derived_callback_batches"] = json!(derived_batches);
            write_report(directory, "headless.partial.json", &report)?;
            println!("Measured {} large-note edit cycles", i + 1);
        }
    }
    report["large_model_input_to_cpu_draw_end"] = samples(&large_edits);
    report["large_complete_foreground_cycle"] = samples(&large_cycles);
    let large_regions_after = cx.update(|cx| large.read(cx).performance_regions());
    report["large_edit_region_delta"] = region_values(large_regions_after, large_regions_before);
    report["large_derived_callback_delta"] =
        cx.update(|cx| derived_delta(derived_values(shell.read(cx)), derived_before));
    report["usage_after_large_edits"] = cx.update(|cx| usage(shell.read(cx), cx));
    report["selected_large_editor_after_edits"] =
        cx.update(|cx| selected_editor(shell.read(cx), cx));
    report["rss_after_large_edits_bytes"] = json!(rss_bytes()?);
    write_report(directory, "headless.partial.json", &report)?;
    println!("Measured 100 distinct large-note edit cycles");
    report["phase"] = json!("large scroll");
    let mut scroll = vec![];
    let mut achieved_scroll_y = vec![];
    for i in 0..100 {
        main.update(&mut cx, |_, _, cx| {
            large.update(cx, |e, cx| {
                let mut state = e.view_state();
                state.scroll.y = px(-(i as f32) * 1000.);
                e.restore_view_state(state, cx);
            })
        })?;
        scroll.push(draw(&mut cx, main)?);
        achieved_scroll_y.push(cx.update(|cx| f32::from(large.read(cx).view_state().scroll.y)));
    }
    report["large_document_scrolling_cpu_draw"] = samples(&scroll);
    let distinct_scroll_positions = achieved_scroll_y
        .iter()
        .map(|y| y.to_bits())
        .collect::<std::collections::HashSet<_>>()
        .len();
    ensure!(
        distinct_scroll_positions == 100,
        "The 100 requested scroll positions did not produce 100 distinct achieved offsets"
    );
    report["large_scroll_position_proof"] =
        json!({"achieved_y":achieved_scroll_y,"distinct_positions":distinct_scroll_positions});
    report["large_scroll_region_delta"] =
        cx.update(|cx| region_values(large.read(cx).performance_regions(), large_regions_after));
    report["selected_large_editor_after_scroll"] =
        cx.update(|cx| selected_editor(shell.read(cx), cx));
    drop(large);
    settle(&mut cx, &shell, main)?;
    let started = Instant::now();
    let long = select(&mut cx, &shell, main, &f.long)?;
    draw(&mut cx, main)?;
    report["long_token_switch_to_cpu_draw_ms"] = json!(ms(started.elapsed()));
    write_report(directory, "headless.partial.json", &report)?;
    println!("Measured scroll and long-token layout");
    drop(long);
    let _ = select(&mut cx, &shell, main, &f.typical[0])?;
    draw(&mut cx, main)?;
    report["phase"] = json!("main Quick cycles");
    let mut cycle_rss = vec![];
    let mut cycle_editors = vec![];
    for cycle in 0..100 {
        cx.update(|cx| shell.update(cx, |s, cx| s.open_quick(cx)));
        cx.run_until_parked();
        let quick = cx.update(|cx| {
            shell
                .read(cx)
                .windows
                .quick
                .as_ref()
                .context("Quick window did not open")
                .map(|q| q.0)
        })?;
        // TestWindow ignores WindowOptions.focus. Activate this owned hidden
        // Quick window while main still exists, so its retained prior-active
        // callback cannot target a main window removed by the next step.
        quick.update(&mut cx, |_, w, _| w.activate_window())?;
        cx.run_until_parked();
        draw(&mut cx, quick)?;
        let main = cx.update(|cx| {
            shell
                .read(cx)
                .windows
                .main
                .context("Main window is missing")
        })?;
        main.update(&mut cx, |_, w, cx| {
            shell.update(cx, |s, cx| s.remove_main(w, cx))
        })?;
        cx.run_until_parked();
        cx.update(|cx| shell.update(cx, |s, cx| s.open_main(Some(f.typical[0].clone()), cx)));
        cx.run_until_parked();
        let main = cx.update(|cx| shell.read(cx).windows.main.context("Main did not reopen"))?;
        draw(&mut cx, main)?;
        super::visual_keys(&mut cx, quick, "ctrl-w")?;
        cx.run_until_parked();
        cycle_rss.push(rss_bytes()?);
        cycle_editors.push(
            cx.update(|cx| usage(shell.read(cx), cx))["session_owned_editors"]
                .as_u64()
                .unwrap(),
        );
        if (cycle + 1) % 10 == 0 {
            report["100_main_quick_open_close_cycles"] = json!({"completed_cycles":cycle+1,"rss_bytes":cycle_rss,"session_owned_editors":cycle_editors,"scope":"same library controller; counts exclude opaque GPUI entities/tasks and GPU allocations"});
            write_report(directory, "headless.partial.json", &report)?;
            println!("Measured {} main/Quick cycles", cycle + 1);
        }
    }
    report["100_main_quick_open_close_cycles"] = json!({"rss_bytes": cycle_rss, "session_owned_editors": cycle_editors, "scope": "same library controller; counts do not include opaque GPUI entities/tasks or GPU allocation"});
    let main = cx.update(|cx| shell.read(cx).windows.main.unwrap());
    let before = main.update(&mut cx, |_, w, _| {
        w.frame_duration_snapshot().draw_duration_histogram.len()
    })?;
    for _ in 0..50 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }
    let after = main.update(&mut cx, |_, w, _| {
        w.frame_duration_snapshot().draw_duration_histogram.len()
    })?;
    report["simulated_idle"] = json!({"simulated_seconds": 5, "unforced_draw_samples": after-before, "native_idle_repaint": "unverified; TestPlatform scheduling only"});
    report["native_submission_and_startup_gates"] = json!(
        "unverified; headless draw diagnostics exclude platform submission and process startup"
    );
    report["usage_final"] = cx.update(|cx| usage(shell.read(cx), cx));
    report["incomplete"] = json!(false);
    report["phase"] = json!("complete");
    write_report(directory, "headless.json", &report)?;
    for window in cx.update(|cx| cx.windows()) {
        let _ = window.update(&mut cx, |_, w, _| w.remove_window());
    }
    drop(shell);
    cx.run_until_parked();
    println!(
        "Measured real-font CPU diagnostics: {}",
        directory.join("headless.json").display()
    );
    Ok(())
}

fn native(
    directory: &Path,
    f: Fixture,
    seconds: u64,
    large: bool,
    queue: Option<bool>,
) -> Result<()> {
    let (worker, library) = Worker::start(f.root.clone(), false)?;
    let directory = directory.to_path_buf();
    let final_report = directory.join("native.json");
    let mut report = metadata(
        &f,
        "native GPUI profiler: input dispatch to platform submission",
    );
    let selected = if large {
        f.large.clone()
    } else {
        f.typical[0].clone()
    };
    report["native_workload"] = json!(if large {
        "synthetic 3,000-paragraph/20-table note in 10,000-note library"
    } else {
        "synthetic ordinary note in 10,000-note library"
    });
    report["native_timing_scope"] = json!(
        "GPUI input dispatch begins after native event delivery; latency ends after platform_window.draw returns. Native-event queue wait before dispatch and compositor/display presentation are excluded. Inspect queue raw service gaps and model progress separately."
    );
    gpui_platform::application().with_assets(crate::assets::Assets).run(move |cx| {
        crate::assets::load_fonts(cx).unwrap(); super::bind_keys(cx); crate::search::bind_keys(cx);
        let h = cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None,size(px(1440.),px(960.)),cx))), app_id: Some("org.omarchy.Notes.Performance".into()), titlebar: None, window_background: WindowBackgroundAppearance::Opaque, ..Default::default() }, |w,cx| {
            w.set_window_title("Omarchy Notes — disposable performance diagnostics");
            cx.new(|cx| { let mut s = Shell::new_live(Appearance::Reference,f.root,worker,library,w,cx); s.command(Command::Select(selected),w,cx); s })
        }).unwrap();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let baseline = h.update(cx, |s,w,cx| (w.input_latency_snapshot(),w.frame_duration_snapshot(),usage(s,cx),rss_bytes().unwrap_or(0),selected_editor(s,cx),environment(w),derived_values(s))).unwrap();
            let _ = write_report(&directory,"native-start.json",&json!({"usage":baseline.2,"rss_bytes":baseline.3,"pid":std::process::id(),"app_id":"org.omarchy.Notes.Performance","seconds":seconds,"selected_editor":baseline.4,"environment":baseline.5}));
            println!("PERFORMANCE_WINDOW_READY pid={} app_id=org.omarchy.Notes.Performance seconds={seconds}",std::process::id());
            // Probe activation follows the input baseline/readiness marker.
            // Its weak target is this synthetic editor only; no other note is read.
            let probe = queue.map(|active| {
                let editor = h.update(cx,|s,_,_|s.live.as_ref().unwrap().notes[s.library.selected.as_ref().unwrap()].active_editor().unwrap().downgrade()).unwrap();
                let progress = std::rc::Rc::new(move |cx:&App|editor.upgrade().map(|e|e.read(cx).model.revision));
                let mut config=QueueProbeConfig::default();
                if !active {config.callbacks_per_burst=0;}
                cx.update(|cx|QueueProbe::start(config,progress,cx)).unwrap()
            });
            let probe_started=probe.as_ref().map_or_else(Instant::now,QueueProbe::started_at);
            // App::subscribe internally holds a weak entity. Retaining this
            // subscription limits the journal to this fixed synthetic editor.
            let _edit_subscription=probe.as_ref().map(|probe| {
                let editor=h.update(cx,|s,_,_|s.live.as_ref().unwrap().notes[s.library.selected.as_ref().unwrap()].active_editor().unwrap().clone()).unwrap();
                let recorder=probe.edit_recorder();
                cx.update(|cx|cx.subscribe(&editor,move|editor,_:&Changed,cx|recorder.record(editor.read(cx).model.revision)))
            });
            let phase_captures=std::rc::Rc::new(std::cell::RefCell::new([None::<NativePhaseCapture>,None]));
            let _phase_task=probe.as_ref().map(|probe| {
                let (start_due,end_due,_)=probe.phase_offsets();
                let captures=phase_captures.clone();
                cx.spawn(async move|cx| {
                    for (index,offset) in [start_due,end_due].into_iter().enumerate() {
                        let due=probe_started+offset;
                        let background=cx.background_executor().clone();
                        let woke=cx.background_executor().spawn(async move {
                            background.timer(due.saturating_duration_since(Instant::now())).await;
                            Instant::now()
                        }).await;
                        if let Ok(capture)=h.update(cx,|s,w,cx|NativePhaseCapture {input:w.input_latency_snapshot(),selected:selected_progress(s,cx),planned:offset,timer_woke:woke.duration_since(probe_started),captured:probe_started.elapsed()}) {
                            captures.borrow_mut()[index]=Some(capture);
                        } else {break;}
                    }
                })
            });
            let mut periodic = vec![];
            for second in 1..=seconds {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let sample = h.update(cx, |s,w,cx| json!({"second":second,"actual_elapsed_ms":ms(probe_started.elapsed()),"rss_bytes":rss_bytes().unwrap_or(0),"draw_samples":w.frame_duration_snapshot().draw_duration_histogram.len(),"input_samples":w.input_latency_snapshot().latency_histogram.len(),"usage":usage(s,cx),"selected_editor":selected_progress(s,cx),"queue_state":probe.as_ref().map(QueueProbe::status)}));
                match sample { Ok(sample) => periodic.push(sample), Err(_) => { cx.update(|cx| cx.quit()); return; } }
            }
            let result: Result<()> = h.update(cx, |s,w,cx| {
                let input = w.input_latency_snapshot(); let frame = w.frame_duration_snapshot();
                let mut latency = input.latency_histogram; latency.subtract(&baseline.0.latency_histogram)?;
                let mut coalesced = input.events_per_frame_histogram; coalesced.subtract(&baseline.0.events_per_frame_histogram)?;
                let mut cpu = frame.draw_duration_histogram; cpu.subtract(&baseline.1.draw_duration_histogram)?;
                let mut submission = frame.dirty_to_present_histogram; submission.subtract(&baseline.1.dirty_to_present_histogram)?;
                let represented_events:u64=coalesced.iter_recorded().map(|v|v.value_iterated_to().saturating_mul(v.count_at_value())).sum();
                report["input_to_platform_submission"] = json!({"samples":latency.len(),"p95_ms":(!latency.is_empty()).then(||latency.value_at_quantile(0.95) as f64/1e6),"p99_ms":(!latency.is_empty()).then(||latency.value_at_quantile(0.99) as f64/1e6),"max_ms":(!latency.is_empty()).then(||latency.max() as f64/1e6),"coalesced_events_p95":coalesced.value_at_quantile(0.95),"mid_draw_events_dropped":input.mid_draw_events_dropped-baseline.0.mid_draw_events_dropped});
                report["represented_input_events"] = json!(represented_events);
                report["queue_probe"] = probe.as_ref().map_or(Value::Null,QueueProbe::snapshot);
                if let [Some(start),Some(end)]=&*phase_captures.borrow() {
                    let mut active_latency=end.input.latency_histogram.clone();active_latency.subtract(&start.input.latency_histogram)?;
                    let mut events=end.input.events_per_frame_histogram.clone();events.subtract(&start.input.events_per_frame_histogram)?;
                    let represented:u64=events.iter_recorded().map(|v|v.value_iterated_to().saturating_mul(v.count_at_value())).sum();
                    report["queue_load_window"] = json!({"planned_start_ms":ms(start.planned),"planned_end_ms":ms(end.planned),"background_start_wake_ms":ms(start.timer_woke),"background_end_wake_ms":ms(end.timer_woke),"actual_start_ms":ms(start.captured),"actual_end_ms":ms(end.captured),"latency_samples":active_latency.len(),"represented_input_events":represented,"p95_ms":(!active_latency.is_empty()).then(||active_latency.value_at_quantile(0.95) as f64/1e6),"p99_ms":(!active_latency.is_empty()).then(||active_latency.value_at_quantile(0.99) as f64/1e6),"max_ms":(!active_latency.is_empty()).then(||active_latency.max() as f64/1e6),"mid_draw_events_dropped":end.input.mid_draw_events_dropped.saturating_sub(start.input.mid_draw_events_dropped),"selected_editor_baseline":start.selected,"selected_editor_final":end.selected,"observed_edit_delta":(start.selected["synthetic_note_id"]==end.selected["synthetic_note_id"]).then(||end.selected["model_revision"].as_u64().zip(start.selected["model_revision"].as_u64()).map(|(a,b)|a.saturating_sub(b))).flatten(),"scope":"Dedicated background-timer-woken boundary captures; actual foreground capture timestamps include scheduling delay. Inspect raw queue/heartbeat samples. Synthetic pressure is not indexing/import evidence."});
                }
                report["settled_idle_tail"] = idle_tail(&periodic);
                report["native_derived_callback_delta"] = derived_delta(derived_values(s),baseline.6);
                report["native_frame_cpu_draw"] = json!({"samples":cpu.len(),"p95_ms":(!cpu.is_empty()).then(||cpu.value_at_quantile(0.95) as f64/1e6),"max_ms":(!cpu.is_empty()).then(||cpu.max() as f64/1e6)});
                report["dirty_to_platform_submission"] = json!({"samples":submission.len(),"p95_ms":(!submission.is_empty()).then(||submission.value_at_quantile(0.95) as f64/1e6)});
                report["usage_final"] = usage(s,cx); report["periodic"] = json!(periodic); report["initial_rss_bytes"] = json!(baseline.3); report["rss_final_bytes"] = json!(rss_bytes()?);
                report["scale_factor"] = json!(w.scale_factor());
                report["actual_logical_viewport"] = json!([f32::from(w.viewport_size().width),f32::from(w.viewport_size().height)]);
                report["environment"] = environment(w);
                report["selected_editor_baseline"] = baseline.4.clone();
                report["selected_editor_final"] = selected_editor(s,cx);
                let final_editor = &report["selected_editor_final"];
                let same_note = final_editor["synthetic_note_id"] == baseline.4["synthetic_note_id"];
                report["edit_revision_delta"] = json!(same_note.then(|| final_editor["model_revision"].as_u64().unwrap_or(0).saturating_sub(baseline.4["model_revision"].as_u64().unwrap_or(0))));
                report["typed_utf8_byte_delta"] = json!(same_note.then(|| report["selected_editor_final"]["text_utf8_bytes"].as_i64().unwrap_or(0)-baseline.4["text_utf8_bytes"].as_i64().unwrap_or(0)));
                report["edit_requirement"] = json!({"minimum_observed_edits":if large {100}else{1000},"observed_model_revision_delta":report["edit_revision_delta"],"minimum_met":report["edit_revision_delta"].as_u64().is_some_and(|n|n>=if large {100}else{1000}),"scope":"Model edits differ from frame-latency samples: input may be coalesced and navigation may invalidate without editing."});
                report["manual_procedure"] = json!("Record actual actions, edit counts, compositor, GPU, scale, power/cache conditions alongside this report. No input was generated by the app.");
                report["gate_status"] = json!("unverified unless a documented guarded workload supplies at least 1,000 edits and all required scenarios; these are platform submission, not display presentation timings");
                s.save_real_notes(cx);
                Ok(())
            }).unwrap_or_else(Err);
            if let Err(error) = result { eprintln!("Performance report failed: {error:#}"); cx.update(|cx| cx.quit()); return; }
            let deadline = Instant::now()+Duration::from_secs(30);
            loop {
                let committed = h.update(cx, |s,w,cx| {
                    s.poll_storage(w,cx);
                    s.live.as_ref().unwrap().notes.values().all(|n|!session_has_uncommitted_work(n,cx))
                });
                match committed {
                    Ok(true) => { report["final_write_boundary"] = json!("all pending synthetic edits acknowledged committed"); break; }
                    Ok(false) if Instant::now()<deadline => cx.background_executor().timer(Duration::from_millis(25)).await,
                    _ => { report["final_write_boundary"] = json!("unverified: save did not settle before timeout or window close"); break; }
                }
            }
            if let Err(error) = write_report(&directory,"native.json",&report) { eprintln!("Performance report failed: {error:#}"); }
            cx.update(|cx| cx.quit());
        }).detach();
        cx.on_window_closed(|cx,_| { if cx.windows().is_empty() { cx.quit(); } }).detach();
    });
    ensure!(
        final_report.is_file(),
        "Native diagnostics ended before writing a complete report; this run supplies no pass"
    );
    let completed: Value = serde_json::from_slice(&std::fs::read(&final_report)?)?;
    ensure!(
        completed["final_write_boundary"] == "all pending synthetic edits acknowledged committed",
        "Native diagnostic save boundary remained unverified; inspect the retained report"
    );
    Ok(())
}

/// Creates a fresh synthetic notebook. Refuses existing output directories.
pub fn performance_check(
    directory: &Path,
    native_seconds: Option<u64>,
    large: bool,
    queue: Option<bool>,
) -> Result<()> {
    ensure!(
        !large || native_seconds.is_some(),
        "Selecting the native large note requires a native duration"
    );
    ensure!(
        queue.is_none() || native_seconds.is_some_and(|seconds| seconds >= 15),
        "Queue diagnostics require at least 15 native seconds"
    );
    std::fs::create_dir(directory)
        .context("Choose a new output directory; existing reports are never overwritten")?;
    let temporary = tempfile::tempdir()?;
    let f = fixture(temporary.path().join("library"))?;
    match native_seconds {
        Some(seconds) => native(directory, f, seconds, large, queue),
        None => headless(directory, f),
    }
}
