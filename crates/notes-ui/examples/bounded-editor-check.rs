fn main() -> anyhow::Result<()> {
    notes_ui::bounded_editor_check(std::path::Path::new(
        &std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/bounded-editor-check".into()),
    ))
}
