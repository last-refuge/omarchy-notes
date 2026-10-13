//! Deterministic release-mode pure editing/history profile, without GPUI.
//! Run: cargo run --release -p notes-editor --example history-profile --locked
//! These numbers cover model CPU and heap payload, not input-to-frame latency.
use notes_core::document::{Block, Document, DocumentSnapshot};
use notes_editor::Editor;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Tracking;
static LIVE: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            LIVE.fetch_add(l.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            if n >= l.size() {
                LIVE.fetch_add(n - l.size(), Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(l.size() - n, Ordering::Relaxed);
            }
        }
        q
    }
}
#[global_allocator]
static ALLOC: Tracking = Tracking;
fn rss() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}
fn pct(t: &mut [u128], p: usize) -> u128 {
    t.sort();
    t[(t.len() - 1) * p / 100]
}
fn main() {
    let mut doc = Document {
        schema: 1,
        blocks: (0..3000)
            .map(|i| {
                Block::paragraph(&format!(
                    "Paragraph {i:04}. {}",
                    "A release fixture with emoji 😀 and combining é. ".repeat(8)
                ))
            })
            .collect(),
        extra: BTreeMap::new(),
    };
    let mut table = Editor::new(Document {
        schema: 1,
        blocks: vec![Block::paragraph("")],
        extra: BTreeMap::new(),
    });
    table.insert_table(4, 4).unwrap();
    for i in 0..20 {
        let mut block = table
            .document
            .blocks
            .iter()
            .find(|b| b.kind == "table")
            .unwrap()
            .clone();
        block.id = format!("table-{i}");
        for (c, cell) in block.cells.iter_mut().enumerate() {
            for (b, p) in cell.blocks.iter_mut().enumerate() {
                p.id = format!("cell-{i}-{c}-{b}");
            }
        }
        doc.blocks.push(block);
    }
    drop(table);
    let mut editor = Editor::new(doc);
    // Verify the snapshot accounting against real allocation sizes. No other
    // threads or allocations occur between these samples.
    let before = LIVE.load(Ordering::Relaxed);
    let snapshot = DocumentSnapshot::from(editor.document.clone());
    let actual = LIVE.load(Ordering::Relaxed) - before;
    assert_eq!(
        snapshot.allocated_bytes(),
        actual,
        "snapshot heap accounting"
    );
    println!("compact snapshot allocated_bytes={actual}");
    drop(snapshot);
    let plain_bytes = editor.text().len();
    let before = LIVE.load(Ordering::Relaxed);
    let before_rss = rss();
    let mut times = Vec::new();
    for i in 0..1000 {
        let start = Instant::now();
        editor.insert("x", i * 5).unwrap();
        times.push(start.elapsed().as_micros());
    }
    println!(
        "coalesced edits: paragraphs=3000 tables=20 document_plain_bytes={plain_bytes} n={} p50_us={} p95_us={} p99_us={} max_us={} allocator_live_before={} allocator_live_after={} rss_before_kib={} rss_after_kib={}",
        times.len(),
        pct(&mut times, 50),
        pct(&mut times, 95),
        pct(&mut times, 99),
        times.iter().max().unwrap(),
        before,
        LIVE.load(Ordering::Relaxed),
        before_rss,
        rss()
    );
    println!(
        "history after coalesced edits: {:?}",
        editor.history_usage()
    );
    assert!(editor.history_usage().allocated_bytes <= notes_editor::MAX_HISTORY_BYTES);
    times.clear();
    let before = LIVE.load(Ordering::Relaxed);
    let before_rss = rss();
    for i in 0..125 {
        let start = Instant::now();
        editor.insert("x", 6000 + i * 1000).unwrap();
        times.push(start.elapsed().as_micros());
    }
    println!(
        "distinct undo edits: n={} p50_us={} p95_us={} p99_us={} max_us={} allocator_live_before={} allocator_live_after={} rss_before_kib={} rss_after_kib={}",
        times.len(),
        pct(&mut times, 50),
        pct(&mut times, 95),
        pct(&mut times, 99),
        times.iter().max().unwrap(),
        before,
        LIVE.load(Ordering::Relaxed),
        before_rss,
        rss()
    );
    println!("history after distinct edits: {:?}", editor.history_usage());
    assert!(editor.history_usage().allocated_bytes <= notes_editor::MAX_HISTORY_BYTES);
    for _ in 0..100 {
        editor.undo();
    }
    println!(
        "after 100 undo allocator_live={} rss_kib={}",
        LIVE.load(Ordering::Relaxed),
        rss()
    );
    println!("history after undo: {:?}", editor.history_usage());
    assert!(editor.history_usage().allocated_bytes <= notes_editor::MAX_HISTORY_BYTES);
    drop(editor);
    println!(
        "after editor drop allocator_live={} rss_kib={}",
        LIVE.load(Ordering::Relaxed),
        rss()
    );
}
