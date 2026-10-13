---
name: gpui-desktop-apps
description: Best practices for building native desktop applications with GPUI (the Rust UI framework from the Zed editor) outside the Zed repository — architecture, entities and tasks, rendering performance, virtualized lists, text input with IME, accessibility, testing, native verification and build setup. Use when creating, extending, debugging or reviewing a GPUI application.
---

# Building desktop apps with GPUI

This skill comes from building a production-style chat/agent desktop app in GPUI on Linux (Wayland/Hyprland), with macOS packaging, and from reading Zed's own source. It covers what made the app correct, fast and verifiable, and the mistakes that cost the most time.

**GPUI is pre-1.0 and its API moves between Zed revisions.** Treat any method name here as "this is how it worked at one pinned revision". Before relying on a signature, check it in the GPUI source at the revision you pinned. When this file and the source disagree, the source wins.

---

## 1. The rules that matter most

1. **Pure core, thin UI.** Put identities, state machines, queues, availability rules and persistence decisions in a framework-free Rust crate with no GPUI dependency. The UI sends *commands*; the core returns *effects* (work to do) and *notes* (what changed). Most correctness tests then need no window.
2. **Guard every asynchronous result.** Tag each request/response with `(conversation_or_document_id, generation, operation_id)`. Route by id, never by "what is currently selected". Drop results whose generation or operation is no longer current. A late response for item A must land in A even when B is on screen.
3. **Render does no I/O and no parsing.** No file reads, database queries, Markdown parsing, syntax highlighting or expensive text shaping inside `render`. Do it on model updates or background tasks, cache by `(item id, revision)`, and keep the old content visible until the new result arrives.
4. **Use `list` for variable-height rows and `uniform_list` for fixed-height rows.** Streaming content needs explicit remeasuring; structural changes need splices. See §5.
5. **Selection belongs to a document model, not to painted rows.** Address text as `(item id, block index, byte offset)`. Mounted rows only hit-test and paint, and copy reads the model, so offscreen content copies correctly.
6. **Text input is the riskiest component.** Implement the platform input handler with UTF-16 ranges, marked (pre-edit) text and real character bounds. Never submit on Enter while the IME is composing. See §6.
7. **Tasks and subscriptions die when dropped.** Store the `Task`s you need, `.detach()` fire-and-forget work, and keep `Subscription`s in a field. Use the closure's `cx` inside `update`; updating an entity that is already being updated panics.
8. **Accessibility needs element ids.** Roles and labels only exist on elements with an id (`div().id(..).role(..)`), and ids must be unique per frame. Verify the real accessibility tree; don't assume it.
9. **Pin GPUI to one full Zed git revision** in your own workspace, enable the platform features explicitly, and know which of Zed's `[patch.crates-io]` overrides you are *not* inheriting (§11).
10. **Native testing can hit the user's other windows.** Guard every synthetic input so it goes only to a window you launched, and never move, resize or focus windows you didn't create (§9).
11. **Report results honestly.** Separate *measured*, *observed*, *failed* and *unverified*. Never mark IME, screen reader, display scaling, suspend/resume or cold start as passed without doing the real thing.
12. **Parallel workers (people or agents) need owned directories,** an agreed API contract, and a crate that keeps compiling. One broken file blocks everyone's build.

---

## 2. Architecture that scales

```text
app-core (no GPUI)      ids, model types, Command / Effect / Note / Event types,
                        AppState state machine, availability(), Backend trait
app-ui   (GPUI)         theme tokens, assets, Model entity (owns AppState),
                        views/elements, text editor, transcript/list views
app      (binary)       main, controller (executes effects, pumps events),
                        adapters (real backends, demo backend), storage
```

