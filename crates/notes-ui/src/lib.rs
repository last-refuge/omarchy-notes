//! Native GPUI preview, isolated from production libraries.
mod accessible_input;
mod assets;
mod rich_editor;
mod search;
mod shell;
mod theme;

use gpui::{prelude::*, *};
use notes_core::launch::Intent;
#[cfg(feature = "visual-tests")]
pub use shell::organization_check;
#[cfg(feature = "visual-tests")]
pub use shell::quick_note_check;
#[cfg(feature = "visual-tests")]
pub use shell::settings::settings_check;
#[cfg(feature = "visual-tests")]
pub use shell::visual_check;
#[cfg(feature = "visual-tests")]
pub use shell::window_launch_check;
#[cfg(feature = "visual-tests")]
pub use shell::{
    context_check, link_check, media_check, pdf_ui_check, quote_check, table_check, writing_check,
    writing_tools_check,
};
pub use theme::Appearance;

pub fn run(appearance: Appearance) {
    run_inner(appearance, None, Intent::Main, None);
}

pub fn run_library(
    appearance: Appearance,
    root: std::path::PathBuf,
    fail_saves: bool,
) -> anyhow::Result<()> {
    run_library_window(appearance, root, fail_saves, false)
}

pub fn run_library_window(
    appearance: Appearance,
    root: std::path::PathBuf,
    fail_saves: bool,
    quick: bool,
) -> anyhow::Result<()> {
    let (worker, library) = notes_storage::Worker::start(root.clone(), fail_saves)?;
    run_owned_library(
        Some(appearance),
        worker,
        library,
        if quick { Intent::Quick } else { Intent::Main },
    )
}

pub fn run_owned_library(
    appearance: Option<Appearance>,
    worker: notes_storage::Worker,
    library: notes_storage::Library,
    intent: Intent,
) -> anyhow::Result<()> {
    intent.validate()?;
    if let Intent::Note { id } = &intent {
        anyhow::ensure!(
            library.notes.iter().any(|n| &n.id == id),
            "That note is not available in this library."
        );
    }
    // Bind only after storage owns the compatible library lock, before any GPU
    // initialization. Dropping this guard stops new clients at app shutdown.
    let endpoint = notes_platform::instance::Endpoint::discover(worker.root())?;
    let (sender, receiver) = notes_platform::launch::channel();
    let _instance =
        notes_platform::instance::Server::bind_with_launches(&worker, endpoint, Some(sender))?;
    let preferences = shell::settings::State::load(appearance);
    run_inner(
        preferences.appearance(),
        Some((library.root.clone(), worker, library, receiver)),
        intent,
        Some(preferences),
    );
    Ok(())
}

fn run_inner(
    appearance: Appearance,
    library: Option<(
        std::path::PathBuf,
        notes_storage::Worker,
        notes_storage::Library,
        notes_platform::launch::Receiver,
    )>,
    intent: Intent,
    preferences: Option<shell::settings::State>,
) {
    let clipboard = if library.is_some() {
        match notes_clipboard::Clipboard::connect() {
            Ok(transport) => rich_editor::ClipboardPlatform {
                transport: Some(transport),
                unavailable: None,
            },
            Err(error) => rich_editor::ClipboardPlatform {
                transport: None,
                unavailable: Some(error.to_string()),
            },
        }
    } else {
        rich_editor::ClipboardPlatform {
            transport: None,
            unavailable: None,
        }
    };
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            cx.set_global(clipboard);
            if let Err(error) = assets::load_fonts(cx) {
                eprintln!("Could not load the preview fonts: {error}");
                cx.quit();
                return;
            }
            search::bind_keys(cx);
            shell::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1440.), px(960.)), cx);
            let result = if intent == Intent::Quick {
                let (root, worker, library, receiver) =
                    library.expect("Quick Note needs a library");
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(680.), px(660.)),
                            cx,
                        ))),
                        titlebar: None,
                        app_id: Some("org.omarchy.Notes.Preview".into()),
                        window_background: WindowBackgroundAppearance::Opaque,
                        window_min_size: Some(size(px(420.), px(420.))),
                        ..Default::default()
                    },
                    |w, cx| {
                        let view = cx.new(|cx| {
                            shell::QuickView::initial(appearance, root, worker, library, w, cx)
                        });
                        view.update(cx, |s, cx| s.configure_launches(receiver, w, cx));
                        if let Some(preferences) = preferences {
                            view.update(cx, |s, cx| s.configure_preferences(preferences, cx));
                        }
                        view
                    },
                )
                .map(|_| ())
            } else {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: None,
                        app_id: Some("org.omarchy.Notes.Preview".into()),
                        window_background: WindowBackgroundAppearance::Opaque,
                        window_min_size: Some(size(px(420.), px(420.))),
                        ..Default::default()
                    },
                    |window, cx| {
                        if let Some((root, worker, library, receiver)) = library {
                            window.set_window_title("Omarchy Notes — Rust edition");
                            cx.new(|cx| {
                                let mut shell = shell::Shell::new_live(
                                    appearance, root, worker, library, window, cx,
                                );
                                if let Some(preferences) = preferences {
                                    shell.configure_preferences(preferences, cx);
                                }
                                shell.configure_launches(receiver, intent, window, cx);
                                shell
                            })
                        } else {
                            window.set_window_title("Omarchy Notes — GPUI sample preview");
                            cx.new(|cx| shell::Shell::new(appearance, window, cx))
                        }
                    },
                )
                .map(|_| ())
            };
            if let Err(error) = result {
                eprintln!("Could not open the native preview: {error}");
                cx.quit();
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        });
}
