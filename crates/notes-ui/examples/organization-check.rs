fn main() -> anyhow::Result<()> {
    notes_ui::organization_check(std::path::Path::new(
        &std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/organization-check".into()),
    ))
}
