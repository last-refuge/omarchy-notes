fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/context-check".into());
    notes_ui::context_check(std::path::Path::new(&directory))
}
