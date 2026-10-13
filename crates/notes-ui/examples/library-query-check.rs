fn main() -> anyhow::Result<()> {
    notes_ui::library_query_check(std::path::Path::new(
        &std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/library-query-check".into()),
    ))
}
