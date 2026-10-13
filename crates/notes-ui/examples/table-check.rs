fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/table-check".into());
    notes_ui::table_check(std::path::Path::new(&directory))
}
