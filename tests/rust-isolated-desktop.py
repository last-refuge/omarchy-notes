#!/usr/bin/env python3
"""Opt-in real IME/Orca QA in an owned nested Hyprland; defaults to preflight.
No host IME replacement, desktop config writes, systemd environment updates,
clipboard operations or input outside the exact owned nested app window.
Speech generation logs are real Orca evidence, not a human usability pass.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import time

REPO = Path(__file__).resolve().parents[1]
NAMED_KEYS = {"Return", "Escape", "Tab", "Home", "End", "Left", "Right", "Up", "Down", "BackSpace", "Delete", "space", "F3"}


def args():
    if not __debug__:
        raise RuntimeError("Native ownership guards require Python assertions; do not use -O or PYTHONOPTIMIZE")
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", action="store_true", help="Actually launch the isolated desktop; otherwise inspect prerequisites only")
    p.add_argument("--binary", type=Path, default=REPO / "target/release/omarchy-notes-preview")
    p.add_argument("--output", type=Path, default=REPO / "target/isolated-native-qa")
    p.add_argument("--mode", choices=("ime", "orca", "both"), default="both")
    p.add_argument("--manual-seconds", type=int, default=0, help="Keep the owned desktop open after scripted checks; no automatic human-pass claim")
    p.add_argument("--routing-proof", action="store_true", help="Prove private input isolation with a second owned host Gtk target; no full scenarios")
    p.add_argument("--private-worker", type=Path, help=argparse.SUPPRESS)
    return p.parse_args()


def checked(command, env=None, timeout=10):
    return subprocess.check_output(command, env=env, text=True, stderr=subprocess.STDOUT, timeout=timeout)


def preflight(a):
    needed = ["Hyprland", "hyprctl", "wtype", "grim", "dbus-run-session", "/usr/lib/at-spi2-registryd"]
    if a.mode in ("ime", "both"):
        needed += ["fcitx5", "fcitx5-remote"]
    if a.mode in ("orca", "both"):
        needed += ["orca"]
    missing = [n for n in needed if not shutil.which(n)]
    if not a.binary.is_file():
        missing.append(str(a.binary))
    packages = subprocess.run(["pacman", "-Q", "hyprland", "aquamarine", "seatd", "orca", "fcitx5", "fcitx5-chinese-addons", "speech-dispatcher"], text=True, capture_output=True).stdout
    if a.mode in ("ime", "both") and not Path("/usr/share/fcitx5/inputmethod/pinyin.conf").is_file():
        missing.append("fcitx5 pinyin composing engine")
    # Do not blindly reuse this backend isolation after a compositor upgrade.
    if "hyprland 0.56.2-" not in packages or "aquamarine 0.15.0-" not in packages or "seatd 0.9.3-" not in packages:
        missing.append("audited Hyprland 0.56.2 / Aquamarine 0.15.0 / libseat 0.9.3 versions")
    if a.mode in ("orca", "both") and "orca 50.2-" not in packages:
        missing.append("audited Orca 50.2 silent transport initializer")
    report = {"status": "unverified" if missing else "observed", "installed_packages": packages.splitlines(), "missing": missing, "launched": False, "binary": str(a.binary.resolve())}
    if a.binary.is_file():
        report["binary_sha256"] = hashlib.sha256(a.binary.read_bytes()).hexdigest()
    print(json.dumps(report, indent=2), flush=True)
    return report


def descendants(node):
    if node is None:
        return
    yield node
    for i in range(node.get_child_count()):
        yield from descendants(node.get_child_at_index(i))


class Desktop:
    def __init__(self, a, root):
        self.a, self.root = a, root
        self.children, self.logs = [], []
        self.env = dict(os.environ)
        self.host_env = dict(self.env, XDG_RUNTIME_DIR=self.env["NOTES_QA_HOST_RUNTIME"], HYPRLAND_INSTANCE_SIGNATURE=self.env["NOTES_QA_HOST_INSTANCE"])
        self.host_env["WAYLAND_DISPLAY"] = self.env["NOTES_QA_HOST_WAYLAND"]
        self.outer_address = None
        self.app_address = None
        self.routing_target = None
        self.report = {"status": "unverified", "scripted_actual_engine_checks": {}, "human_screen_reader_usability": "unverified", "candidate_visual_review": "unverified", "started_pids": [], "binary_sha256": hashlib.sha256(a.binary.read_bytes()).hexdigest(), "source_revision": checked(["git", "rev-parse", "HEAD"]).strip(), "source_has_tracked_changes": subprocess.run(["git", "diff", "--quiet"]).returncode != 0}

    def start(self, name, command, env=None):
        log = (self.a.output / (name + ".log")).open("w")
        self.logs.append(log)
        child = subprocess.Popen(command, env=env or self.env, stdout=log, stderr=log, start_new_session=True)
        self.children.append((name, child))
        self.report["started_pids"].append({"name": name, "pid": child.pid})
        return child

    def wait(self, predicate, description, timeout=15, guarded=False):
        from gi.repository import GLib
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            if guarded:
                self.guard()
            context = GLib.MainContext.default()
            while context.pending():
                context.iteration(False)
            result = predicate()
            if result:
                return result
            time.sleep(.05)
        raise AssertionError("Timed out: " + description)

    def compositor(self, kind, host=False):
        return json.loads(checked(["hyprctl", kind, "-j"], self.host_env if host else self.env, timeout=3))

    def guard(self, address=None):
        assert self.nest.poll() is None and self.app.poll() is None, "Owned process exited"
        self.verify_private_socket()
        active, workspace = self.compositor("activewindow"), self.compositor("activeworkspace")
        expected = address or self.app_address
        assert active.get("pid") == self.app.pid and active.get("address") == expected, "Refusing input: exact nested app window lost focus"
        assert active.get("class") in ("org.omarchy.Notes.Preview", "org.omarchy.Notes.Rust"), "Unexpected owned application identity"
        assert active["workspace"]["id"] == workspace["id"], "Refusing input: nested workspace changed"
        parent = next((c for c in self.compositor("clients", host=True)
                       if c.get("pid") == self.nest.pid and c.get("address") == self.outer_address), None)
        assert parent and parent.get("mapped", False) and not parent.get("hidden", False), "Owned nested parent window disappeared"
        assert parent["workspace"]["id"] == self.outer_workspace, "Owned nested parent workspace changed"
        if self.routing_target:
            outer, host_workspace = self.compositor("activewindow", host=True), self.compositor("activeworkspace", host=True)
            target_process, target_address = self.routing_target
            assert target_process.poll() is None, "Owned host routing target exited"
            assert outer.get("pid") == target_process.pid and outer.get("address") == target_address, "Owned routing target lost host focus"
            assert outer["workspace"]["id"] == host_workspace["id"], "Owned routing target workspace changed"
            return
        if self.a.routing_proof:
            outer, host_workspace = self.compositor("activewindow", host=True), self.compositor("activeworkspace", host=True)
            assert outer.get("pid") == self.nest.pid and outer.get("address") == self.outer_address, "Owned routing setup lost host focus"
            assert outer["workspace"]["id"] == host_workspace["id"], "Owned routing setup workspace changed"

    def verify_private_socket(self):
        private = Path(self.env["WAYLAND_DISPLAY"])
        if not private.is_absolute():
            private = Path(self.env["XDG_RUNTIME_DIR"]) / private
        private = private.resolve(strict=True)
        host = Path(self.env["NOTES_QA_HOST_WAYLAND"]).resolve(strict=True)
        assert private.is_relative_to(Path(self.root) / "r"), "Private socket escaped owned runtime"
        assert private != host and not os.path.samefile(private, host), "Private and host sockets are identical"
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(2)
            client.connect(str(private))
            pid, uid, _ = struct.unpack("3i", client.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        assert pid == self.nest.pid and uid == os.getuid(), "Private peer is not the owned nested process"
        return {"private_socket": str(private), "host_socket": str(host), "private_peer_pid": pid,
                "private_peer_uid": uid, "distinct_socket_inodes": True}

    def prove_routing(self):
        from gi.repository import Atspi
        journal = self.a.output / "host-target-journal.json"
        wrapper = '''import hashlib,json,os,sys,time
from pathlib import Path
import gi
gi.require_version('Gtk','4.0')
from gi.repository import Gtk
path=Path(sys.argv[1]); count=0; marker='OWNED HOST SENTINEL'
app=Gtk.Application(application_id='org.omarchy.Notes.QaHostTarget')
def activate(app):
    window=Gtk.ApplicationWindow(application=app,title='Disposable private input routing check')
    window.set_default_size(480,160)
    entry=Gtk.Entry(); entry.set_text(marker)
    box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=16)
    box.append(Gtk.Label(label='Owned QA target: private input should leave this field unchanged.'))
    box.append(entry); window.set_child(box)
    def report(entry):
        value=entry.get_text()
        path.write_text(json.dumps({'pid':os.getpid(),'changes':count,'sentinel_unchanged':value==marker,'text_sha256':hashlib.sha256(value.encode()).hexdigest(),'monotonic':time.monotonic()}))
    def changed(entry):
        global count
        count+=1; report(entry)
    entry.connect('changed',changed)
    window.present(); entry.grab_focus(); report(entry)
app.connect('activate',activate)
app.run([sys.argv[0]])
'''
        target = self.start("host-routing-target", [sys.executable, "-c", wrapper, str(journal)], self.host_env)
        address = self.wait(lambda: next((c["address"] for c in self.compositor("clients", host=True)
                                        if c.get("pid") == target.pid and c.get("class") == "org.omarchy.Notes.QaHostTarget" and c.get("mapped", False)), None), "owned host routing target mapping")
        own = next(c for c in self.compositor("clients", host=True) if c.get("address") == address)
        assert own["pid"] == target.pid and own["workspace"]["id"] == self.compositor("activeworkspace", host=True)["id"]
        if self.compositor("activewindow", host=True).get("address") != address:
            reply = checked(["hyprctl", "dispatch", f"hl.dsp.focus({{window={json.dumps('address:'+address)}}})"], self.host_env)
            assert reply.strip() == "ok", "Owned host target focus request rejected"
        self.routing_target = target, address
        self.guard()
        initial = self.wait(lambda: json.loads(journal.read_text()) if journal.exists() else None, "owned host entry journal", guarded=True)
        assert initial["pid"] == target.pid and initial["changes"] == 0 and initial["sentinel_unchanged"]
        checked(["fcitx5-remote", "-c"], self.env)
        text = self.focused_editor()
        before = Atspi.Text.get_text(text, 0, -1)
        self.key("End", ctrl=True)
        self.wait(lambda: Atspi.Text.get_caret_offset(text) == len(before), "private end caret for routing proof", guarded=True)
        marker = "PRIVATE_SOCKET_" + str(self.app.pid)
        self.text(marker)
        self.wait(lambda: Atspi.Text.get_text(text, 0, -1) == before + marker, "private fixture input acknowledgement", guarded=True)
        self.screenshot("routing-proof-inner.png")
        for _ in range(10):
            self.guard()
            current = json.loads(journal.read_text())
            assert current["changes"] == 0 and current["sentinel_unchanged"], "Private input reached the owned host target"
            time.sleep(.05)
        self.report["private_input_routing_proof"] = {**self.verify_private_socket(), "host_target_pid": target.pid,
            "host_target_address": address, "host_target_changes": current["changes"], "host_sentinel_unchanged": current["sentinel_unchanged"],
            "private_inserted_utf8_bytes": len(marker.encode()), "nested_exact_body_acknowledged": True,
            "scope": "Private virtual keyboard affected only the owned nested fixture while a separate owned host Entry remained focused; capture uses only private app bounds."}
        self.report["status"] = "observed"

    def key(self, key, ctrl=False, shift=False):
        assert key in NAMED_KEYS or (len(key) == 1 and key.isascii() and key.isprintable())
        command = ["wtype"] + (["-M", "ctrl"] if ctrl else []) + (["-M", "shift"] if shift else []) + ["-k", key] + (["-m", "shift"] if shift else []) + (["-m", "ctrl"] if ctrl else [])
        self.guard()
        subprocess.run(command, env=self.env, check=True, timeout=3)
        self.guard()

    def text(self, value):
        assert all(c.isprintable() for c in value)
        for char in value:
            self.guard()
            subprocess.run(["wtype", "--", char], env=self.env, check=True, timeout=3)
            self.guard()

    def focused_editor(self):
        from gi.repository import Atspi
        def find():
            for node in descendants(self.accessible_app):
                if node.get_name() == "Note content" and "Text" in node.get_interfaces() and node.get_state_set().contains(Atspi.StateType.FOCUSED):
                    return node.get_text_iface()
            return None
        return self.wait(find, "focused owned rich editor", guarded=True)

    def listen_editor_events(self):
        from gi.repository import Atspi, Gio
        session_bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        address = session_bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus", "GetAddress", None, None, 0, 3000, None).unpack()[0]
        flags = Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION
        self.event_bus = Gio.DBusConnection.new_for_address_sync(address, flags, None, None)
        def owned_sender():
            names = self.event_bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "ListNames", None, None, 0, 3000, None).unpack()[0]
            from gi.repository import GLib
            for name in names:
                if not name.startswith(":"):
                    continue
                pid = self.event_bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetConnectionUnixProcessID", GLib.Variant("(s)", (name,)), None, 0, 3000, None).unpack()[0]
                if pid == self.app.pid:
                    return name
            return None
        sender = self.wait(owned_sender, "owned application's private accessibility bus identity")
        events = {"sender": sender, "raw_signals": [], "atspi_events": [], "cache_signals": []}
        self.report["owned_editor_event_delivery"] = events
        def raw(connection, bus_sender, path, interface, member, parameters, _data):
            events["raw_signals"].append({"member": member, "path": path, "signature": parameters.get_type_string(), "body": parameters.print_(False)})
        self.signal_subscription = self.event_bus.signal_subscribe(sender, "org.a11y.atspi.Event.Object", None, None, None, Gio.DBusSignalFlags.NONE, raw, None)
        def cache(connection, bus_sender, path, interface, member, parameters, _data):
            events["cache_signals"].append({"member": member, "signature": parameters.get_type_string(), "path": path})
        self.cache_subscription = self.event_bus.signal_subscribe(sender, "org.a11y.atspi.Cache", None, None, None, Gio.DBusSignalFlags.NONE, cache, None)
        def delivered(event, _data=None):
            if event.source.get_process_id() == self.app.pid:
                events["atspi_events"].append({"type": event.type, "detail1": event.detail1, "detail2": event.detail2, "source": event.source.get_name()})
        self.atspi_listener = Atspi.EventListener.new(delivered, None)
        for event in ("object:text-caret-moved", "object:text-selection-changed"):
            assert self.atspi_listener.register(event)
        return events

    def validate_cache_signals(self):
        signals = self.report["owned_editor_event_delivery"]["cache_signals"]
        self.wait(lambda: any(s["member"] == "AddAccessible" for s in signals)
                  and any(s["member"] == "RemoveAccessible" for s in signals),
                  "actual owned cache insertion and removal signals", guarded=True)
        expected = {"AddAccessible": "(((so)(so)(so)iiassusau))", "RemoveAccessible": "((so))"}
        for signal in signals:
            if signal["member"] in expected:
                assert signal["signature"] == expected[signal["member"]], "Invalid native cache argument boundary: " + str(signal)
        self.report["cache_signal_validation"] = {"status": "observed", "AddAccessible": expected["AddAccessible"], "RemoveAccessible": expected["RemoveAccessible"], "encoding": "GVariant parameter tuple includes one outer argument-list boundary"}

    def all_text(self, text):
        from gi.repository import Atspi
        return Atspi.Text.get_text(text, 0, -1)

    def caret(self, text, offset):
        from gi.repository import Atspi
        self.guard()
        assert Atspi.Text.set_caret_offset(text, offset)
        self.guard()
        self.wait(lambda: Atspi.Text.get_caret_offset(text) == offset, "caret acknowledgement")

    def screenshot(self, name, include_owned_popup=False):
        self.guard()
        window = self.compositor("activewindow")
        monitor = next(m for m in self.compositor("monitors") if m["id"] == window["monitor"])
        x, y = window["at"]
        width, height = window["size"]
        right = min(x + width, monitor["x"] + round(monitor["width"] / monitor["scale"]))
        bottom = min(y + height, monitor["y"] + round(monitor["height"] / monitor["scale"]))
        if include_owned_popup:
            # Only our app, fixture and fcitx popup occupy this private output.
            # Include the popup's area below the app; never sample host pixels.
            bottom = monitor["y"] + round(monitor["height"] / monitor["scale"])
        x, y = max(x, monitor["x"]), max(y, monitor["y"])
        assert right > x and bottom > y, "Owned capture bounds are empty"
        geometry = f"{x},{y} {right-x}x{bottom-y}"
        subprocess.run(["grim", "-g", geometry, str(self.a.output / name)], env=self.env, check=True, timeout=5)
        self.guard()

    def fit_window(self, width, height):
        self.guard()
        window = self.compositor("activewindow")
        monitor = next(m for m in self.compositor("monitors") if m["id"] == window["monitor"])
        max_width = round(monitor["width"] / monitor["scale"])
        max_height = round(monitor["height"] / monitor["scale"])
        width, height = min(width, max_width - 40), min(height, max_height - 40)
        # Our private .conf intentionally uses Hyprland's legacy config parser,
        # whose dispatcher protocol differs from the host's Lua config. Both
        # modes can return an error message with exit0: require explicit success.
        target = "address:" + self.app_address
        commands = [["setfloating", target],
                    ["resizewindowpixel", f"exact {width} {height}," + target],
                    ["movewindowpixel", f"exact {monitor['x']+20} {monitor['y']+20}," + target]]
        def dispatch_geometry(stage, arguments):
            self.guard()
            before = self.compositor("activewindow")
            result = checked(["hyprctl", "dispatch", *arguments], self.env)
            self.guard()
            after = self.compositor("activewindow")
            self.report.setdefault("owned_geometry_transitions", []).append({
                "stage": stage, "arguments": arguments, "reply": result.strip(),
                "before": {k: before[k] for k in ("pid", "address", "at", "size", "floating")},
                "after": {k: after[k] for k in ("pid", "address", "at", "size", "floating")},
            })
            assert result.strip() == "ok", "Owned nested dispatcher rejected request: " + result.strip()
        for stage, arguments in zip(("float", "resize", "move"), commands):
            dispatch_geometry(stage, arguments)
        def settled():
            self.guard()
            current = self.compositor("activewindow")
            if current["at"] != [monitor["x"] + 20, monitor["y"] + 20] or current["size"] != [width, height]:
                return False
            if not hasattr(self, "accessible_app"):
                return True
            from gi.repository import Atspi
            for node in descendants(self.accessible_app):
                if node.get_name() != "Note content" or not node.get_state_set().contains(Atspi.StateType.FOCUSED):
                    continue
                parent = node.get_parent()
                chain = []
                chosen = None
                while parent:
                    bounds = None
                    if "Component" in parent.get_interfaces():
                        rect = Atspi.Component.get_extents(parent.get_component_iface(), Atspi.CoordType.WINDOW)
                        bounds = [rect.x, rect.y, rect.width, rect.height]
                    item = {"name": parent.get_name(), "role": parent.get_role_name(),
                            "path": parent.path, "live_component_extents": bounds}
                    chain.append(item)
                    # GPUI's built-in Window root has no bounds. Main renders a
                    # bounded Window; Quick renders a bounded named Group.
                    if chosen is None and bounds and bounds[2] > 0 and bounds[3] > 0 and (
                            item["name"] == "Quick Note" or item["role"] in ("frame", "window")):
                        chosen = item
                    parent = parent.get_parent()
                self.report["latest_focused_viewport_ancestors"] = chain
                if chosen and abs(chosen["live_component_extents"][2] - width) <= 2 and abs(chosen["live_component_extents"][3] - height) <= 2:
                    self.report.setdefault("rendered_viewport_roots", []).append({"requested_size": [width, height], "chosen": chosen, "ancestor_chain": chain})
                    return True
            return False
        try:
            self.wait(settled, "owned compositor and native accessible viewport settlement")
        except AssertionError:
            self.guard()
            current = self.compositor("activewindow")
            from gi.repository import Atspi
            accessible = getattr(self, "accessible_app", None)
            if accessible is None:
                desktop = Atspi.get_desktop(0)
                accessible = next((node for i in range(desktop.get_child_count())
                                   if (node := desktop.get_child_at_index(i)).get_process_id() == self.app.pid), None)
            frames = []
            focused = []
            if accessible:
                for node in descendants(accessible):
                    if node.get_state_set().contains(Atspi.StateType.FOCUSED):
                        focused.append([node.get_role_name(), node.get_name()])
                    if node.get_role_name() in ("frame", "window") and "Component" in node.get_interfaces():
                        bounds = Atspi.Component.get_extents(node.get_component_iface(), Atspi.CoordType.WINDOW)
                        frames.append([bounds.x, bounds.y, bounds.width, bounds.height])
            self.report.setdefault("failed_owned_geometry", []).append({
                "requested_at": [monitor["x"] + 20, monitor["y"] + 20],
                "requested_size": [width, height],
                "observed_at": current["at"], "observed_size": current["size"],
                "observed_floating": current["floating"],
                "owned_pid": current["pid"], "owned_address": current["address"],
                "nested_monitor": {k: monitor[k] for k in ("id", "x", "y", "width", "height", "scale")},
                "accessible_frame_extents": frames, "focused_owned_nodes": focused,
            })
            raise

    def committed(self):
        def leaves(blocks):
            for block in blocks:
                if block["type"] == "table":
                    for cell in block["cells"]:
                        yield from leaves(cell["blocks"])
                else:
                    yield "".join(span.get("text", "\ufffc") for span in block.get("spans", []))
        with sqlite3.connect(f"file:{self.library / 'library.sqlite3'}?mode=ro", uri=True) as db:
            return {note_id: "\n".join(leaves(json.loads(body)["blocks"]))
                    for note_id, body in db.execute("SELECT id,body FROM notes WHERE deleted_at IS NULL")}

    def cli(self, *arguments, expect_ok=True):
        self.guard()
        result = subprocess.run([str(self.a.binary), "--data-dir", str(self.library), *arguments], env=self.env, text=True, capture_output=True, timeout=35)
        # A deliberate launch can activate another owned nested app window.
        assert self.app.poll() is None
        if expect_ok:
            assert result.returncode == 0, (result.stdout, result.stderr)
        return result

    def open(self):
        import gi
        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi, Gio, GLib
        config = self.root / "hyprland.conf"
        config.write_text("""monitor = ,1100x800@60,0x0,1
