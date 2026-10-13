//! Test driver: mutate only the explicitly supplied scratch library.
use notes_core::document::Span;
fn main() -> anyhow::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .expect("scratch library path required");
    let mut store = notes_storage::Store::open(std::path::Path::new(&root))?;
    if std::env::args().nth(2).as_deref() == Some("hold") {
        use std::io::Write;
        println!("LOCKED");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        return Ok(());
    }
    for mut note in store.load()?.notes {
        let id = note.body.leaves()[0].id.clone();
        let block = note.body.block_mut(&id).unwrap();
        let mut span = Span::text(" · Rust");
        span.set_mark("italic", true);
        block.spans.push(span);
        store.save(&note).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    let mut media = store.create(notes_core::document::Document::default(), None)?;
    let paths = vec![
        store
            .root
            .join("blobs/bd/bdec1de53e3c856d6973c7324a5d719d6f6169e01a6f75ae3b4453f419d625e2"),
        store
            .root
            .join("blobs/d7/d7449961543b028f75a5778a6cbcfb8d3b7ea2dfe66da1efc60d9747f4fc4803"),
    ];
    media.body = store.import_media(
        &media.id,
        notes_storage::media::Import::Files {
            paths,
            images: None,
        },
    )?;
    store.save(&media).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    store.create(media.body, None)?;
    Ok(())
}