- **Command → Outcome.** `AppState::dispatch(Command) -> Outcome { effects, notes }` and `AppState::apply_event(BackendEvent) -> Outcome`. The GPUI `Model` entity owns `AppState`, runs effects through an installed effect handler, emits notes, and calls `cx.notify()`.
- **Notes drive cheap view updates.** Use fine-grained notes such as `ItemsAppended(id, n)`, `ItemsPrepended(id, n)`, `ItemChanged(id, index)`, `ItemsReset(id)` and `SelectionChanged`. Views map them to list `splice`/`remeasure` calls instead of rebuilding.
- **One `availability()` function** decides which actions are possible right now. Buttons, keyboard shortcuts and the command palette all read it, so they can never disagree.
- **A narrow backend port.** `trait Backend { fn start(&self, sink); fn request(&self, BackendRequest); fn shutdown(&self) }`. Events come back on a channel the adapter was given at construction, never through return values. That makes slow, lost and late responses representable and testable.
- **The controller executes effects; render never does.** It opens storage, builds the backend, restores saved state *before* creating the model entity, and spawns a foreground task. That task drains the backend channel in batches and coalesces consecutive tiny events, such as streamed tokens for one operation, so one paint covers a burst.
- **Keep heavy engines out of the UI process when isolation matters.** If the app drives a long-running engine (an agent, a compiler, a sync service), make the engine a GUI-free library. Host it either in-process (good for tests and benchmarks) or as a private worker mode of the same executable: check a hidden flag like `--engine-worker` at the top of `main`, *before* any GPUI initialization. Talk over inherited stdin/stdout pipes with length-prefixed frames and a versioned protocol. The desktop owns the child, restarts it within a bounded budget, and the worker exits when its stdin closes. No global process discovery, no sockets for the single-app case.
- **Publish only committed state.** If the backend persists data, events should describe changes that are already durable. On a sequence gap or a new backend epoch, the client re-reads a snapshot and asks about anything in doubt. It never guesses, and it never automatically resends a mutation.
- **Deterministic demo backend.** Use your own small PRNG with a fixed seed and fixed base time, versioned scenario scripts embedded with `include_str!`, and configurable speed (speed 0 for tests). Keep a visible "Demo" marker, and never let demo content imply real execution.

### Persistence (SQLite works well)
- Version the schema explicitly (`PRAGMA user_version`), migrate in one transaction, and refuse a database newer than the app understands, leaving it untouched.
- Use `journal_mode=WAL` and `synchronous=FULL` for anything you acknowledge to the user.
- Run **one writer thread** behind a **bounded** queue. A full queue returns an error instead of blocking the UI thread.
- Acknowledge ("Saved") only after the transaction commits **and** the saved revision still matches. If the user edited during the write, the item stays dirty.
- Journal a user's submission durably *before* sending it anywhere, and clear the draft only after that commit. Mark a request whose acknowledgment was lost "outcome unknown" and offer an explicit "check status". Never resend it automatically.
- Write a kill test: spawn the test binary as a child, SIGKILL it after N acknowledged writes, reopen, and assert every acknowledged write survived.

### Syncing a text editor with the core
The editor owns its live text and emits `Changed`, which the core receives as `EditDraft(text)`. When the core itself changes the text (submit clears it, a rejection restores it, a conversation switch replaces it), it bumps a `sync_epoch`. The view's observer then calls `editor.set_text(..)`, and `set_text` must **not** emit `Changed`, or you loop. Do this in the observer, not in `render`.

---

## 3. Entities, tasks, subscriptions

