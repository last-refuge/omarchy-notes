fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/visual-check".into());
    notes_ui::visual_check(std::path::Path::new(&directory))
}
