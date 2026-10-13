fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/pdf-ui-check".into());
    notes_ui::pdf_ui_check(std::path::Path::new(&directory))
}
