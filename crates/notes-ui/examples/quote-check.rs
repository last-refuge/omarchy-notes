fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/quote-check".into());
    notes_ui::quote_check(std::path::Path::new(&directory))
}
