fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/link-check".into());
    notes_ui::link_check(std::path::Path::new(&directory))
}
