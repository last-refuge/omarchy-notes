fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/writing-tools-check".into());
    notes_ui::writing_tools_check(std::path::Path::new(&directory))
}
