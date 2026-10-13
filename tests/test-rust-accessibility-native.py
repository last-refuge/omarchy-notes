#!/usr/bin/env python3
"""Opt-in AT-SPI + Hyprland test. Uses a private bus, copied library and owned window.
No desktop settings, clipboard access, screenshots or external viewers. Requires
python-gobject, at-spi2-core, dbus-run-session, hyprctl and wtype. This is an API
check, not an Orca usability pass or proof of IME support.
"""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import uuid
from rust_native_test import InstanceSocket, guard_nested_desktop

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("NOTES_NATIVE_QA_BINARY", REPO / "target/release/omarchy-notes-rust"))


def stop(process):
    if process and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def run():
    if not __debug__:
        raise RuntimeError("Native ownership guards require Python assertions; do not use -O or PYTHONOPTIMIZE")
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, Gio, GLib
    diagnostic_output = os.environ.get("NOTES_NATIVE_QA_OUTPUT")
    diagnostic_output = Path(diagnostic_output) if diagnostic_output else None
    if diagnostic_output:
        diagnostic_output.mkdir(parents=True, exist_ok=False)

    def wait(predicate, description, timeout=12):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            context = GLib.MainContext.default()
            while context.pending():
                context.iteration(False)
            result = predicate()
            if result:
                return result
            time.sleep(.05)
        raise AssertionError(f"Timed out: {description}")

    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, 0, 3000, None)
    # The isolated session deliberately has no systemd user manager for activation.
    registry = subprocess.Popen(["/usr/lib/at-spi2-registryd"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    process = None
    instance_socket = None
    try:
        with tempfile.TemporaryDirectory(prefix="notes-accessibility-") as tmp:
            root = Path(tmp)
            os.environ["XDG_CONFIG_HOME"] = str(root / "config")
            library = root / "library"
            shutil.copytree(REPO / "tests/fixtures/qt-v1", library)
            source = json.loads((library / "documents.json").read_text())
            original = next(n for n in source if n["title"].startswith("Q3 planning"))
            media = [b for b in original["body"]["blocks"] if any("image" in s or "attachment" in s for s in b.get("spans", []))]
            content = "Accessible notes\ne\u0301 👩‍👩‍👧‍👦 中文 العربية\nFinal paragraph"
            blocks = [{"id": f"text-{i}", "type": "paragraph", "spans": [{"text": t}]} for i, t in enumerate(content.split("\n"))]
            blocks[0]["spans"][0]["marks"] = ["bold", "italic", "underline"]
            blocks[0].update({"type": "heading", "level": 1})
            blocks[2]["spans"][0]["href"] = "note://00000000-0000-4000-8000-000000000001"
            blocks += [{"id": "native-check", "type": "list", "list": "check", "checked": False, "spans": [{"text": "Review accessibility"}]}]
            blocks += media
            blocks += [{"id": f"offscreen-{i}", "type": "paragraph", "spans": [{"text": f"Offscreen paragraph {i}"}]} for i in range(20)]
            expected = "\n".join("".join(s.get("text", "\ufffc") for s in b["spans"]) for b in blocks)
            with sqlite3.connect(library / "library.sqlite3") as db:
                db.execute("UPDATE notes SET deleted_at = 1 WHERE id != ?", (original["id"],))
                db.execute("UPDATE notes SET title = ?, body = ?, plain_text = ? WHERE id = ?", ("Accessible notes", json.dumps({"schema": 1, "blocks": blocks}), expected, original["id"]))
            with (root / "app.log").open("w") as log:
                app_env = dict(os.environ)
                if os.environ.get("NOTES_NATIVE_QA_TRACE"):
                    app_env["WAYLAND_DEBUG"] = "client"
                process = subprocess.Popen([str(BINARY), "--data-dir", str(library), "--reference"], env=app_env, stdout=log, stderr=log)
            address = None

            def compositor(kind):
                return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))

            def guard():
                guard_nested_desktop()
                assert process.poll() is None, "Owned process exited"
                active, workspace = compositor("activewindow"), compositor("activeworkspace")
                assert active.get("pid") == process.pid and active.get("address") == address and active.get("class") == "org.omarchy.Notes.Rust", "Refusing input: owned test window lost focus"
                assert active["workspace"]["id"] == workspace["id"], "Refusing input: workspace changed"

            def key(name, ctrl=True, shift=False):
                assert name in ("z", "s", "n", "k", "f", "a", "b", "x", "y", "e", "h", "m", "q", "7", "8", "r", "space", "F3", "Tab", "Return", "Escape")
                # wtype's generated keymap sends the requested keysym. Shift
                # does not transform digit keysyms as a physical US layout does.
                if shift:
                    name = {"7": "ampersand", "8": "asterisk"}.get(name, name)
                args = ["wtype"] + (["-M", "ctrl"] if ctrl else []) + (["-M", "shift"] if shift else []) + ["-k", name] + (["-m", "shift"] if shift else []) + (["-m", "ctrl"] if ctrl else [])
                guard()
                subprocess.run(args, check=True, timeout=3)
                guard()

            def type_text(value):
                def focused_text():
                    return next((n.get_text_iface() for n in descendants(app) if "Text" in n.get_interfaces() and n.get_state_set().contains(Atspi.StateType.FOCUSED)), None)
                provider = wait(focused_text, "text input focus before typing")
                for char in value:
                    assert char.isprintable()
                    before = Atspi.Text.get_text(provider, 0, -1)
                    if Atspi.Text.get_n_selections(provider):
                        selection = Atspi.Text.get_selection(provider, 0)
                        start, end = sorted((selection.start_offset, selection.end_offset))
                    else:
                        start = end = Atspi.Text.get_caret_offset(provider)
                    expected_input = before[:start] + char + before[end:]
                    guard()
                    subprocess.run(["wtype", "--", char], check=True, timeout=3)
                    guard()
                    try:
                        wait(lambda: Atspi.Text.get_text(provider, 0, -1) == expected_input, "native input acknowledgement for " + repr(char))
                    except AssertionError:
                        print("Input diagnostics:", repr(expected_input), repr(Atspi.Text.get_text(provider, 0, -1)), flush=True)
                        raise

            def owned_window():
                assert process.poll() is None, (root / "app.log").read_text()[-2000:]
                return next((w for w in compositor("clients") if w.get("pid") == process.pid and w.get("class") == "org.omarchy.Notes.Rust"), None)

            address = wait(owned_window, "owned window")["address"]
            guard()
            instance_socket = InstanceSocket(library, process)

            def owned_app():
                desktop = Atspi.get_desktop(0)
                return next((a for i in range(desktop.get_child_count()) if (a := desktop.get_child_at_index(i)).get_process_id() == process.pid), None)

            app = wait(owned_app, "AT-SPI app registration")

            def descendants(node):
                if node is None:
                    return  # A menu can remove a child between the two queries.
                yield node
                for i in range(node.get_child_count()):
                    yield from descendants(node.get_child_at_index(i))

            def find(name):
                return next((n for n in descendants(app) if n.get_name() == name), None)

            editor = wait(lambda: find("Note content"), "editor node")
            assert "Text" in editor.get_interfaces()
            text = editor.get_text_iface()
            wait(lambda: Atspi.Text.get_text(text, 0, -1) == expected, "complete text including offscreen content")
            assert Atspi.Text.get_character_count(text) == len(expected)
            assert Atspi.Text.get_text_attribute_value(text, 0, "weight") == "700"
            assert Atspi.Text.get_text_attribute_value(text, 0, "style") == "italic"
            assert Atspi.Text.get_text_attribute_value(text, 0, "underline") == "single"
            heading = wait(lambda: next((n for n in descendants(editor) if n.get_role() == Atspi.Role.HEADING and n.get_name() == "Accessible notes"), None), "named heading semantics")
            assert heading.get_role() == Atspi.Role.HEADING
            link = wait(lambda: next((n for n in descendants(editor) if n.get_role() == Atspi.Role.LINK), None), "inline link semantics")
            assert link.get_name() == "Final paragraph"
            assert "Action" in link.get_interfaces()
            assert any(n.get_role() == Atspi.Role.LIST for n in descendants(editor))
            rect = Atspi.Text.get_character_extents(text, 0, Atspi.CoordType.WINDOW)
            assert rect.width > 0 and rect.height > 0 and rect.x >= 0 and rect.y >= 0
            offset = Atspi.Text.get_offset_at_point(text, rect.x + rect.width // 2, rect.y + rect.height // 2, Atspi.CoordType.WINDOW)
            assert offset == 0, offset
            def undo_diagnostics(label):
                active = compositor("activewindow")
                selections = [Atspi.Text.get_selection(text, i) for i in range(Atspi.Text.get_n_selections(text))]
                # Diagnostic metadata can outlive a removed status/popover node.
                # Resolve the current owned frame and snapshot each live node
                # once; preserve defunct entries rather than aborting the actual
                # text/undo/save assertions below.
                nodes, defunct = [], []
                def snapshot_node(node, path):
                    if node is None:
                        return
                    try:
                        node.clear_cache()
                        states = node.get_state_set()
                        nodes.append({"name": node.get_name(), "role": node.get_role_name(),
                                      "enabled": states.contains(Atspi.StateType.ENABLED),
                                      "sensitive": states.contains(Atspi.StateType.SENSITIVE),
                                      "focused": states.contains(Atspi.StateType.FOCUSED)})
                        children = [node.get_child_at_index(i) for i in range(node.get_child_count())]
                    except GLib.GError as error:
                        defunct.append({"path": path, "error": str(error)})
                        return
                    for i, child in enumerate(children):
                        snapshot_node(child, path + [i])
                current_app = owned_app()
                if current_app:
                    current_app.clear_cache()
                    for i in range(current_app.get_child_count()):
                        frame = current_app.get_child_at_index(i)
                        try:
                            frame.clear_cache()
                            if frame.get_name() == active.get("title"):
                                snapshot_node(frame, [i])
                        except GLib.GError as error:
                            defunct.append({"path": [i], "error": str(error)})
                buttons = {name: [{"enabled": n["enabled"], "sensitive": n["sensitive"]}
                                 for n in nodes if n["name"] == name]
                           for name in ("Undo", "Redo")}
                state = {"label": label, "text": Atspi.Text.get_text(text, 0, -1),
                         "caret": Atspi.Text.get_caret_offset(text),
                         "selections": [[s.start_offset, s.end_offset] for s in selections],
                         "focused_owned_nodes": [(n["role"], n["name"]) for n in nodes if n["focused"]],
                         "owned_window_active": active.get("pid") == process.pid and active.get("address") == address,
                         "active_pid": active.get("pid"), "owned_pid": process.pid,
                         "undo_redo_accessible_states": buttons, "app_exit_status": process.poll(),
                         "cached_save_status_names": [n["name"] for n in nodes if n["role"] in ("status", "status bar")],
                         "defunct_diagnostic_entries": defunct,
                         "owned_composition_protocol_events": [line for line in (root / "app.log").read_text().splitlines()
                                                               if "text_input" in line and ("preedit_string(" in line or "commit_string(" in line)],
                         "operation": "named Undo action" if os.environ.get("NOTES_NATIVE_QA_FOCUS") == "undo-button" else "Ctrl-Z"}
                print("Undo diagnostics:", json.dumps(state, ensure_ascii=False), flush=True)
                if diagnostic_output:
                    (diagnostic_output / (label + ".json")).write_text(json.dumps(state, indent=2, ensure_ascii=False) + "\n")
                    shutil.copyfile(root / "app.log", diagnostic_output / (label + "-app.log"))
            first = expected.index("👩")
            last = first + len("👩‍👩‍👧‍👦")
            guard()
            assert Atspi.Text.set_selection(text, 0, first, last)
            guard()
            wait(lambda: Atspi.Text.get_n_selections(text) == 1 and Atspi.Text.get_caret_offset(text) == last, "Unicode selection")
            selected = Atspi.Text.get_selection(text, 0)
            assert (selected.start_offset, selected.end_offset) == (first, last)
            guard()
            subprocess.run(["wtype", "--", "X"], check=True, timeout=3)
            guard()
            changed = expected[:first] + "X" + expected[last:]
            try:
                wait(lambda: Atspi.Text.get_text(text, 0, -1) == changed, "edit at accessible selection")
            except AssertionError:
                undo_diagnostics("failed-native-edit")
                raise
            if os.environ.get("NOTES_NATIVE_QA_FOCUS") == "edit":
                undo_diagnostics("after-native-edit")
                print("PASS: focused cached-client native Unicode replacement; full suite not run.", flush=True)
                return
            undo_diagnostics("before-native-undo")
            if os.environ.get("NOTES_NATIVE_QA_FOCUS") == "undo-button":
                undo_button = find("Undo")
                assert undo_button is not None
                guard()
                assert Atspi.Action.do_action(undo_button.get_action_iface(), 0)
                guard()
            else:
                key("z")
            try:
                wait(lambda: Atspi.Text.get_text(text, 0, -1) == expected, "undo preserves rich document")
            except AssertionError:
                undo_diagnostics("failed-native-undo")
                raise
            undo_diagnostics("after-native-undo")
            if os.environ.get("NOTES_NATIVE_QA_FOCUS") in ("undo", "undo-button"):
                print("PASS: focused cached-client native Unicode replacement/undo; full suite not run.", flush=True)
                return
            check = wait(lambda: find("Review accessibility"), "checkbox name")
            assert not check.get_state_set().contains(Atspi.StateType.CHECKED)
            guard()
            assert Atspi.Action.do_action(check.get_action_iface(), 0)
            guard()

            def is_checked():
                check.clear_cache()
                return check.get_state_set().contains(Atspi.StateType.CHECKED)

            wait(is_checked, "checkbox toggled state")
            assert find("Image: Bar chart of search latency by week")
            wait(lambda: any(n.get_name().startswith("Attachment: ") and not n.get_name().endswith("Attachment") for n in descendants(app)), "attachment filename")
            key("s")

            def saved():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    body = json.loads(db.execute("SELECT body FROM notes WHERE id = ?", (original["id"],)).fetchone()[0])
                check = next(b for b in body["blocks"] if b["id"] == "native-check")
                return check["checked"] and body["blocks"][1]["spans"][0]["text"] == content.split("\n")[1]

            wait(saved, "durable checkbox and Unicode undo")
            key("n")

            def empty_editor():
                fresh = find("Note content")
                if fresh and "Text" in fresh.get_interfaces():
                    provider = fresh.get_text_iface()
                    if Atspi.Text.get_character_count(provider) == 0:
                        return provider
                return None

            empty = wait(empty_editor, "new empty accessible editor")
            guard()
            assert Atspi.Text.set_caret_offset(empty, 0)
            guard()
            wait(lambda: Atspi.Text.get_caret_offset(empty) == 0, "empty caret")
            guard()
            subprocess.run(["wtype", "--", "Z"], check=True, timeout=3)
            guard()
            wait(lambda: Atspi.Text.get_text(empty, 0, -1) == "Z", "typing into empty accessible input")
            key("s")

            def new_saved():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    return db.execute("SELECT COUNT(*) FROM notes WHERE plain_text = 'Z' AND deleted_at IS NULL").fetchone()[0] == 1

            wait(new_saved, "new note saved")
            def click(name):
                node = wait(lambda: find(name), name)
                guard()
                assert Atspi.Action.do_action(node.get_action_iface(), 0)
                guard()

            def table_shape(rows, columns):
                return any(n.get_name().startswith(f"Table, {rows} rows and {columns} columns.") for n in descendants(app))

            def editor_focused():
                editor = find("Note content")
                return editor and editor.get_state_set().contains(Atspi.StateType.FOCUSED)

            def undo_table(rows, columns, description):
                # A tree change and returned input focus are separate native
                # notifications. Do not send Undo while focus is still on the
                # action panel that was just removed.
                wait(editor_focused, "editor focus after table action")
                key("z")
                try:
                    wait(lambda: table_shape(rows, columns), description)
                except AssertionError:
                    print("Table undo diagnostics:", [(n.get_name(), n.get_state_set().contains(Atspi.StateType.FOCUSED)) for n in descendants(app) if n.get_name().startswith("Table,") or n.get_name() == "Note content"], flush=True)
                    raise

            click("Insert a 2 by 2 table")
            wait(lambda: table_shape(2, 2), "inserted native table")
            wait(lambda: any(n.get_role() == Atspi.Role.TABLE for n in descendants(find("Note content"))), "native table semantics")
            assert any(n.get_role() == Atspi.Role.TABLE_CELL for n in descendants(find("Note content")))
            for word in ("One", "Two", "Three", "Four"):
                type_text(word)
                provider = find("Note content").get_text_iface()
                # The synthetic table's cells are separated by one newline in
                # AT-SPI text. A completed wtype process only queues Tab; await
                # the actual next-cell caret before the next input oracle reads.
                next_cell = Atspi.Text.get_caret_offset(provider) + 1
                key("Tab", ctrl=False)
                wait(lambda: Atspi.Text.get_caret_offset(provider) == next_cell,
                     "native Tab acknowledged at next table cell")
            wait(lambda: table_shape(3, 2), "Tab appends row")
            click("Table options")
            wait(lambda: find("Row below"), "table options loaded")
            assert not find("Split merged cell"), "only applicable commands belong in table options"
            click("Row below")
            wait(lambda: table_shape(4, 2), "row insertion")
            undo_table(3, 2, "undo row insertion")
            click("Table options"); click("Column after")
            wait(lambda: table_shape(3, 3), "column insertion")
            click("Table options"); click("Delete column")
            wait(lambda: table_shape(3, 2), "column deletion")
            undo_table(3, 3, "undo column deletion")
            click("Table options"); click("Remove table")
            wait(lambda: not table_shape(3, 3), "table removal")
            undo_table(3, 3, "undo table removal")
            provider = find("Note content").get_text_iface()
            # insert_table already created this fixture's trailing paragraph;
            # leave_table moves into it without changing text. First await Undo's
            # restored in-table caret so an old end caret cannot satisfy the ack.
            before_leave = Atspi.Text.get_text(provider, 0, -1)
            expected_boundary = len(before_leave)
            wait(lambda: Atspi.Text.get_caret_offset(provider) < expected_boundary,
                 "native table undo caret restored inside table")
            key("Return", shift=True)
            wait(lambda: Atspi.Text.get_text(provider, 0, -1) == before_leave
                 and Atspi.Text.get_caret_offset(provider) == expected_boundary
                 and Atspi.Text.get_n_selections(provider) == 0,
                 "native leave-table caret acknowledged at trailing paragraph")
            type_text("After the table.")
            key("s")

            def table_saved():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    body = json.loads(db.execute("SELECT body FROM notes WHERE title = 'Z' AND deleted_at IS NULL").fetchone()[0])
                tables = [b for b in body["blocks"] if b["type"] == "table"]
                return len(tables) == 1 and tables[0]["rows"] == 3 and tables[0]["columns"] == 3 and all(any(s.get("text") == word for c in tables[0]["cells"] for b in c["blocks"] for s in b.get("spans", [])) for word in ("One", "Two", "Three", "Four")) and body["blocks"][-1]["spans"][0]["text"] == "After the table."

            wait(table_saved, "native table edits and undo saved")

            # Find uses an independently named text input and preserves note data.
            key("f")
            within = wait(lambda: find("Find in note"), "find input")
            query_in_note = within.get_text_iface()
            type_text("o")
            wait(lambda: find("1 of 3"), "case-insensitive match count")
            key("F3", ctrl=False)
            wait(lambda: find("2 of 3"), "next match")
            key("F3", ctrl=False, shift=True)
            wait(lambda: find("1 of 3"), "previous match")
            guard()
            assert Atspi.Text.set_selection(query_in_note, 0, 0, 1)
            guard()
            type_text("three")
            wait(lambda: find("1 of 1"), "table-cell find")
            key("Escape", ctrl=False)
            for name, shifted in (("b", False), ("x", True), ("y", True), ("e", False), ("h", True)):
                key(name, shift=shifted)

            def formatted_block():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    body = json.loads(db.execute("SELECT body FROM notes WHERE title = 'Z' AND deleted_at IS NULL").fetchone()[0])
                return next(b for t in body["blocks"] if t["type"] == "table" for c in t["cells"] for b in c["blocks"] if "".join(s.get("text", "") for s in b.get("spans", [])) == "Three")

            def heading(level):
                block = formatted_block()
                return block["type"] == "heading" and block["level"] == level

            wait(lambda: heading(2), "heading shortcut saved")
            key("m", shift=True)
            click("Subheading")
            wait(lambda: heading(3), "subheading action saved before undo")
            key("z")
            wait(lambda: heading(2), "undo subheading saved")
            key("7", shift=True)
            wait(lambda: formatted_block().get("list") == "ordered", "numbered-list shortcut saved")
            key("z")
            wait(lambda: heading(2), "undo numbered list saved")
            key("8", shift=True)
            wait(lambda: formatted_block().get("list") == "bullet", "bulleted-list shortcut saved")
            key("z")
            wait(lambda: heading(2), "undo bulleted list saved")
            key("s")

            def formatted_saved():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    body = json.loads(db.execute("SELECT body FROM notes WHERE title = 'Z' AND deleted_at IS NULL").fetchone()[0])
                blocks = [b for t in body["blocks"] if t["type"] == "table" for c in t["cells"] for b in c["blocks"]]
                block = next(b for b in blocks if "".join(s.get("text", "") for s in b.get("spans", [])) == "Three")
                return block["type"] == "heading" and block["level"] == 2 and set(block["spans"][0].get("marks", [])) == {"bold", "strikethrough", "highlight", "code"}

            try:
                wait(formatted_saved, "formatting shortcuts/menu/undo after find survive save")
            except AssertionError:
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    print("Saved test note:", db.execute("SELECT body FROM notes WHERE title = 'Z' AND deleted_at IS NULL").fetchone()[0], flush=True)
                raise
            key("space", shift=True)
            wait(lambda: find("Selection formatting"), "selection popover in native accessibility tree")
            click("Highlight selection")
            wait(lambda: "highlight" not in formatted_block()["spans"][0].get("marks", []), "selection popover applies one mark")
            key("z")
            wait(formatted_saved, "undo selection popover mark")
            key("Escape", ctrl=False)
            wait(lambda: not find("Selection formatting"), "Escape dismisses selection popover")

            # Links keep the table text/marks intact, expose named controls,
            # and navigate only within our disposable library.
            key("k", shift=True)
            destination = wait(lambda: find("Link destination"), "link address input")
            def destination_focused():
                destination.clear_cache()
                return destination.get_state_set().contains(Atspi.StateType.FOCUSED)
            wait(destination_focused, "link input focus")
            assert "Text" in destination.get_interfaces()
            type_text("https://example.com/research")
            key("Return", ctrl=False)
            wait(lambda: formatted_block()["spans"][0].get("href") == "https://example.com/research", "web link persisted without replacing text")
            key("k", shift=True)
            destination = wait(lambda: find("Link destination"), "edit link address")
            wait(destination_focused, "edit link focus")
            key("a")
            type_text("note://" + original["id"])
            key("Return", ctrl=False)
            wait(lambda: formatted_block()["spans"][0].get("href") == "note://" + original["id"], "note link persisted")
            key("r", shift=True)
            wait(lambda: find("Related notes, 1. Control Shift R"), "explicit related-note summary")
            key("Return", ctrl=False)
            wait(lambda: find("Return to previous linked note"), "return link action")
            wait(lambda: "Accessible notes" in Atspi.Text.get_text(find("Note content").get_text_iface(), 0, -1), "linked note opened")
            click("Return to previous linked note")
            wait(lambda: Atspi.Text.get_text(find("Note content").get_text_iface(), 0, -1).startswith("Z"), "return to original note")
            click("Remove link, keep text")
            wait(lambda: "href" not in formatted_block()["spans"][0], "remove link persisted")
            assert formatted_saved(), "link edits preserve heading and marks"
            key("q", shift=True)
            key("s")
            click("Enable quotes with a backup")
            wait(lambda: find("Enable quotes with a verified backup"), "explicit format-upgrade panel")
            key("Escape", ctrl=False)
            wait(lambda: not find("Enable quotes with a verified backup"), "cancel before opting in")
            with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                assert db.execute("PRAGMA user_version").fetchone()[0] == 2
            click("Enable quotes with a backup")
            click("Back up and enable quotes")
            def quote_saved():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    version = db.execute("PRAGMA user_version").fetchone()[0]
                return version == 3 and formatted_block()["type"] == "quote"
            wait(quote_saved, "native quote upgrade and durable save")
            with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                backup = Path(db.execute("SELECT backup_path FROM schema_migrations WHERE version=3").fetchone()[0])
            assert backup.is_relative_to(root) and json.loads((backup / "manifest.json").read_text())["schemaVersion"] == 2
            click("Return to writing")
            key("z")
            wait(formatted_saved, "undo quote restores heading and marks in upgraded library")
            # An external capture updates the library without touching the open
            # editor or taking keyboard focus. No graphical session for the CLI.
            def saved_editor_text():
                with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                    body = json.loads(db.execute("SELECT body FROM notes WHERE title='Z' AND deleted_at IS NULL").fetchone()[0])
                def leaves(blocks):
                    for block in blocks:
                        if block["type"] == "table":
                            for cell in block["cells"]:
                                yield from leaves(cell["blocks"])
                        else:
                            yield "".join(span.get("text", "\ufffc") for span in block.get("spans", []))
                # Search plain_text omits empty cells; compare the actual editor
                # representation so those cells remain part of this assertion.
                return "\n".join(leaves(body["blocks"]))
            type_text(" Native draft")
            provider_before_capture = find("Note content").get_text_iface()
            before_capture = Atspi.Text.get_text(provider_before_capture, 0, -1)
            caret_before_capture = Atspi.Text.get_caret_offset(provider_before_capture)
            assert editor_focused()
            assert saved_editor_text() != before_capture, "begin capture while the editor has a newer draft"
            capture_id = str(uuid.uuid4())
            capture_env = dict(os.environ)
            for name in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS"):
                capture_env.pop(name, None)
            command = [str(BINARY), "--data-dir", str(library), "--capture", "A thought from the terminal", "--request-id", capture_id]
            guard()
            first = subprocess.run(command, env=capture_env, check=True, capture_output=True, text=True, timeout=10)
            guard()
            wait(lambda: find("A thought from the terminal"), "forwarded capture in note list")
            assert Atspi.Text.get_text(find("Note content").get_text_iface(), 0, -1) == before_capture
            assert editor_focused()
            assert Atspi.Text.get_caret_offset(find("Note content").get_text_iface()) == caret_before_capture
            guard()
            second = subprocess.run(command, env=capture_env, check=True, capture_output=True, text=True, timeout=10)
            guard()
            assert first.stdout == second.stdout
            with sqlite3.connect(f"file:{library}/library.sqlite3?mode=ro", uri=True) as db:
                assert db.execute("SELECT count(*) FROM notes WHERE title='A thought from the terminal'").fetchone()[0] == 1
            key("s")
            wait(lambda: saved_editor_text() == before_capture, "original draft saves after external capture")
            key("k")
            search = wait(lambda: find("Search notes"), "accessible search")
            def search_focused():
                search.clear_cache()
                return search.get_state_set().contains(Atspi.StateType.FOCUSED)
            wait(search_focused, "library search focused before typing")
            assert "Text" in search.get_interfaces()
            query = search.get_text_iface()
            assert Atspi.Text.get_text(query, 0, -1) == "", "placeholder must not become search content"
            for char in "Accessible notes":
                guard()
                subprocess.run(["wtype", "--", char], check=True, timeout=3)
                guard()
            try:
                wait(lambda: Atspi.Text.get_text(query, 0, -1) == "Accessible notes", "search text")
            except AssertionError:
                print("Actual search query:", repr(Atspi.Text.get_text(query, 0, -1)), "focused:", search_focused(), flush=True)
                raise
            assert find("Accessible notes"), "search result must remain reachable"
            guard()
            assert Atspi.Text.set_selection(query, 0, 0, len("Accessible notes"))
            guard()
            wait(lambda: Atspi.Text.get_n_selections(query) == 1, "accessible search selection")
            guard()
            subprocess.run(["wtype", "--", "Z"], check=True, timeout=3)
            guard()
            wait(lambda: Atspi.Text.get_text(query, 0, -1) == "Z", "replacement search query")
            wait(lambda: find("Z"), "filtered new-note result")
            print("PASS: real AT-SPI registration, Unicode/offscreen/empty text, formatting, geometry, selection, guarded edit/undo, named media, checkbox actions, durable save, table creation/Tab/structural actions/undo, find-in-note, complete formatting commands, selection popover, editable links, explicit related-note navigation/return, quote-upgrade consent/cancel/backup/save/undo, forwarded capture without focus/content changes or duplication and accessible search. Orca and IME remain unverified.")
    finally:
        stop(process)
        stop(registry)
        if instance_socket is not None:
            instance_socket.cleanup()


if __name__ == "__main__":
    if not __debug__:
        raise RuntimeError("Native ownership guards require Python assertions; do not use -O or PYTHONOPTIMIZE")
    if sys.argv[1:] == ["--private-bus"]:
        run()
    else:
        env = dict(os.environ, GSETTINGS_BACKEND="memory")
        env.pop("AT_SPI_BUS_ADDRESS", None)
        with tempfile.TemporaryDirectory(prefix="notes-accessibility-config-") as config:
            env["XDG_CONFIG_HOME"] = config
            subprocess.run(["dbus-run-session", "--", "/usr/bin/python", str(Path(__file__).resolve()), "--private-bus"], env=env, check=True)