- **Tasks.** Dropping a `Task` cancels it (it's `#[must_use]`). Hold it in a field, `.await` it, `.detach()` it, or `.detach_and_log_err(cx)`.
- **`cx.spawn`.** `cx.spawn(async move |this, cx| ...)` gives a `WeakEntity` and an async context. `this.update(cx, ...)` returns a `Result`, because the entity may be gone.
- **Background work.** `cx.background_spawn` runs `Send` work on other threads. UI work stays on the single foreground thread.
- **Subscriptions.** `cx.subscribe`, `cx.observe` and `cx.on_release` return a `#[must_use] Subscription` that unregisters on drop. Keep them in `_subscriptions: Vec<Subscription>`. The `subscribe_in`/`observe_in` variants also give you the `Window`.
- **Nested updates.** Inside `entity.update(cx, |this, cx| ...)`, use the closure's `cx`, never the outer one. Updating an entity that is already being updated panics. Escape hatch: `cx.defer(...)` runs at the end of the current effect cycle. You need this, for example, inside list scroll callbacks while the list state is borrowed.
- **Back-references.** Use `WeakEntity` for them; mutually recursive strong handles leak.
- **Re-entrant effect handlers.** If an effect handler can re-enter the model, take it out of its `Option` before calling it and put it back afterwards. That avoids a double borrow.
- **Clocks in tests.** Use the executor's timer (`cx.background_executor().timer(d).await`), not another runtime's timer, or "run until parked" won't see it.
- **Logging.** Add a logging backend (e.g. `env_logger`). Otherwise you'll never see GPUI's own diagnostics, such as GPU adapter selection.

---

## 4. Rendering and performance

- **`Render` vs `RenderOnce`.** `Render` (`&mut self` plus a `Context<Self>`) is for stateful views. `RenderOnce` (by value) is for stateless components such as buttons.
- **`cx.notify()`.** Call it when render-affecting state changes. Notifies are deduplicated.
- **Frames are demand-driven.** The compositor frame callback drives drawing, and a window draws only when something is dirty. Dirtiness propagates to ancestor views only. Don't notify the whole window for a streamed token: notify the streaming entity and invalidate only the streaming item.
- **Child entities re-render every frame their parent draws.** `Entity::cached(style)` / `AnyView::cached(..)` reuse the previous subtree unless that entity notified, the window refreshed, or bounds/mask/text style changed. It needs a definite size. Zed uses it only for large panels (docks, pane groups). Reach for it only after measuring.
- **Element ids** must be unique per frame. Two views keyed by the same entity must not be siblings. Use keyed state (`use_keyed_state`) for list items; `use_state` keys by code location. In loops, give text and elements explicit ids (`.with_id(i)`, `div().id(i)`).
- **Motion.** `window.refresh()` bypasses caches. Decorative motion should use the built-in animation helpers, which respect reduced motion. Never animate the whole window. Stop caret blinking when reduced motion is on, and after idle.
- **Text layout cache.** The line layout cache is two-frame (reuse within consecutive frames, not an LRU). Changing the wrap width forces re-wrapping. Cheap `render` + this cache is how Zed stays fast.
- **What Zed does.** One foreground thread. Parsing and highlighting on background tasks, coalesced to one in-flight job plus one follow-up. Results swapped in on the foreground, with old content visible until then. Code-block highlights reused while streaming when blocks are unchanged.
- **Profiling.** Zed's `profiler` cargo feature (frame journals, an FPS overlay that paints raw quads so it can't cause frames, a hang monitor) and its measurement environment flag are worth borrowing. At minimum, build a developer command that logs frame p50/p95, input-to-frame latency and RSS.
- **Label what you measure.** CPU time from input handler to paint-end is "frame submission", not "display presentation". Say so wherever you quote it.

---

## 5. Lists, streaming and scroll anchoring

`list(ListState, |ix, window, cx| ...)` caches item heights. Items outside the viewport must not change height unless you tell the list. Visible items re-render every frame. A width change invalidates **all** cached heights.

- **Construction.** `ListState::new(count, ListAlignment::Top | Bottom, overdraw_px)`. For a chat transcript, Zed's agent panel uses Top alignment with about 2048 px overdraw and tail-follow.
- **Streaming one item:** `remeasure_items(ix..ix+1)`. **New item:** `splice(len..len, 1)`. **Removal:** `splice(range, 0)`. **Font-size change:** `remeasure()`, which keeps a proportional position. `reset(n)` drops scroll state, so use it sparingly.
- **Follow the tail.** `set_follow_mode(FollowMode::Tail)`. Scrolling up stops following; returning within about 1 px of the bottom resumes it. On completion, if still following, call `scroll_to_end()` again so the final height is pinned.
- **Prepend history without a jump:**
  ```rust
  let top = list.logical_scroll_top();
  list.splice(0..0, n);
  list.scroll_to(ListOffset { item_ix: top.item_ix + n, offset_in_item: top.offset_in_item });
  ```
  Test it: after a prepend, the anchor must sit at `item_ix + n` with the same offset.
