fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/quick-note-check".into());
    notes_ui::quick_note_check(std::path::Path::new(&directory))
}
