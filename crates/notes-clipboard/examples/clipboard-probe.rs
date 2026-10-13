//! Manual Wayland integration probe. Only reads after a caller installs test clipboard data.
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::PathBuf,
};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let clipboard = notes_clipboard::Clipboard::connect()?;
    match args.first().map(String::as_str) {
        Some("offer") => {
            let mut offers = BTreeMap::new();
            for pair in args[1..].as_chunks::<2>().0 {
                offers.insert(pair[0].clone(), std::fs::read(&pair[1])?);
            }
            clipboard.copy(offers)?;
            println!("READY");
            io::stdout().flush()?;
            let mut line = String::new();
            while io::stdin().read_line(&mut line)? > 0 {
                if line.trim() == "self" {
                    let result = clipboard.paste(false)?;
                    println!("{} {}", result.mime, result.bytes.len());
                    io::stdout().flush()?;
                }
                line.clear();
            }
        }
        Some("paste") | Some("plain") => {
            let content = clipboard.paste(args[0] == "plain")?;
            std::fs::write(PathBuf::from(&args[1]), content.bytes)?;
            println!("{}", content.mime);
        }
        _ => anyhow::bail!("Use offer MIME FILE [MIME FILE …], paste FILE, or plain FILE"),
    }
    Ok(())
}
