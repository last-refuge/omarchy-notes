fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/media-check".into());
    notes_ui::media_check(std::path::Path::new(&directory))
}