xwayland { enabled = false }
animations { enabled = false }
misc {
    disable_hyprland_logo = true
    disable_splash_rendering = true
    force_default_wallpaper = 0
}
debug {
    disable_logs = false
    enable_stdout_logs = true
}
""")
        nest_env = dict(self.env, WAYLAND_DISPLAY=self.env["NOTES_QA_HOST_WAYLAND"],
                        LIBSEAT_BACKEND="notes-qa-no-physical-seat", HYPRLAND_NO_SD_VARS="1",
                        HYPRLAND_NO_SD_NOTIFY="1", HYPRLAND_NO_SD_TARGET="1", HYPRLAND_NO_RT="1")
        nest_env.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
        # The deliberately nonexistent libseat backend returns before calling
        # any backend's open_seat; DRM discovery therefore cannot take the seat.
        self.nest = self.start("nested-hyprland", ["Hyprland", "--config", str(config)], nest_env)
        def identity():
            assert self.nest.poll() is None, "Nested compositor failed; inspect nested-hyprland.log"
            for lock in (Path(self.env["XDG_RUNTIME_DIR"]) / "hypr").glob("*/hyprland.lock"):
                data = lock.read_text().splitlines()
                if len(data) >= 2 and int(data[0]) == self.nest.pid:
                    return lock.parent.name, data[1]
            return None
        signature, socket = self.wait(identity, "owned nested compositor socket")
        self.env["HYPRLAND_INSTANCE_SIGNATURE"] = signature
        self.env["WAYLAND_DISPLAY"] = socket
        log = (self.a.output / "nested-hyprland.log").read_text()
        assert "No backend matched name 'notes-qa-no-physical-seat'" in log, "Seat isolation refusal was not observed"
        assert "Wayland backend" in log, "Nested Wayland backend was not observed"
        assert "Seat opened with backend" not in log, "Physical seat unexpectedly opened"
        self.outer_address = self.wait(lambda: next((c["address"] for c in self.compositor("clients", host=True) if c.get("pid") == self.nest.pid and c.get("mapped", False) and not c.get("hidden", False)), None), "mapped owned host nested window")
        own_outer = next(c for c in self.compositor("clients", host=True) if c.get("address") == self.outer_address)
        assert own_outer["pid"] == self.nest.pid
        self.outer_workspace = own_outer["workspace"]["id"]
        self.report["private_guard_contract"] = {**self.verify_private_socket(),
            "inner_scope": "exact live Notes PID/address/app ID/current private workspace before and after every input/capture/mutation",
            "parent_scope": "exact live owned nested parent PID/address/mapped/original workspace; host focus is independent",
            "routing_proof_reference": "docs/rust-native-acceptance.md"}
        if self.a.routing_proof:
            assert self.outer_workspace == self.compositor("activeworkspace", host=True)["id"], "Owned routing setup is not on the current host workspace"
        if self.a.routing_proof and self.compositor("activewindow", host=True).get("address") != self.outer_address:
            try:
                target = json.dumps("address:" + self.outer_address)
                reply = checked(["hyprctl", "dispatch", f"hl.dsp.focus({{window={target}}})"], self.host_env)
                assert reply.strip() == "ok", "Owned host Lua dispatcher rejected request: " + reply.strip()
            except subprocess.CalledProcessError as error:
                raise AssertionError("Owned startup focus request failed: " + error.output) from error
        bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        bus.call_sync("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, 0, 3000, None)
        self.start("registry", ["/usr/lib/at-spi2-registryd"])
        self.library = self.root / "library"
        shutil.copytree(REPO / "tests/fixtures/qt-v1", self.library)
        with sqlite3.connect(self.library / "library.sqlite3") as db:
            self.note_id = db.execute("SELECT id FROM notes ORDER BY id LIMIT 1").fetchone()[0]
            original_documents = [json.loads(row[0]) for row in db.execute("SELECT body FROM notes")]
            blocks = [{"id": "qa-heading", "type": "heading", "level": 1, "spans": [{"text": "Native acceptance"}]}]
            for i, text in enumerate(["A calm sentence for speech checks.", "é 👩‍👩‍👧‍👦 日本語 العربية and English."] + [f"Synthetic paragraph {i}" for i in range(40)]):
                blocks.append({"id": f"qa-{i}", "type": "paragraph", "spans": [{"text": text}]})
            blocks.append({"id": "qa-check", "type": "list", "list": "check", "checked": False, "spans": [{"text": "Review the native behavior"}]})
            blocks.append({"id": "qa-link", "type": "paragraph", "spans": [{"text": "Example link", "href": "https://example.invalid/qa"}]})
            merged = json.loads((REPO / "tests/fixtures/qt-merged-table.json").read_text())
            blocks.extend(b for b in merged["blocks"] if b["type"] == "table")
            blocks.extend(b for d in original_documents for b in d["blocks"] if any("image" in s or "attachment" in s for s in b.get("spans", [])))
            def plain_block(block):
                if block["type"] == "table":
                    return "\n".join(plain_block(b) for cell in block["cells"] for b in cell["blocks"])
                return "".join(s.get("text", "\ufffc") for s in block.get("spans", []))
            plain = "\n".join(plain_block(b) for b in blocks)
            db.execute("UPDATE notes SET deleted_at=1 WHERE id!=?", (self.note_id,))
            db.execute("UPDATE notes SET body=?,plain_text=?,title=? WHERE id=?", (json.dumps({"schema": 1, "blocks": blocks}), plain, "Native acceptance", self.note_id))
        if self.a.mode in ("ime", "both"):
            profile = self.root / "config/fcitx5/profile"
            profile.parent.mkdir(parents=True)
            profile.write_text("""[Groups/0]
