//! Media and clipboard effects owned by a particular editor session.
use super::*;
use notes_core::document::Document;
use notes_storage::media::{Import, Media};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

pub(crate) struct ClipboardPlatform {
    pub transport: Option<notes_clipboard::Clipboard>,
    pub unavailable: Option<String>,
}
impl Global for ClipboardPlatform {}
pub struct MediaRequest {
    pub operation: u64,
    pub content: Import,
}
impl EventEmitter<MediaRequest> for RichEditor {}
#[derive(Clone)]
pub struct Prepared {
    pub info: Media,
    pub image: Option<Arc<RenderImage>>,
}
struct Pending {
    operation: u64,
    revision: u64,
    selection: notes_editor::Selection,
}
#[derive(Default)]
pub struct State {
    pub items: BTreeMap<String, Prepared>,
    pub active: bool,
    pub loading: bool,
    generation: u64,
    stamp: String,
    operation: u64,
    pending: Option<Pending>,
    pub ready: Option<Document>,
    pub notice: Option<String>,
    warning: Option<String>,
    task: Option<Task<()>>,
}
impl RichEditor {
    pub fn has_pending_work(&self) -> bool {
        self.media.pending.is_some() || self.media.ready.is_some()
    }
    pub fn activate_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.media.active = true;
        self.refresh_media(window, cx);
    }
    pub fn deactivate_media(&mut self, cx: &mut Context<Self>) {
        self.media.active = false;
        self.media.generation += 1;
        self.media.task = None;
        self.media.loading = false;
        self.media.stamp.clear();
        for item in self.media.items.values_mut() {
            item.image = None;
            item.info.preview = None;
        }
        cx.notify();
    }
    pub(super) fn refresh_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.media.active {
            return;
        }
        let doc = self.model.committed().clone();
        let stamp = format!(
            "{:?}{:?}",
            doc.references("image"),
            doc.references("attachment")
        );
        if stamp == self.media.stamp {
            return;
        }
        self.media.stamp = stamp;
        self.media.generation += 1;
        let generation = self.media.generation;
        self.media.loading = true;
        let root = PathBuf::from(&self.library_id);
        let task = cx.background_spawn(async move { notes_storage::media::load(&root, &doc) });
        self.media.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |s, _, cx| {
                if generation != s.media.generation || !s.media.active {
                    return;
                }
                s.media.loading = false;
                match result {
                    Ok(items) => {
                        s.media.items = items
                            .into_iter()
                            .map(|mut info| {
                                let image = info
                                    .preview
                                    .take()
                                    .and_then(|p| {
                                        image::RgbaImage::from_raw(p.width, p.height, p.bgra)
                                    })
                                    .map(|rgba| {
                                        Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]))
                                    });
                                (info.key.clone(), Prepared { info, image })
                            })
                            .collect();
                    }
                    Err(error) => s.media.notice = Some(format!("Couldn’t load media. {error}")),
                }
                s.layout = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn begin_transfer(&mut self, cx: &mut Context<Self>) -> Option<u64> {
        if self.model.marked.is_some() {
            self.error = Some("Finish composing text first.".into());
            cx.notify();
            return None;
        }
        if self.has_pending_work() {
            self.media.notice = Some("Finish or cancel the current insertion first.".into());
            cx.notify();
            return None;
        }
        self.media.operation += 1;
        let operation = self.media.operation;
        self.media.pending = Some(Pending {
            operation,
            revision: self.model.revision,
            selection: self.model.selection.clone(),
        });
        self.error = None;
        self.media.warning = None;
        cx.notify();
        Some(operation)
    }
    fn current_transfer(&self, operation: u64) -> bool {
        self.media
            .pending
            .as_ref()
            .is_some_and(|p| p.operation == operation)
    }
    fn unchanged_transfer(&self, operation: u64) -> bool {
        self.media.pending.as_ref().is_some_and(|p| {
            p.operation == operation
                && p.revision == self.model.revision
                && p.selection == self.model.selection
                && self.model.marked.is_none()
        })
    }
    pub fn import_result(
        &mut self,
        operation: u64,
        result: Result<Document, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.current_transfer(operation) {
            return;
        }
        let unchanged = self.unchanged_transfer(operation);
        self.media.pending = None;
        match result {
            Ok(document) => {
                if unchanged {
                    self.media.notice = None;
                    self.edit(|e| e.paste_fragment(document.blocks), window, cx);
                    if self.error.is_none() {
                        self.media.notice = self.media.warning.take();
                    }
                } else {
                    self.media.ready = Some(document);
                    self.media.notice =
                        Some("Ready to insert. Place your cursor, then choose Insert here.".into());
                }
            }
            Err(error) => {
                self.media.notice = None;
                self.error = Some(format!("Couldn’t add this selection. {error}"));
            }
        }
        self.media.stamp.clear();
        self.refresh_media(window, cx);
        cx.emit(InputState);
        cx.notify();
    }
    pub fn queue_import(&mut self, content: Import, cx: &mut Context<Self>) {
        if let Some(operation) = self.begin_transfer(cx) {
            self.media.notice = Some("Adding your files…".into());
            cx.emit(MediaRequest { operation, content });
            cx.emit(InputState);
        }
    }
    pub fn choose_files(&mut self, images: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = self.begin_transfer(cx) else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(
                if images {
                    "Insert images"
                } else {
                    "Attach files"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |s, _, cx| {
                if !s.current_transfer(operation) {
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => {
                        s.media.notice = Some("Adding your files…".into());
                        cx.emit(MediaRequest {
                            operation,
                            content: Import::Files {
                                paths,
                                images: Some(images),
                            },
                        });
                    }
                    Ok(Ok(None)) => s.media.pending = None,
                    result => {
                        s.media.pending = None;
                        s.error = Some(format!("Couldn’t open the file picker: {result:?}"));
                    }
                }
                cx.emit(InputState);
                cx.notify();
            });
        })
        .detach();
    }
    pub fn insert_ready(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.marked.is_some() {
            return;
        }
        if let Some(doc) = self.media.ready.take() {
            let backup = doc.clone();
            self.edit(|e| e.paste_fragment(doc.blocks), window, cx);
            if self.error.is_some() {
                self.media.ready = Some(backup);
            } else {
                self.media.notice = self.media.warning.take();
            }
            cx.emit(InputState);
            cx.notify();
        }
    }
    pub fn cancel_transfer(&mut self, cx: &mut Context<Self>) {
        self.media.pending = None;
        self.media.ready = None;
        self.media.notice = None;
        self.media.warning = None;
        cx.emit(InputState);
        cx.notify();
    }
    pub fn copy_rich(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.range().is_empty() {
            return;
        }
        let platform = cx.try_global::<ClipboardPlatform>();
        let transport = platform.and_then(|p| p.transport.clone());
        let unavailable = platform.and_then(|p| p.unavailable.clone());
        let Some(transport) = transport else {
            if self.copy(false, cx) && cut {
                self.edit(|e| e.insert("", 0), window, cx);
            }
            if unavailable.is_some() {
                self.media.notice = Some("Copied. This desktop supports plain text with other apps and rich text within this window.".into());
                cx.notify();
            }
            return;
        };
        let blocks = match self.model.fragment() {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        let Some(operation) = self.begin_transfer(cx) else {
            return;
        };
        self.media.notice = Some("Copying selection…".into());
        let root = PathBuf::from(&self.library_id);
        let doc = Document::from_blocks(blocks);
        let task = cx.background_spawn(async move {
            notes_storage::media::portable(&root, doc)
                .and_then(|f| f.offers())
                .and_then(|offers| transport.copy(offers))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |s, w, cx| {
                if !s.current_transfer(operation) {
                    return;
                }
                let unchanged = s.unchanged_transfer(operation);
                s.media.pending = None;
                s.media.notice = None;
                match result {
                    Ok(()) => {
                        if cut && unchanged {
                            s.edit(|e| e.insert("", 0), w, cx);
                        } else if cut {
                            s.media.notice =
                                Some("Copied. The selection changed, so nothing was cut.".into());
                        }
                    }
                    Err(error) => s.error = Some(format!("Couldn’t copy. {error}")),
                }
                cx.emit(InputState);
                cx.notify();
            });
        })
        .detach();
    }
    pub fn paste_rich(&mut self, plain: bool, window: &mut Window, cx: &mut Context<Self>) {
        let transport = cx
            .try_global::<ClipboardPlatform>()
            .and_then(|p| p.transport.clone());
        let Some(transport) = transport else {
            self.paste(plain, window, cx);
            if cx
                .try_global::<ClipboardPlatform>()
                .is_some_and(|p| p.unavailable.is_some())
            {
                self.media.notice = Some("This desktop supports plain text with other apps and rich text within this window.".into());
                cx.notify();
            }
            return;
        };
        let Some(operation) = self.begin_transfer(cx) else {
            return;
        };
        self.media.notice = Some("Reading clipboard…".into());
        let task = cx.background_spawn(async move {
            let content = transport.paste(plain)?;
            let (input, warnings) = match content.mime.as_str() {
                notes_exchange::FRAGMENT_MIME => (
                    Import::Fragment(notes_exchange::Fragment::decode(&content.bytes)?),
                    vec![],
                ),
                "text/html" => {
                    let parsed =
                        notes_exchange::html::import(std::str::from_utf8(&content.bytes)?)?;
                    (
                        Import::Fragment(notes_exchange::Fragment {
                            document: parsed.document,
                            bundle: Default::default(),
                        }),
                        parsed.warnings,
                    )
                }
                "text/uri-list" => (
                    Import::Files {
                        paths: notes_exchange::file_urls(&content.bytes)?,
                        images: None,
                    },
                    vec![],
                ),
                "image/png" | "image/jpeg" | "image/webp" | "image/gif" => (
                    Import::Image {
                        bytes: content.bytes,
                        name: format!(
                            "Clipboard image.{}",
                            content.mime.split('/').nth(1).unwrap_or("png")
                        ),
                    },
                    vec![],
                ),
                _ => {
                    let text = std::str::from_utf8(&content.bytes)?
                        .replace("\r\n", "\n")
                        .replace('\r', "\n");
                    (
                        Import::Fragment(notes_exchange::Fragment {
                            document: Document {
                                blocks: text.split('\n').map(Block::paragraph).collect(),
                                ..Default::default()
                            },
                            bundle: Default::default(),
                        }),
                        vec![],
                    )
                }
            };
            Ok::<_, anyhow::Error>((input, warnings))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |s, _, cx| {
                if !s.current_transfer(operation) {
                    return;
                }
                match result {
                    Ok((content, warnings)) => {
                        s.media.notice = Some("Preparing your selection…".into());
                        if !warnings.is_empty() {
                            s.media.warning = Some(warnings.join(" "));
                        }
                        cx.emit(MediaRequest { operation, content });
                    }
                    Err(error) => {
                        s.media.pending = None;
                        s.media.notice = None;
                        s.error = Some(format!(
                            "Couldn’t paste. {error} Use Ctrl Shift V for plain text."
                        ));
                    }
                }
                cx.emit(InputState);
                cx.notify();
            });
        })
        .detach();
    }
    pub fn selected_media(&self) -> Option<&Prepared> {
        let range = self.model.range();
        self.layout
            .as_ref()?
            .objects
            .iter()
            .find(|o| range.start == o.start && range.end == o.start + 3)
            .and_then(|o| self.media.items.get(&o.key))
    }
    pub fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_media().map(|i| i.info.clone()) else {
            return;
        };
        let root = PathBuf::from(&self.library_id);
        let task =
            cx.background_spawn(async move { notes_storage::media::open_copy(&root, &item) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |s, _, cx| {
                match result {
                    Ok(path) => {
                        if let Ok(url) = url::Url::from_file_path(path) {
                            cx.open_url(url.as_str());
                        }
                    }
                    Err(error) => s.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub fn save_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_media().map(|i| i.info.clone()) else {
            return;
        };
        let root = PathBuf::from(&self.library_id);
        let picker = cx.prompt_for_new_path(&root, Some(&item.name));
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            match result {
                Ok(Ok(Some(path))) => {
                    let task = cx.background_spawn(async move {
                        notes_storage::media::export_file(&root, &item, &path)
                    });
                    let result = task.await;
                    let _ = this.update_in(cx, |s, _, cx| {
                        match result {
                            Ok(()) => s.media.notice = Some("File saved.".into()),
                            Err(error) => {
                                s.error = Some(format!("Couldn’t save this file. {error}"))
                            }
                        }
                        cx.notify();
                    });
                }
                Ok(Ok(None)) => {}
                error => {
                    let _ = this.update_in(cx, |s, _, cx| {
                        s.error = Some(format!("File picker unavailable: {error:?}"));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    pub fn media_controls(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let pending = self.media.pending.is_some();
        let ready = self.media.ready.is_some();
        let selected = self
            .selected_media()
            .map(|p| format!("{} · {}", p.info.name, bytes_label(p.info.size)));
        let issue = self.selected_media().and_then(|p| p.info.error.clone());
        div()
            .flex()
            .flex_col()
            .flex_none()
            .when(pending || ready || self.media.notice.is_some(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .px(px(22.))
                        .py(px(8.))
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child(
                            self.media
                                .notice
                                .clone()
                                .unwrap_or_else(|| "Preparing…".into()),
                        )
                        .when(ready, |d| {
                            d.child(self.media_button(
                                "insert-ready",
                                "Insert here",
                                |s, w, cx| s.insert_ready(w, cx),
                                cx,
                            ))
                        })
                        .when(!pending && !ready, |d| {
                            d.child(self.media_button(
                                "dismiss-media",
                                "Dismiss",
                                |s, _, cx| s.cancel_transfer(cx),
                                cx,
                            ))
                        })
                        .when(pending || ready, |d| {
                            d.child(self.media_button(
                                "cancel-media",
                                "Cancel",
                                |s, _, cx| s.cancel_transfer(cx),
                                cx,
                            ))
                        }),
                )
            })
            .when_some(issue, |d, message| {
                d.child(
                    div()
                        .px(px(22.))
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child(message)
                        .child(self.media_button(
                            "retry-media",
                            "Reload file",
                            |s, w, cx| {
                                s.media.stamp.clear();
                                s.refresh_media(w, cx);
                            },
                            cx,
                        )),
                )
            })
            .when_some(selected, |d, label| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .px(px(22.))
                        .py(px(8.))
                        .bg(theme.overlay)
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child(div().flex_1().min_w_0().text_ellipsis().child(label))
                        .child(self.media_button(
                            "open-media",
                            "Open",
                            |s, w, cx| s.open_selected(w, cx),
                            cx,
                        ))
                        .child(self.media_button(
                            "save-media",
                            "Save as…",
                            |s, w, cx| s.save_selected(w, cx),
                            cx,
                        ))
                        .child(self.media_button(
                            "remove-media",
                            "Remove",
                            |s, w, cx| s.edit(|e| e.insert("", 0), w, cx),
                            cx,
                        )),
                )
            })
    }
    pub fn media_button(
        &self,
        id: &'static str,
        label: &'static str,
        action: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .px(px(9.))
            .py(px(5.))
            .rounded(px(5.))
            .border_1()
            .border_color(transparent_black())
            .text_color(theme.muted)
            .hover(move |d| d.bg(theme.overlay).text_color(theme.text))
            .focus_visible(move |d| d.border_color(theme.accent))
            .on_mouse_down(MouseButton::Left, |_, w, _| w.prevent_default())
            .on_click(cx.listener(move |s, _, w, cx| action(s, w, cx)))
            .child(label)
    }
}
pub fn bytes_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024. * 1024.))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
    }
}

pub fn file_kind(mime: &str) -> &'static str {
    match mime {
        "application/pdf" => "PDF document",
        "text/plain" => "Text document",
        "text/markdown" => "Markdown document",
        "text/csv" => "Spreadsheet",
        _ if mime.starts_with("image/") => "Image",
        _ if mime.starts_with("audio/") => "Audio",
        _ if mime.starts_with("video/") => "Video",
        _ => "Attached file",
    }
}