- **Reentrancy.** The scroll handler runs while the list state is borrowed. Reading `logical_scroll_top()` there panics, so defer with `cx.defer`.
- **Fixed header row.** Put "Loading earlier…" / "Beginning of conversation" at index 0 with a fixed height, so loading a page never shifts content. Request older pages once, near the top, guarded by a `loading_older` flag.
- **One list state per document**, plus per-item UI state (expanded/collapsed) keyed by stable item id, in a small bounded map. Switching away and back then restores scroll anchor, expansion and selection. Never keep that state in the render closure.
- **`uniform_list` sizing.** `uniform_list(id, count, |range, window, cx| rows)` suits fixed-height rows (diffs, large directories). Pitfall: inside a `flex_1().min_h_0()` wrapper it rendered **zero rows**. Give it `.size_full()` inside a sized parent. For horizontal scroll, use unconstrained horizontal sizing with width taken from the widest item, and set rows to `min_w_full`.
- **Measure boundedness.** On a 10,000-item conversation the app mounted only 4–6 rows, and transcript frame CPU p95 was about 1.7 ms. Write tests asserting mounted-row bounds for very large lists (thousands of rows and entries) and that paging reaches the oldest item exactly once with no duplicates.

### Markdown while streaming
- **Parsing.** Parse with `pulldown-cmark` into your own block model (paragraphs, headings, lists, quotes, fenced code, inline styles, links), keeping plain selectable text plus styled runs.
- **Never panic on malformed input.** Unterminated Markdown during streaming must render sensibly; an open fence renders as code.
- **Caching.** Cache parsed blocks by `(item id, content revision)` in a bounded LRU, and reparse only the streaming tail per token batch.
- **Wrap code blocks** instead of scrolling them horizontally. That keeps hit-testing exact and avoids nested scroll regions.
- **Syntax highlighting.** A small tokenizer is fine for a prototype; use tree-sitter or syntect for production, computed off the render path.

### Document-level selection
- Represent selection as `Selection { anchor, head }`, with positions `(item id, block, byte offset)` that are independent of mounted rows. `selected_text(items)` walks the model, so copy includes offscreen rows.
- Each frame, every mounted text block registers its bounds and wrapped lines in a hit-test registry that is **cleared and repopulated every frame**. That's what makes recycled rows safe.
- Mouse: find the block under the pointer, clamping to the first or last visible block when outside, then use `closest_index_for_position`. Auto-scroll while dragging past the edges.
- Keyboard (in its own key context): select all, copy, shift-arrows, shift-home/end, Escape. Double-click selects a word; triple-click selects a paragraph.
- Clamp offsets when streaming reshapes a block.

---

## 6. Text input and IME (the hard part)

**Split the editor in two:**
1. A **pure editing model**, fully unit-tested, covering:
   - bytes, grapheme boundaries and UTF-8 ↔ UTF-16 conversion;
   - selection with reversal, and a marked (pre-edit) range;
   - undo/redo with typing coalescing and IME-commit collapse;
   - word/line motion and CRLF normalization.

   Test emoji ZWJ sequences, flags, combining marks, UTF-16 round trips, undo through emoji edits, and marked-text commit/cancel.
2. A **GPUI entity plus a custom `Element`** that paints and connects to the platform.

GPUI's `examples/input.rs` is a starting point. It is single-line, returns no IME bounds and ignores most editing behavior.

**Platform contract** (implement the entity input handler):
- **Ranges.** Every range is **UTF-16**. Implement `text_for_range` (and fill in the actual range), `selected_text_range`, `marked_text_range`, `unmark_text`, `replace_text_in_range`, `replace_and_mark_text_in_range`, `bounds_for_range` and `character_index_for_point`. `bounds_for_range` must return real caret rectangles so the candidate window appears in the right place.
- **Registration.** Register the handler **during paint**: `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx)`. It is active only while focused, and only for the next frame.
- **Disabled editors.** `accepts_text_input` drives IME enablement each frame (on Wayland, through the text-input protocol). A disabled editor must return false and be read-only.
- **Call `window.invalidate_character_coordinates()` whenever the caret moves.** Otherwise the IME popup stays at the old position.
- **Enter while composing goes to the IME.** Emit "submit" only when there is no marked range. Shift+Enter inserts a newline. Test composition by calling `replace_and_mark_text_in_range` and then sending Enter.
- **Practices worth copying from Zed's editor:**
  - clip offsets to character boundaries and report the adjusted range;
  - during composition, treat an empty replacement range as "insert at cursor";
  - disable auto-close and auto-surround while composing;
  - return `None` from `selected_text_range` when input is disabled;
  - underline marked text.

