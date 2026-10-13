//! Test/embedding driver: real single writer and protocol, no graphical session.
use anyhow::{Context, Result};
use std::{
    io::{self, Write},
    path::PathBuf,
};
fn main() -> Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("Pass a scratch library path.")?,
    );
    let fail = std::env::args().nth(2).as_deref() == Some("--simulate-save-failure");
    let (worker, _) = notes_storage::Worker::start(root, fail)?;
    let endpoint = notes_platform::instance::Endpoint::discover(worker.root())?;
    let _server = notes_platform::instance::Server::bind(&worker, endpoint)?;
    println!("READY");
    io::stdout().flush()?;
    let mut stop = String::new();
    io::stdin().read_line(&mut stop)?;
    Ok(())
}
