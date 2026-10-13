fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_default_env()
        .format(|buffer, record| {
            use std::io::Write;
            writeln!(
                buffer,
                "[{} {}:{}] {}",
                record.level(),
                record.target(),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
    let mut output = std::path::PathBuf::from("target/performance-check");
    let mut native = None;
    let mut large = false;
    let mut queue = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                output = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--output needs a new directory"))?
                    .into();
            }
            "--native" => {
                let seconds: u64 = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--native needs a duration in seconds"))?
                    .parse()?;
                anyhow::ensure!(
                    (5..=3600).contains(&seconds),
                    "Native duration must be 5–3600 seconds"
                );
                native = Some(seconds);
            }
            "--large" => large = true,
            "--queue-probe" => {
                anyhow::ensure!(
                    queue.replace(true).is_none(),
                    "Choose only one of --queue-probe or --passive"
                );
            }
            "--passive" => {
                anyhow::ensure!(
                    queue.replace(false).is_none(),
                    "Choose only one of --queue-probe or --passive"
                );
            }
            "--help" => {
                println!(
                    "Opt-in disposable Rust performance diagnostics\n\nperformance-check [--output NEW_DIRECTORY] [--native SECONDS [--large] [--queue-probe | --passive]]\n\nDefault: real-font headless CPU draw diagnostics, never desktop input.\n--native: opens one owned test window for manual/guarded input; records GPUI input-to-platform-submission and idle histograms. No input is sent automatically.\n--large: initially selects the synthetic 3,000-paragraph/20-table note; requires --native.\n--queue-probe: finite synthetic ready-task pressure and foreground heartbeat; requires at least 15 native seconds.\n--passive: matched heartbeat/timer control with zero pressure callbacks.\nSet OMARCHY_PERFORMANCE_REVISION to the tested Git revision. Reports contain synthetic fixtures only."
                );
                return Ok(());
            }
            _ => anyhow::bail!("Unknown option {arg}; use --help"),
        }
    }
    anyhow::ensure!(
        !large || native.is_some(),
        "--large requires --native; the default headless workload already includes the large fixture"
    );
    anyhow::ensure!(
        queue.is_none() || native.is_some_and(|seconds| seconds >= 15),
        "Queue diagnostics require --native with at least 15 seconds"
    );
    notes_ui::performance_check(&output, native, large, queue)
}