**Layout:**
- **Shaping.** `shape_line` is single-line only. Use `shape_text(text, size, runs, wrap_width, ..)`, which returns one wrapped line per `\n`-separated line; positions are relative to each line's origin. Accumulate each line's height for vertical offsets.
- **Runs.** Text runs must cover the whole string. Runs cannot change font size, so inline code can't be smaller than the surrounding prose in one paragraph.
- **Height.** Use measured layout so the composer grows with wrapped content up to N lines (8 worked well), then scrolls internally while keeping the caret visible. Pass the mouse wheel through when it can't scroll.
- **Motion.** Home/End move by *visual* wrapped row. Up/Down use wrapped-layout positions with a remembered goal column.
- **Single-line variant.** Offer a single-line mode for palette, rename and search fields: no wrap, newlines become spaces, Up/Down are emitted as events.

**Verify with a real IME** (e.g. fcitx5 or ibus with a composing engine) and a screen reader before claiming support. Tests prove only the logic.

---

## 7. Elements, styling, overlays, keyboard

- **Interactive and accessibility methods live on the stateful div.** `div().id("x").role(Role::Button).aria_label("…")`. Without `.id()` those methods don't exist.
- **Focus.** `track_focus(&handle)` makes an element focusable; `tab_stop(true)`/`tab_index(n)` place it in the tab order, and `tab_group()` scopes indices. `focus_visible(..)` styles appear only after keyboard input.
- **Key bindings.** Combine `.key_context("Name")` with `.on_action(cx.listener(..))` and `track_focus`. Bindings are scoped by context, and actions reach only the focused path. Don't put a key context with Up/Down/Enter bindings on an overlay that contains a text input, or its bindings will shadow the editor's.
- **Menus and overlays.** `deferred(child).with_priority(n)` paints above ancestors while keeping layout. Add a full-window backdrop with `.occlude()` and a click handler for dismissal. `anchored()` children must have no margin.
- **Splitter dragging.** Use `.on_drag(Marker, ..)` on the divider and `.on_drag_move::<Marker>(..)` on the root. Keep a **local live width** during the drag and commit one "set width" command on mouse-up, instead of writing preferences on every move.
- **Icons and fonts.** `svg().path("icons/x.svg").text_color(c)` colors stroke icons that use `currentColor`. Ship assets through an `AssetSource` registered on the application builder, and register fonts with `cx.text_system().add_fonts(..)`. Record provenance and licences for every bundled asset.
- **Responsive layout.** Base it on `window.viewport_size()` in render; resizes re-render automatically. Test several widths, e.g. 1440, 1024 and 720 px, with a navigation drawer at narrow widths.
- **File pickers.** `cx.prompt_for_paths(..)` returns a channel of `Result<Option<Vec<PathBuf>>>`: `Err` means no picker is available (show a message), `None` means the user cancelled. Do file metadata work on a background task.
- **Window options.** Set `app_id` and the titlebar title in the window options, and update the title from an observer.
- **Theming.** Define semantic tokens (surface, text, muted, accent, danger, syntax colors) for light and dark. Never hard-code colors in views.

---

## 8. Accessibility

- **Which elements become nodes.** On Linux, GPUI exposes accessibility through AccessKit to AT-SPI. Only elements with a global element id *and* a role become nodes, and duplicate ids are silently dropped in release builds.
- **Give the app real structure:**
  - landmarks for navigation, content and inspector;
  - a `log` role for a transcript, with an `article` per message;
  - a status element for run state;
  - accessible names on headings and inputs.

  Headings and inputs often come out unnamed or wrongly named until you label them explicitly.
- **Dump the real tree.** On Linux, use Python with `gi.repository.Atspi`: find the app under the desktop and walk its children, printing role and name. Do it after every structural change.
- **Known limits (at the revision used):**
  - caret and selection are not exposed as AT-SPI text;
  - only mounted rows of virtualized lists appear.

  Make every action reachable from a keyboard path such as the command palette, so virtualized content is still operable.

---

## 9. Testing

