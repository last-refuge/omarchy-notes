#!/usr/bin/env python3
"""Owned Wayland/AT-SPI settings, typography, undo and persistence checks.

All preferences and notes are disposable. Input is guarded by exact PID,
app ID, window address and workspace. This is not an IME or Orca signoff.
"""
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
BINARY = REPO / "target/release/omarchy-notes-rust"
APP_ID = "org.omarchy.Notes.Rust"


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


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
            value = predicate()
            if value:
                return value
            time.sleep(.04)
        raise AssertionError("Timed out: " + label)

    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, 0, 3000, None)
    registry = subprocess.Popen(["/usr/lib/at-spi2-registryd"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    process = None
    endpoint = None
    try:
        with tempfile.TemporaryDirectory(prefix="notes-settings-native-") as tmp:
            root = Path(tmp)
            os.environ["XDG_CONFIG_HOME"] = str(root / "config")
            library = root / "library"
            path = root / "config/omarchy-notes-gpui/preferences.json"
            log = REPO / "target/settings-native-app.log"
            subprocess.run([str(BINARY), "--data-dir", str(library), "--capture", "A little room for a thought"], check=True, capture_output=True, timeout=15)
            with sqlite3.connect(library / "library.sqlite3") as db:
                note_id = db.execute("SELECT id FROM notes").fetchone()[0]

            def compositor(kind):
                return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))

            def owned_window():
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
                # This machine's AT-SPI cache can retain closed-window nodes.
                # Resolve the current PID and its exact live window title afresh.
                desktop = Atspi.get_desktop(0)
                app = next((a for i in range(desktop.get_child_count()) if (a := desktop.get_child_at_index(i)).get_process_id() == process.pid), None)
                if app is None:
                    return None
                app.clear_cache()
                title = owned_window()["title"]
                for i in reversed(range(app.get_child_count())):
                    child = app.get_child_at_index(i)
                    child.clear_cache()
                    if child.get_name() == title:
                        return child
                return None

            def named(name, focused=False):
                return next((n for n in descendants(frame()) if n.get_name() == name and (not focused or n.get_state_set().contains(Atspi.StateType.FOCUSED))), None)

            def editor():
                return named("Note content", True)

            def text():
                node = editor()
                return Atspi.Text.get_text(node.get_text_iface(), 0, -1) if node else None

            def key(name, ctrl=False, shift=False, closing=False):
                assert name in ("comma", "Tab", "Return", "Escape", "q", "z", "s")
                guard()
                args = ["wtype", "-s", "30"] + (["-M", "ctrl"] if ctrl else []) + (["-M", "shift"] if shift else []) + ["-k", name] + (["-m", "shift"] if shift else []) + (["-m", "ctrl"] if ctrl else []) + ["-s", "30"]
                subprocess.run(args, check=True, timeout=3)
                if not closing:
                    guard()

            def focus_control(name, tabs):
                for _ in range(tabs):
                    key("Tab")
                node = wait(lambda: named(name, True), "focused " + name)
                bounds = node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                # The platform frame has no bounds on this adapter. The real
                # settings group exposes the visible content area, including
                # its fixed header and status footer, in the same coordinates.
                frame_bounds = named("Notebook settings").get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                assert bounds.width > 0 and bounds.height > 0
                assert frame_bounds.height > 0 and frame_bounds.y <= bounds.y and bounds.y + bounds.height <= frame_bounds.y + frame_bounds.height, (name, (bounds.x, bounds.y, bounds.width, bounds.height), (frame_bounds.x, frame_bounds.y, frame_bounds.width, frame_bounds.height))
                return node

            def saved():
                return json.loads(path.read_text())["preferences"] if path.exists() else {}

            def stored():
                with sqlite3.connect(library / "library.sqlite3") as db:
                    return db.execute("SELECT plain_text FROM notes WHERE deleted_at IS NULL").fetchone()[0]

            def start():
                nonlocal process, endpoint
                with log.open("w") as output:
                    process = subprocess.Popen([str(BINARY), "--data-dir", str(library), "--note", note_id], stdout=output, stderr=output)
                window = wait(owned_window, "native settings window")
                endpoint = InstanceSocket(library, process)
                wait(lambda: compositor("activewindow").get("address") == window["address"], "owned initial focus")
                wait(frame, "accessible frame")
                return window["address"]

            def close():
                key("q", ctrl=True, closing=True)
                process.wait(timeout=12)
                assert process.returncode == 0, log.read_text()[-3000:]
                endpoint.cleanup()

            address = start()
            wait(editor, "initial writing focus")
            assert not path.exists(), "Reading defaults must not write settings"
            original = text()
            provider = editor().get_text_iface()
            guard()
            assert Atspi.Text.set_caret_offset(provider, 9)
            guard()
            wait(lambda: Atspi.Text.get_caret_offset(provider) == 9, "caret at nine")
            before = provider.get_character_extents(9, Atspi.CoordType.WINDOW)
            guard()
            subprocess.run(["wtype", "--", "!"], check=True, timeout=3)
            guard()
            edited = original[:9] + "!" + original[9:]
            wait(lambda: text() == edited, "edit before settings")
            key("comma", ctrl=True)
            wait(lambda: named("Close settings", True), "settings initial focus")
            focus_control("Warm paper", 2)
            key("Return")
            wait(lambda: saved().get("appearance") == "light", "saved palette")
            focus_control("Writing font: Sans", 3)
            key("Return")
            wait(lambda: saved().get("note_font") == "sans", "saved font")
            focus_control("Increase Writing size", 3)
            key("Return")
            try:
                wait(lambda: saved().get("note_size") == 20, "first writing size")
            except AssertionError:
                print("Actual preferences:", saved(), "Focused:", [n.get_name() for n in descendants(frame()) if n.get_state_set().contains(Atspi.StateType.FOCUSED)], flush=True)
                raise
            key("Return")
            wait(lambda: saved().get("note_size") == 21, "second writing size")
            focus_control("Increase Sidebar width", 2)
            key("Return")
            wait(lambda: saved().get("sidebar_width") == 249, "sidebar width")
            focus_control("Increase Note list width", 2)
            key("Return")
            wait(lambda: saved().get("list_width") == 353, "list width")
            key("Escape")
            wait(editor, "return to editor")
            assert text() == edited
            provider = editor().get_text_iface()
            assert Atspi.Text.get_caret_offset(provider) == 10
            after = provider.get_character_extents(9, Atspi.CoordType.WINDOW)
            assert after.height > before.height, (before.height, after.height)
            key("z", ctrl=True)
            wait(lambda: text() == original, "settings do not consume note undo")
            key("z", ctrl=True, shift=True)
            wait(lambda: text() == edited, "retained redo")
            key("s", ctrl=True)
            wait(lambda: stored() == edited, "durable note")
            close()
            expected = saved()
            assert expected["note_size"] == 21
            address = start()
            wait(editor, "reopened editor")
            assert text() == edited
            key("comma", ctrl=True)
            wait(lambda: named("Warm paper") and named("Warm paper").get_state_set().contains(Atspi.StateType.SELECTED), "reopened palette")
            assert named("Writing font: Sans").get_state_set().contains(Atspi.StateType.SELECTED)
            focus_control("Increase Writing size", 8)
            wait(lambda: named("Writing size: 21 pixels"), "reopened writing size")
            focus_control("Increase Sidebar width", 2)
            wait(lambda: named("Sidebar width: 249 pixels"), "reopened sidebar width")
            focus_control("Increase Note list width", 2)
            wait(lambda: named("Note list width: 353 pixels"), "reopened list width")
            close()

            good_bytes = path.read_bytes()
            future = b'{"version":999,"preferences":{}}\n'
            path.write_bytes(future)
            address = start()
            wait(editor, "notes remain usable with future preferences")
            key("comma", ctrl=True)
            wait(lambda: any(n.get_name().startswith("Preferences not saved") for n in descendants(frame())), "visible preferences failure")
            focus_control("Graphite", 1)
            key("Return")
            wait(lambda: named("Retry saving preferences"), "failed palette save")
            assert path.read_bytes() == future, "Never overwrite a newer preferences schema"
            # Simulate an external repair. Retry merges the explicitly changed
            # palette while retaining the repaired file's independent settings.
            path.write_bytes(good_bytes)
            focus_control("Retry saving preferences", 13)
            key("Return")
            wait(lambda: saved().get("appearance") == "reference", "retry merges desired palette")
            assert saved()["note_size"] == 21 and saved()["note_font"] == "sans"
            assert saved()["sidebar_width"] == 249 and saved()["list_width"] == 353
            wait(lambda: named("Your choices are remembered on this device."), "saved feedback")
            wait(lambda: named("Close settings", True), "focus restored after retry disappears")
            close()
            print("PASS: native keyboard-visible settings, live font metrics, exact caret and undo/redo retention, durable save/relaunch, future-file preservation and repaired-file retry/merge. Real IME/Orca remain unverified.")
    finally:
        stop(process)
        if endpoint is not None:
            endpoint.cleanup()
        stop(registry)


if __name__ == "__main__":
    if "--isolated" not in sys.argv:
        os.execvp("dbus-run-session", ["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "--isolated"])
    run()
