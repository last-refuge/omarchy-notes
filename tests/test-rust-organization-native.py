#!/usr/bin/env python3
"""Owned-window folder organization; private bus, scratch library/config only."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import time
from rust_native_test import InstanceSocket

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "target/release/omarchy-notes-preview"
APP_ID = "org.omarchy.Notes.Preview"


def stop(p):
    if p is not None and p.poll() is None:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait(timeout=5)


def run():
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, Gio, GLib

    def wait(predicate, label, timeout=12):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            context = GLib.MainContext.default()
            while context.pending():
                context.iteration(False)
            result = predicate()
            if result:
                return result
            time.sleep(.04)
        raise AssertionError("Timed out: " + label)

    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, 0, 3000, None)
    registry = subprocess.Popen(["/usr/lib/at-spi2-registryd"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    process = None
    endpoint = None
    try:
        with tempfile.TemporaryDirectory(prefix="notes-organization-native-") as tmp:
            root = Path(tmp)
            os.environ["XDG_CONFIG_HOME"] = str(root / "config")
            library = root / "library"
            log = REPO / "target/organization-native-app.log"
            subprocess.run([str(BINARY), "--data-dir", str(library), "--capture", "Keep this thought nearby"], check=True, capture_output=True, timeout=15)

            def rows(query, args=()):
                with sqlite3.connect(library / "library.sqlite3") as db:
                    return db.execute(query, args).fetchall()

            note_id = rows("SELECT id FROM notes")[0][0]

            def compositor(kind):
                return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))

            def window():
                assert process.poll() is None, log.read_text()[-3000:]
                return next((w for w in compositor("clients") if w.get("pid") == process.pid and w.get("class") == APP_ID), None)

            def guard():
                assert process.poll() is None, log.read_text()[-3000:]
                active, workspace = compositor("activewindow"), compositor("activeworkspace")
                assert active.get("pid") == process.pid and active.get("class") == APP_ID and active.get("address") == address, "Refusing input: owned window lost focus"
                assert active["workspace"]["id"] == workspace["id"], "Refusing input: workspace changed"

            def descendants(node):
                if node is None:
                    return
                node.clear_cache()
                yield node
                for i in range(node.get_child_count()):
                    yield from descendants(node.get_child_at_index(i))

            def frame():
                desktop = Atspi.get_desktop(0)
                app = next((a for i in range(desktop.get_child_count()) if (a := desktop.get_child_at_index(i)).get_process_id() == process.pid), None)
                if app is None:
                    return None
                app.clear_cache()
                title = window()["title"]
                for i in reversed(range(app.get_child_count())):
                    child = app.get_child_at_index(i)
                    child.clear_cache()
                    if child.get_name() == title:
                        return child
                return None

            def named(name, focused=False):
                return next((n for n in descendants(frame()) if n.get_name() == name and (not focused or n.get_state_set().contains(Atspi.StateType.FOCUSED))), None)

            def key(name, ctrl=False, shift=False, closing=False):
                assert name in ("n", "m", "q", "s", "z", "k", "a", "F2", "Return", "Escape", "Tab", "Down")
                guard()
                mods = [("ctrl", ctrl), ("shift", shift)]
                args = ["wtype", "-s", "30"]
                for mod, enabled in mods:
                    if enabled:
                        args += ["-M", mod]
                args += ["-k", name]
                for mod, enabled in reversed(mods):
                    if enabled:
                        args += ["-m", mod]
                args += ["-s", "30"]
                subprocess.run(args, check=True, timeout=3)
                if not closing:
                    guard()

            def click(name):
                node = wait(lambda: named(name), "action " + name)
                guard()
                assert node.get_action_iface().do_action(0), name
                guard()

            def text(label):
                node = named(label, True)
                return Atspi.Text.get_text(node.get_text_iface(), 0, -1) if node else None

            def type_text(label, value):
                for char in value:
                    node = wait(lambda: named(label, True), "focused " + label)
                    provider = node.get_text_iface()
                    before = Atspi.Text.get_text(provider, 0, -1)
                    caret = Atspi.Text.get_caret_offset(provider)
                    guard()
                    subprocess.run(["wtype", "--", char], check=True, timeout=3)
                    guard()
                    wait(lambda: text(label) == before[:caret] + char + before[caret:], "typed character")

            def replace(label, value):
                node = wait(lambda: named(label, True), "editable " + label)
                provider = node.get_text_iface()
                guard()
                length = len(Atspi.Text.get_text(provider, 0, -1))
                if length:
                    assert Atspi.Text.set_selection(provider, 0, 0, length) or Atspi.Text.add_selection(provider, 0, length)
                guard()
                # Let the adapter commit the selection before replacing it.
                if length:
                    wait(lambda: Atspi.Text.get_n_selections(provider) == 1, "selected field")
                # Acknowledge the first replacement before sending the rest.
                guard()
                subprocess.run(["wtype", "--", value[0]], check=True, timeout=3)
                guard()
                wait(lambda: text(label) == value[0], "replacement text")
                type_text(label, value[1:])

            def all_notes():
                if not named("All notes"):
                    key("k", ctrl=True)
                    click("Show folders")
                click("All notes")

            def create(name, parent=None):
                before = rows("SELECT id FROM folders")
                if not named("New folder (Ctrl Alt N)"):
                    key("k", ctrl=True)
                    click("Show folders")
                # Alt shortcuts are covered by the GPU harness; native input
                # avoids compositor bindings and invokes the owned AT-SPI node.
                click("New folder (Ctrl Alt N)")
                type_text("Folder name", name)
                key("Return")
                wait(lambda: len(rows("SELECT id FROM folders")) == len(before) + 1, "committed folder")
                wait(lambda: named("Organize your notebook") is None, "form closed after commit")
                return rows("SELECT id FROM folders WHERE name=? AND parent_id IS ?", (name, parent))[0][0]

            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "--data-dir", str(library), "--reference", "--note", note_id], stdout=output, stderr=output)
            address = wait(window, "owned window")["address"]
            endpoint = InstanceSocket(library, process)
            wait(lambda: compositor("activewindow").get("address") == address, "owned focus")
            wait(lambda: named("Note content", True), "editor")
            type_text("Note content", "!")
            edited = text("Note content")
            work = create("Work")
            clients = create("Clients", work)
            all_notes()
            personal = create("Personal")
            other = create("Clients", personal)
            assert clients != other
            # Forward to our own library owner; never open another process/window.
            subprocess.run([str(BINARY), "--data-dir", str(library), "--note", note_id], check=True, capture_output=True, timeout=15)
            wait(lambda: named("Note content", True), "return to original note")
            assert text("Note content") == edited
            provider = named("Note content", True).get_text_iface()
            caret = Atspi.Text.get_caret_offset(provider)
            click("Move note to a folder (Ctrl Alt M)")
            type_text("Find destination", "Work / Clients")
            wait(lambda: named("Work / Clients"), "filtered destination")
            key("Return")
            wait(lambda: rows("SELECT folder_id FROM notes WHERE id=?", (note_id,))[0][0] == clients, "durable note move")
            wait(lambda: named("Note content", True), "caret after filing")
            assert Atspi.Text.get_caret_offset(named("Note content", True).get_text_iface()) == caret
            key("z", ctrl=True)
            wait(lambda: text("Note content") == "Keep this thought nearby", "filing preserves undo")
            key("z", ctrl=True, shift=True)
            wait(lambda: text("Note content") == edited, "filing preserves redo")
            key("F2")
            replace("Folder name", "Projects")
            key("Return")
            wait(lambda: rows("SELECT name FROM folders WHERE id=?", (clients,))[0][0] == "Projects", "rename identity")
            # Folder actions are accessible even after the editor regains focus.
            if not named("Move this folder"):
                key("k", ctrl=True)
            click("Move this folder")
            type_text("Find destination", "Personal")
            key("Return")
            wait(lambda: rows("SELECT parent_id FROM folders WHERE id=?", (clients,))[0][0] == personal, "nested folder move")
            wait(lambda: named("Organize your notebook") is None, "folder move acknowledged in UI")
            assert rows("SELECT folder_id,plain_text FROM notes WHERE id=?", (note_id,))[0] == (clients, edited)
            key("F2")
            replace("Folder name", "Clients")
            key("Return")
            wait(lambda: any("already a folder" in n.get_name() for n in descendants(frame())), "recoverable sibling-name error")
            assert rows("SELECT name FROM folders WHERE id=?", (clients,))[0][0] == "Projects"
            replace("Folder name", "Research")
            key("Return")
            wait(lambda: rows("SELECT name FROM folders WHERE id=?", (clients,))[0][0] == "Research", "retry corrected name")
            wait(lambda: named("Organize your notebook") is None, "retry acknowledged in UI")
            key("q", ctrl=True, closing=True)
            process.wait(timeout=12)
            assert process.returncode == 0
            endpoint.cleanup()
            print("PASS: native nested folders, duplicate names in different parents, search/filing, exact caret and undo/redo retention, rename/move identity, sibling-name error and corrected retry, and durable close. Real IME/Orca remain unverified.")
    finally:
        stop(process)
        if endpoint is not None:
            endpoint.cleanup()
        stop(registry)


if __name__ == "__main__":
    if "--isolated" not in sys.argv:
        os.execvp("dbus-run-session", ["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "--isolated"])
    run()