**Layers that paid off:**
1. **Pure core tests.** Run the state machine through every scenario with the demo backend at speed 0: stale-guard drops, run-state transitions, lost acknowledgments, queue rules, draft/save rules. Prove determinism by comparing the `Debug` output of event sequences across identical seeds.
2. **Pure model tests** for the editor model, selection, Markdown and diff parsing.
3. **GPUI tests** with `#[gpui::test]` and `TestAppContext`/`VisualTestContext`. Simulate keystrokes and input, and check:
   - Enter vs Shift+Enter vs Enter while composing;
   - IME commit and cancel, and that `set_text` doesn't emit change events;
   - list anchoring after prepends;
   - tail-follow pause and resume;
   - per-document scroll restore;
   - bounded mounted rows for huge lists;
   - drag selection across rows.

   Add `gpui` as a dev-dependency with the `test-support` feature.
4. **Headless end-to-end workflow tests** that drive the real core state plus the real backend adapter plus real storage against a deterministic fake provider. Cover submit/settle/reopen, lost acknowledgment resolved by lookup (never resent), stop then new prompt, queues, restart mid-run, attachments, history paging, and copy of complete output. These caught more real bugs than anything else.
5. **Crash and fault tests.** SIGKILL a real child process at each durable boundary, inject storage failures (full disk, read-only, failure at a specific phase), and corrupt the database. Assert nothing is lost, duplicated or falsely reported as successful.

**Caveats:**
- GPUI's test text system is a no-op, so layout metrics in tests aren't real glyph metrics. Confirm wrapping and long tokens natively.
- A test that waits on time should use the executor timer.
- Don't let a test depend on wall-clock races. Wait on conditions, with generous deadlines and useful timeout messages.
- If a test fails once under heavy parallel load, find the race (often in the test harness) rather than retrying it away.

### Native testing on the real desktop (Linux/Wayland example)
- Launch a **release** build with a **throwaway data directory and profile**. Use a local scripted backend instead of real accounts or paid services.
- Find your window through the compositor (Hyprland: `hyprctl clients -j`, matched by your `app_id`/pid). Take screenshots with `grim -g "X,Y WxH"` **using the geometry of your window only**, read *after* the window has been placed. Don't capture the whole screen; other windows may contain private content.
- Keyboard input (`wtype`) goes **only through a guard script** that:
  - pins the window you launched;
  - refuses unless that exact window is active, on the current workspace;
  - allows only plain text, named keys and ctrl/shift chords (no Super/Alt, which the compositor may bind);
  - re-checks after every key.

  Focus can be stolen mid-run.
- **Never** move, resize, focus or switch workspaces for windows you didn't launch, edit the user's compositor configuration, install packages, or change display scale without explicit approval.
- Mouse automation needs a virtual pointer device. Keep pointer events inside a region your window owns, and don't assume it's on top.
- Kill your own processes by pid. A `pkill -f <pattern>` can match the shell running the command itself.
- **Close what you started**, including test runs left in the background. Check for leftover processes before saying you're done.
- **Persistence check:** type, wait for "saved", `kill -9`, relaunch with the same data dir, and confirm the state came back.

---

## 10. Verification and reporting

- Keep four buckets: **measured** (a number from a defined method), **observed** (seen happen in the real app), **failed**, **unverified** (with the reason).
- Label every number: what it measures, on which machine and build, under what load, and with what sample size.
- **Unverified until done for real:** real IME, screen reader, 125%/150% scaling, suspend/resume, minimize/restore, true cold boot (needs dropping caches), display-presentation latency, long-session memory.
- Re-run the full checks (`cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --all --check`) on the exact tree you report. Merged branches can conflict semantically even when git merges them cleanly.
- Keep a visible "Demo / simulated" marker on demo modes so screenshots can't be mistaken for real execution.

---

## 11. Build, dependencies, toolchain

