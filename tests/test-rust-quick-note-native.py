#!/usr/bin/env python3
"""Opt-in Wayland/AT-SPI multi-window check using only an owned scratch app.
No clipboard, desktop settings, screenshots or external viewers. Every input is
guarded before/after by PID, app ID, window address and active workspace.
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
            result = predicate()
            if result:
                return result
            time.sleep(.04)
        raise AssertionError("Timed out: " + label)

    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, 0, 3000, None)
    registry = subprocess.Popen(["/usr/lib/at-spi2-registryd"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    process = None
    socket = None
    try:
        with tempfile.TemporaryDirectory(prefix="notes-quick-native-") as tmp:
            root = Path(tmp)
            os.environ["XDG_CONFIG_HOME"] = str(root / "config")
            library = root / "library"
            log = REPO / "target/quick-native-app.log"
            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "--data-dir", str(library), "--reference", "--quick-note"], stdout=output, stderr=output)

            def compositor(kind):
                return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))

            def windows():
                assert process.poll() is None, log.read_text()[-3000:]
                return [w for w in compositor("clients") if w.get("pid") == process.pid and w.get("class") == APP_ID]

            def window(quick):
                return next((w for w in windows() if w["title"].startswith("Quick Note") == quick), None)

            def guard(address):
                assert process.poll() is None, log.read_text()[-3000:]
                active, workspace = compositor("activewindow"), compositor("activeworkspace")
                assert active.get("pid") == process.pid and active.get("address") == address and active.get("class") == APP_ID, "Refusing input: owned window lost focus"
                assert active["workspace"]["id"] == workspace["id"], "Refusing input: workspace changed"

            def focus(address):
                assert any(w["address"] == address for w in windows()), "Only owned windows may be focused"
                assert next(w for w in windows() if w["address"] == address)["workspace"]["id"] == compositor("activeworkspace")["id"]
                if compositor("activewindow").get("address") != address:
                    # Current Lua dispatcher; use an exact owned address only.
                    # https://wiki.hypr.land/configuring/core/dispatchers/
                    result = subprocess.run(["hyprctl", "dispatch", 'hl.dsp.focus({ window = "address:' + address + '" })'], capture_output=True, text=True, timeout=3)
                    assert result.returncode == 0, result.stdout + result.stderr + log.read_text()[-3000:]
                wait(lambda: compositor("activewindow").get("address") == address, "owned focus")
                guard(address)

            def key(address, name, shift=False, transition=None):
                assert name in ("n", "s", "o", "q", "w", "z", "f", "Escape")
                guard(address)
                args = ["wtype"] + ([] if name == "Escape" else ["-M", "ctrl"]) + (["-M", "shift"] if shift else []) + ["-k", name] + (["-m", "shift"] if shift else []) + ([] if name == "Escape" else ["-m", "ctrl"])
                subprocess.run(args, check=True, timeout=3)
                if transition is None:
                    guard(address)
                else:
                    # A launch/close legitimately changes address. Permit only
                    # the expected window of our same process, never type again
                    # until its exact address has been checked and pinned.
                    target = wait(lambda: window(transition), "expected owned window transition")
                    focus(target["address"])
                    guard(target["address"])
                    return target["address"]

            def descendants(node):
                if node is None:
                    return
                node.clear_cache()
                yield node
                for i in range(node.get_child_count()):
                    yield from descendants(node.get_child_at_index(i))

            def owned_app():
                desktop = Atspi.get_desktop(0)
                return next((a for i in range(desktop.get_child_count()) if (a := desktop.get_child_at_index(i)).get_process_id() == process.pid), None)

            q = wait(lambda: window(True), "Quick-only startup")["address"]
            assert len(windows()) == 1
            socket = InstanceSocket(library, process)
            focus(q)
            app = wait(owned_app, "AT-SPI registration")

            def frame():
                # This installed AT-SPI client cannot consume AccessKit's remove
                # cache event, so closed frames can remain in its app children.
                # Read only the newest frame for our active owned window title.
                active = compositor("activewindow")
                assert active.get("pid") == process.pid and active.get("class") == APP_ID
                app.clear_cache()
                children = [app.get_child_at_index(i) for i in range(app.get_child_count())]
                for child in reversed(children):
                    child.clear_cache()
                    if child.get_name() == active["title"]:
                        return child
                return None

            def editor():
                return next((n for n in descendants(frame()) if n.get_name() == "Note content" and n.get_state_set().contains(Atspi.StateType.FOCUSED)), None)

            def current_text():
                node = editor()
                return Atspi.Text.get_text(node.get_text_iface(), 0, -1) if node else None

            def type_text(address, value):
                for char in value:
                    provider = wait(editor, "focused editor").get_text_iface()
                    before = Atspi.Text.get_text(provider, 0, -1)
                    caret = Atspi.Text.get_caret_offset(provider)
                    guard(address)
                    subprocess.run(["wtype", "--", char], check=True, timeout=3)
                    guard(address)
                    try:
                        wait(lambda: current_text() == before[:caret] + char + before[caret:], "input acknowledgement")
                    except AssertionError:
                        print("Expected/actual:", repr(before[:caret] + char + before[caret:]), repr(current_text()), flush=True)
                        print("Editors:", [(n.get_name(), Atspi.Text.get_text(n.get_text_iface(), 0, -1), n.get_state_set().contains(Atspi.StateType.FOCUSED)) for n in descendants(app) if n.get_name() == "Note content"], flush=True)
                        print("Windows:", [(w["address"], w["title"]) for w in windows()], flush=True)
                        raise

            def stored():
                with sqlite3.connect(library / "library.sqlite3") as db:
                    return db.execute("SELECT id,plain_text FROM notes WHERE deleted_at IS NULL ORDER BY id").fetchall()

            assert stored() == []
            type_text(q, "A thought from Quick Note")
            key(q, "s")
            wait(lambda: len(stored()) == 1 and stored()[0][1] == "A thought from Quick Note", "first durable save")
            note_id = stored()[0][0]
            # Selection and undo survive moving the same entity into the main window.
            provider = wait(editor, "quick editor").get_text_iface()
            guard(q)
            assert Atspi.Text.set_caret_offset(provider, 9)
            guard(q)
            wait(lambda: Atspi.Text.get_caret_offset(provider) == 9, "quick caret")
            main = key(q, "o", shift=True, transition=False)
            wait(lambda: len(windows()) == 1, "quick removed on handoff")
            assert current_text() == "A thought from Quick Note"
            assert Atspi.Text.get_caret_offset(wait(editor, "main editor").get_text_iface()) == 9
            type_text(main, "!")
            key(main, "z")
            wait(lambda: current_text() == "A thought from Quick Note", "undo new main edit")
            key(main, "z")
            wait(lambda: current_text() != "A thought from Quick Note", "undo retained")
            key(main, "z", shift=True)
            wait(lambda: current_text() == "A thought from Quick Note", "redo retained")
            key(main, "f")
            wait(lambda: any(n.get_name() == "Find in note" and n.get_state_set().contains(Atspi.StateType.FOCUSED) for n in descendants(frame())), "rebound find focus")
            key(main, "Escape")
            wait(editor, "editor focus after find")
            q = key(main, "n", shift=True, transition=True)
            wait(lambda: len(windows()) == 2, "separate quick window")
            assert current_text() == ""
            main = key(q, "w", transition=False)
            wait(lambda: len(windows()) == 1, "untouched quick closed")
            assert len(stored()) == 1
            q = key(main, "n", shift=True, transition=True)
            type_text(q, "Keep writing after main closes")
            focus(main)
            q = key(main, "q", transition=True)
            wait(lambda: len(windows()) == 1, "main closed independently")
            type_text(q, " safely")
            key(q, "s")
            wait(lambda: any(text == "Keep writing after main closes safely" for _, text in stored()), "writer survives main close")
            main = key(q, "o", shift=True, transition=False)
            wait(lambda: len(windows()) == 1, "main reopened")
            assert current_text() == "Keep writing after main closes safely"
            assert len(stored()) == 2 and any(i == note_id for i, _ in stored())
            print("PASS: native Quick-only startup, first save, exact-caret/undo handoff, rebound find, untouched discard, independent main close, shared writer survival and main reopen. IME/Orca remain unverified.")
    finally:
        stop(process)
        if socket is not None:
            socket.cleanup()
        stop(registry)


if __name__ == "__main__":
    if "--isolated" not in sys.argv:
        os.execvp("dbus-run-session", ["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "--isolated"])
    run()
