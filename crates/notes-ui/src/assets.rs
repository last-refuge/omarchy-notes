use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

macro_rules! icons {
    ($($name:literal),*) => {
        fn icon_bytes(path: &str) -> Option<&'static [u8]> {
            match path { $(concat!("icons/", $name, ".svg") => Some(include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)* _ => None }
        }
    };
}
icons!(
    "note",
    "folder",
    "pin",
    "search",
    "back",
    "sidebar",
    "checklist",
    "attach",
    "trash",
    "more",
    "list",
    "all-notes",
    "link",
    "close",
    "gallery",
    "star",
    "focus",
    "sun",
    "check",
    "plus",
    "arrow",
    "sort",
    "minus",
    "maximize"
);

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(icon_bytes(path).map(Cow::Borrowed))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

pub fn load_fonts(cx: &gpui::App) -> anyhow::Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/NotoSans-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/NotoSans-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/NotoSerif-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/NotoSerif-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/NotoSerif-Italic.ttf")),
    ])
}