- **Workspace and pinning.** Use an independent workspace and pin `gpui` and `gpui_platform` to the **same full Zed git revision**. Use `default-features = false` with explicit features, e.g. `["wayland", "x11", "font-kit"]` on Linux. Pin the Rust toolchain in `rust-toolchain.toml` (Zed's own toolchain version is a good guide).
- **Entry point.** Build the app with the platform crate's `application()` and `.with_assets(..)`. Don't copy APIs from other Zed revisions.
- **Zed's `[patch.crates-io]` overrides are NOT inherited by your workspace.** Know what you are skipping:
  - Zed patches **`calloop`** with a fork that bounds the time spent per event-loop tick. Upstream runs every ready future per tick regardless of duration, which can freeze the UI under slow futures. Seriously consider applying the same patch for Linux apps, and re-measure.
  - Some patches are build-size or build-time wins only (e.g. `async-task`).
- **Feature flags.** `test-support` turns on platform backends for tests. `inspector`, `profiler` and `leak-detection` are separate opt-ins.
- **Linux system packages** typically needed: wayland-client, xkbcommon, a Vulkan loader and driver, fontconfig, freetype, and cmake/clang for bundled C dependencies such as SQLite.
- **Licensing.** GPUI is Apache-2.0. Zed's higher-level crates (`ui`, `editor`, `markdown`, `agent_ui`, …) are GPL. **Read them for ideas; don't import or paste them** into a non-GPL app. Keep a licence and runtime inventory for releases.
- **Packaging.** Measure installed and archive bytes, strip release binaries for size measurements (`debug = "line-tables-only"` keeps useful backtraces), list the shared libraries the binary needs, and test the installed app with a bare environment (`env -i PATH=/usr/bin:/bin`) in a scratch HOME.
- **GPU selection and transparency.** GPUI prefers the compositor's GPU. It may choose an integrated GPU over a discrete one on hybrid machines, which is usually right for a desktop app. Wayland windows may be created transparent, and some desktop environments apply window opacity rules, so see-through screenshots aren't necessarily your bug.

---

## 12. Working process

- **Build in dependency order.**
  1. Native foundation: the window opens on the target platform, and the GPU path is recorded.
  2. The framework-free core: types, protocol, state machine, tests.
  3. The model entity and effect handler.
  4. Text input and virtualization *inside the real app*, not throwaway spikes.
  5. Workflows.
  6. The visual system.
  7. A batched review, then a correction pass.
- **Read the relevant Zed source right before implementing** the equivalent piece (lists, editor, markdown), at your pinned revision.
- **Parallel workers.**
  - Give each worker an owned set of files, pre-create module stubs and `pub mod` lines, and state the API contract (constructors, events, semantics) in the assignment.
  - Each worker should report gaps rather than edit files it doesn't own.
  - Verify every hand-back yourself: re-run the tests and launch the app.
- **Scope is the owner's decision.** If you find more work than was agreed, report it and ask before starting it. Say plainly what is done, what isn't, and what is still running.
- **Report incidents plainly.** For example: focus theft, a screenshot that captured other windows, a test left running, a file committed by mistake. Fix what you can and say what you changed.

---

## Quick reference: snippets

**Spawn background work and update an entity safely:**
```rust
let task = cx.spawn(async move |this, cx| {
    let parsed = cx.background_executor().spawn(async move { parse(input) }).await;
    this.update(cx, |this, cx| {
        this.parsed = Some(parsed);
        cx.notify();
    })
    .ok(); // the entity may have been dropped
});
self._parse_task = Some(task); // keep it, or it is cancelled
```

**Keep subscriptions alive:**
```rust
self._subscriptions.push(cx.subscribe(&model, |this, _model, note: &Note, cx| {
    this.apply_note(note, cx); // splice/remeasure the list; no rebuild
}));
```

**Prepend without moving the reader:**
```rust
let top = self.list.logical_scroll_top();
self.list.splice(0..0, added);
self.list.scroll_to(ListOffset { item_ix: top.item_ix + added, offset_in_item: top.offset_in_item });
```

**Stale-guarded event application (core, no GPUI):**
```rust
fn apply_event(&mut self, ev: BackendEvent) -> Outcome {
    let Some(conv) = self.conversations.get_mut(&ev.conversation) else { return Outcome::default() };
    if ev.generation != conv.generation { return Outcome::default(); }            // stale attachment
    if ev.is_op_scoped() && ev.op != conv.run.op() { return Outcome::default(); } // stale operation
    // ...apply, then return effects + fine-grained notes
}
```