Name=Notes QA
Default Layout=us
DefaultIM=pinyin
[Groups/0/Items/0]
Name=keyboard-us
[Groups/0/Items/1]
Name=pinyin
[GroupOrder]
0=Notes QA
""")
            fcitx_env = dict(self.env, WAYLAND_DEBUG="1")
            self.start("fcitx", ["fcitx5", "-D", "--disable=clipboard,notifications,notificationitem,xim,xcb,ibusfrontend,fcitx4frontend,cloudpinyin", "--verbose=default=4,key_trace=5"], fcitx_env)
            self.wait(lambda: subprocess.run(["fcitx5-remote", "--check"], env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0, "private fcitx instance")
        if self.a.mode in ("orca", "both"):
            # Orca 50 uses GSettings and removed the older --enable and
            # --user-prefs-dir switches. Retain speech-generation listeners but
            # suppress its audio transport. Speech=false unregisters caret and
            # selection listeners, defeating this silent diagnostic. No server
            # is started/contacted; the user's no-dictation request persists.
            wrapper = """import runpy, sys
from gi.repository import Gio, GLib
settings = Gio.Settings.new_with_path('org.gnome.Orca.Speech', '/org/gnome/orca/default/speech/')
assert settings.set_boolean('enable', True)
sound = Gio.Settings.new_with_path('org.gnome.Orca.Sound', '/org/gnome/orca/default/sound/')
assert sound.set_boolean('enabled', False)
from orca import debug, speech, speech_manager
# Some presentation initialization still requests a server with speech disabled.
# Replace only this process's transport initializer, never event processing.
speech_manager.SpeechManager._init_server = lambda self: speech.set_server(None)
def flush_debug_log():
    if debug.debugFile:
        debug.debugFile.flush()
    return True
