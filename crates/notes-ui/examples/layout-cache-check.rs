fn main() -> anyhow::Result<()> {
    notes_ui::layout_cache_check(std::path::Path::new(
        &std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/layout-cache-check".into()),
    ))
}
