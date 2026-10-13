fn main() -> anyhow::Result<()> {
    notes_ui::library_recovery_check(std::path::Path::new(
        &std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/library-recovery-check".into()),
    ))
}