GLib.timeout_add(100, flush_debug_log)
print('QA: actual speech generation logged only; no audio transport', flush=True)
sys.argv = ['orca', '--debug-file=' + sys.argv[1]]
runpy.run_path('/usr/bin/orca', run_name='__main__')
"""
            self.orca = self.start("orca", [sys.executable, "-c", wrapper, str(self.a.output / "orca-debug.log")])
        self.app = self.start("notes", [str(self.a.binary), "--data-dir", str(self.library), "--reference", "--note", self.note_id])
        self.app_address = self.wait(lambda: next((c["address"] for c in self.compositor("clients") if c.get("pid") == self.app.pid and c.get("mapped", False) and not c.get("hidden", False)), None), "mapped owned nested Notes window")
        self.guard()
        self.fit_window(1050, 740)
        def registered():
            desktop = Atspi.get_desktop(0)
            return next((node for i in range(desktop.get_child_count()) if (node := desktop.get_child_at_index(i)).get_process_id() == self.app.pid), None)
        self.accessible_app = self.wait(registered, "owned AT-SPI registration")
        self.focused_editor()
        self.listen_editor_events()
        self.report["isolation"] = {"nested_compositor_pid": self.nest.pid, "nested_instance": signature, "nested_socket": self.env["WAYLAND_DISPLAY"], "host_outer_window": self.outer_address, "note_pid": self.app.pid, "physical_seat_backend": "refused before open_seat", "host_ime_replaced": False, "host_settings_written": False}

    def ime_one(self, label):
        from gi.repository import Atspi
        text = self.focused_editor()
        before = self.all_text(text)
        before_disk = self.committed()
        self.caret(text, 0)
        checked(["fcitx5-remote", "-s", "pinyin"], self.env)
        checked(["fcitx5-remote", "-o"], self.env)
        assert checked(["fcitx5-remote", "-n"], self.env).strip() == "pinyin"
        for char in "nihao":
            self.key(char)
        self.wait(lambda: self.all_text(text) != before, "real pinyin preedit")
        self.wait(lambda: "preedit_string" in (self.a.output / "fcitx.log").read_text(), "Wayland preedit protocol")
        time.sleep(1)
        assert self.committed() == before_disk, "Preedit leaked into autosave"
        self.screenshot(f"ime-{label}-composition.png", include_owned_popup=True)
        rectangles = re.findall(r"text_input_rectangle\((-?\d+),\s*(-?\d+),\s*(\d+),\s*(\d+)\)", (self.a.output / "fcitx.log").read_text())
        assert rectangles and int(rectangles[-1][2]) > 0 and int(rectangles[-1][3]) > 0, "No actual candidate-placement rectangle"
        self.key("Escape")
        self.wait(lambda: self.all_text(text) == before, "real composition cancellation")
        for char in "nihao":
            self.key(char)
        self.key("space")
        self.wait(lambda: self.all_text(text).startswith("你好"), "real pinyin commit")
        committed = self.all_text(text)
        self.key("z", ctrl=True)
        self.wait(lambda: self.all_text(text) == before, "one-step IME undo")
        self.key("z", ctrl=True, shift=True)
        self.wait(lambda: self.all_text(text) == committed, "IME redo")
        self.key("s", ctrl=True)
        self.wait(lambda: committed in self.committed().values(), "durable IME commit")
        # Exercise replacement and Enter/Escape while the engine composes.
        self.guard()
        assert Atspi.Text.set_selection(text, 0, 0, 2)
        self.guard()
        for char in "zhongwen":
            self.key(char)
        self.key("Return")
        self.wait(lambda: self.all_text(text) != committed, "Enter handled by composing engine")
        assert self.all_text(text).count("\n") == committed.count("\n"), "Composing Enter inserted a document break"
        replacement = self.all_text(text)
        self.key("s", ctrl=True)
        self.wait(lambda: replacement in self.committed().values(), "Enter committed composition rather than leaving protected preedit")
        self.key("z", ctrl=True)
        self.wait(lambda: self.all_text(text) == committed, "selection replacement undo")
        self.caret(text, len(committed) - 1)
        for char in "nihao":
            self.key(char)
        self.screenshot(f"ime-{label}-scrolled.png", include_owned_popup=True)
        self.fit_window(850, 650)
        self.screenshot(f"ime-{label}-resized.png", include_owned_popup=True)
        self.key("Escape")
        self.wait(lambda: self.all_text(text) == committed, "composition survives resize then cancels")
        checked(["fcitx5-remote", "-c"], self.env)
        # The final selection-replacement undo is still dirty after cancelling
        # preedit. A SQLite identity query must await this exact final body.
        self.key("s", ctrl=True)
        self.wait(lambda: committed in self.committed().values(), "final undo/cancellation body durably acknowledged", guarded=True)
        self.report["scripted_actual_engine_checks"]["ime-" + label] = {"status": "observed", "engine": "fcitx5 pinyin", "commit": "你好", "cancel_undo_redo_selection_enter_scroll_resize": True, "final_body_disk_acknowledged": True, "caret_end_scope": "actual document scroll in long main fixture; end-caret only in two-character Quick fixture", "candidate_rectangle": list(map(int, rectangles[-1])), "candidate_visual_review": "unverified; inspect owned-output screenshots; resized raster needs separate settlement review"}

    def orca_checks(self):
        from gi.repository import Atspi
        text = self.focused_editor()
        self.caret(text, 0)
        log = self.a.output / "orca-debug.log"
        # The silence wrapper imports presentation code before debug starts;
        # some registration messages precede the log file. Wait until actual
        # Orca has queried this focused editor's caret instead of mistaking a
        # missing startup log line for an absent registered listener.
        self.wait(lambda: log.exists() and any("AXText:" in line and "Note content" in line and "reports caret offset" in line for line in log.read_text().splitlines()), "actual silent Orca focused-editor caret query", guarded=True)
        events = self.report["owned_editor_event_delivery"]
        before_caret = sum(e["member"] == "TextCaretMoved" for e in events["raw_signals"])
        before_selection = sum(e["member"] == "TextSelectionChanged" for e in events["raw_signals"])
        self.key("Right")
        self.focused_editor()
        self.wait(lambda: Atspi.Text.get_caret_offset(text) > 0, "Orca-test keyboard caret motion")
        self.wait(lambda: sum(e["member"] == "TextCaretMoved" for e in events["raw_signals"]) > before_caret, "actual private-bus TextCaretMoved delivery after keyboard caret motion", guarded=True)
        self.wait(lambda: any(e["type"] == "object:text-caret-moved" for e in events["atspi_events"]), "libatspi caret event parsing", guarded=True)
        start = Atspi.Text.get_caret_offset(text)
        self.key("Right", shift=True)
        self.focused_editor()
        self.wait(lambda: Atspi.Text.get_n_selections(text) == 1, "Orca-test keyboard selection")
        selection = Atspi.Text.get_selection(text, 0)
        assert selection.start_offset != selection.end_offset
        assert start in (selection.start_offset, selection.end_offset)
        self.wait(lambda: sum(e["member"] == "TextSelectionChanged" for e in events["raw_signals"]) > before_selection, "actual private-bus TextSelectionChanged delivery after keyboard selection", guarded=True)
        self.wait(lambda: any(e["type"] == "object:text-selection-changed" for e in events["atspi_events"]), "libatspi selection event parsing", guarded=True)
        self.key("Left")
        self.focused_editor()
        self.wait(lambda: Atspi.Text.get_n_selections(text) == 0, "Orca-test selection collapse")
        content = log.read_text()
        assert self.orca.poll() is None, "Actual Orca engine exited"
        assert "QA: actual speech generation logged only; no audio transport" in (self.a.output / "orca.log").read_text(), "Silent Orca transport guard not observed"
        # Actual installed Orca runs and listens; audio/speech output is disabled.
        # This is neither manufactured speech nor an audible/human task pass.
        generated = [line for line in content.splitlines() if "SPEECH OUTPUT:" in line]
        self.report["scripted_actual_engine_checks"]["orca"] = {"status": "observed", "mode": "silent actual speech generation with audio transport disabled", "speech_utterances": len(generated), "owned_focus_and_caret_actions": True, "selection_offsets": [selection.start_offset, selection.end_offset], "selection_speech": "unverified; inspect generated log", "audible_output": "unverified; no audio playback authorized", "human_core_tasks": "unverified"}
        (self.a.output / "orca-speech.txt").write_text("\n".join(generated) + "\n")
        self.guard()
        assert Atspi.Text.get_caret_offset(text) >= 0
        self.guard()

    def run(self):
        self.open()
        if self.a.routing_proof:
            self.prove_routing()
            return
        if os.environ.get("NOTES_NATIVE_QA_FOCUS") == "geometry":
            self.report["status"] = "observed"
            self.report["focused_geometry_probe"] = "Owned startup geometry settled; full IME/Orca scenarios not run"
            return
        if os.environ.get("NOTES_NATIVE_QA_FOCUS") == "quick-geometry":
            self.cli("--quick-note")
            self.app_address = self.wait(lambda: next((c["address"] for c in self.compositor("clients") if c.get("pid") == self.app.pid and "Quick" in c.get("title", "")), None), "owned Quick geometry window")
            self.guard()
            self.fit_window(680, 660)
            self.report["status"] = "observed"
            self.report["focused_geometry_probe"] = "Exact owned Quick compositor and rendered-root viewport settled; full scenarios not run"
            return
        if self.a.mode in ("ime", "both"):
            self.ime_one("main")
            self.cli("--quick-note")
            self.app_address = self.wait(lambda: next((c["address"] for c in self.compositor("clients") if c.get("pid") == self.app.pid and "Quick" in c.get("title", "")), None), "owned Quick Note window")
            self.guard()
            self.fit_window(680, 660)
            self.ime_one("quick")
            quick_ids = [i for i, text in self.committed().items() if text == "你好"]
            assert len(quick_ids) == 1, "Quick committed identity ambiguous"
            # Handoff of a composing shared editor must fail without moving it.
            checked(["fcitx5-remote", "-o"], self.env)
            for char in "nihao":
                self.key(char)
            result = self.cli("--note", quick_ids[0], expect_ok=False)
            assert result.returncode != 0, "Composing handoff incorrectly reported success"
            self.guard()
            self.key("Escape")
            checked(["fcitx5-remote", "-c"], self.env)
            result = self.cli("--note", quick_ids[0])
            self.app_address = self.wait(lambda: next((c["address"] for c in self.compositor("clients") if c.get("pid") == self.app.pid and "Quick" not in c.get("title", "")), None), "main after settled Quick handoff")
            self.guard()
            assert self.all_text(self.focused_editor()) == "你好", "Quick handoff changed the committed content"
            self.report["scripted_actual_engine_checks"]["handoff"] = {"status": "observed", "composition_guard_and_committed_content": True}
            self.cli("--note", self.note_id)
            self.guard()
        if self.a.mode in ("orca", "both"):
            self.orca_checks()
        self.validate_cache_signals()
        if self.a.manual_seconds:
            print("Owned desktop ready for manual acceptance; automatic input stopped.", flush=True)
            until = time.monotonic() + self.a.manual_seconds
            while time.monotonic() < until and self.nest.poll() is None and self.app.poll() is None:
                time.sleep(.5)
        self.report["status"] = "observed"

    def cleanup(self):
        if hasattr(self, "signal_subscription"):
            self.event_bus.signal_unsubscribe(self.signal_subscription)
        if hasattr(self, "cache_subscription"):
            self.event_bus.signal_unsubscribe(self.cache_subscription)
        if hasattr(self, "atspi_listener"):
            for event in ("object:text-caret-moved", "object:text-selection-changed"):
                self.atspi_listener.deregister(event)
        for log in (Path(self.env["XDG_RUNTIME_DIR"]) / "hypr").glob("*/hyprland.log"):
            shutil.copyfile(log, self.a.output / "nested-hyprland-runtime.log")
        results = []
        for name, child in reversed(self.children):
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=5)
            # Each group was created by this runner. Catch a service's owned
            # descendants even when its immediate launcher already exited.
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            results.append({"name": name, "pid": child.pid, "returncode": child.returncode})
        for log in self.logs:
            log.close()
        # The installed libatspi consumer writes this warning during parsing;
        # validate it after its driver log has been flushed too.
        self.report["invalid_cache_client_warnings"] = "pending outer-driver log validation"
        self.report["cleanup"] = results
        (self.a.output / "report.json").write_text(json.dumps(self.report, indent=2) + "\n")


def main():
    a = args()
    a.binary = a.binary.resolve()
    a.output = a.output.resolve()
    assert 0 <= a.manual_seconds <= 3600
    if a.private_worker:
        desktop = Desktop(a, a.private_worker)
        try:
            desktop.run()
        except BaseException as error:
            desktop.report["status"] = "failed"
            desktop.report["error"] = repr(error)
            raise
        finally:
            desktop.cleanup()
        return
    report = preflight(a)
    if not a.run:
        return
    assert not report["missing"], "Install approved prerequisites before running"
    assert os.environ.get("HYPRLAND_INSTANCE_SIGNATURE"), "Host Hyprland identity required"
    host_runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    host_socket = Path(os.environ["WAYLAND_DISPLAY"])
    if not host_socket.is_absolute():
        host_socket = host_runtime / host_socket
    assert host_socket.exists(), "Host Wayland socket is unavailable"
    a.output.mkdir(parents=True, exist_ok=False)
    # Hyprland IPC appends a long instance ID; keep the runtime's UNIX socket
    # path well below sockaddr_un's limit independent of the artifact path.
    with tempfile.TemporaryDirectory(prefix="nq-", dir="/tmp") as directory:
        root = Path(directory)
        for name in ("r", "config", "data", "state", "cache"):
            (root / name).mkdir(mode=0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=str(root / "r"), XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"), XDG_STATE_HOME=str(root / "state"), XDG_CACHE_HOME=str(root / "cache"), GSETTINGS_BACKEND="memory", LANG="C.UTF-8", LANGUAGE="en", NOTES_QA_HOST_RUNTIME=str(host_runtime), NOTES_QA_HOST_INSTANCE=os.environ["HYPRLAND_INSTANCE_SIGNATURE"], NOTES_QA_HOST_WAYLAND=str(host_socket))
        for name in ("DISPLAY", "AT_SPI_BUS_ADDRESS", "XDG_ACTIVATION_TOKEN", "HYPRLAND_INSTANCE_SIGNATURE", "XDG_SESSION_ID", "DBUS_SESSION_BUS_ADDRESS", "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE", "NOTIFY_SOCKET", "WATCHDOG_USEC", "WATCHDOG_PID", "MANAGERPID", "PULSE_SERVER", "PULSE_COOKIE"):
            env.pop(name, None)
        # Only AT-SPI activation is needed. A standard session bus also starts
        # portals/GVfs and a document FUSE mount when fcitx probes settings.
        # Keep this owned session's service inventory explicit and minimal.
        services = root / "bus-services"
        services.mkdir()
        (services / "org.a11y.Bus.service").write_text("[D-BUS Service]\nName=org.a11y.Bus\nExec=/usr/lib/at-spi-bus-launcher\n")
        bus_config = root / "dbus.conf"
        bus_config.write_text(f'''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir={root / 'r'}</listen>
  <auth>EXTERNAL</auth>
  <servicedir>{services}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
''')
        command = ["dbus-run-session", "--config-file=" + str(bus_config), "--", sys.executable, str(Path(__file__).resolve()), "--run", "--private-worker", directory, "--binary", str(a.binary), "--output", str(a.output), "--mode", a.mode, "--manual-seconds", str(a.manual_seconds)]
        if a.routing_proof:
            command.append("--routing-proof")
        with (a.output / "driver.log").open("w") as driver_log:
            session = subprocess.Popen(command, env=env, stdout=driver_log, stderr=driver_log, start_new_session=True)
            try:
                returncode = session.wait()
            finally:
                # The private bus and its activation helpers belong to this
                # new group; the worker separately stops app/service groups.
                try:
                    os.killpg(session.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        print("Actual engine report: " + str(a.output / "report.json"), flush=True)
        report_path = a.output / "report.json"
        if report_path.exists():
            recorded = json.loads(report_path.read_text())
            driver_text = (a.output / "driver.log").read_text()
            warnings = [line for line in driver_text.splitlines()
                        if ("AddAccessible with unknown signature" in line or "Unknown signature" in line and "RemoveAccessible" in line)]
            recorded["invalid_cache_client_warnings"] = warnings
            if warnings and recorded.get("status") == "observed":
                recorded["status"] = "failed"
                recorded["error"] = "Installed libatspi rejected cache signal signatures"
                returncode = 1
            report_path.write_text(json.dumps(recorded, indent=2) + "\n")
        if returncode:
            raise subprocess.CalledProcessError(returncode, command)


if __name__ == "__main__":
    main()
