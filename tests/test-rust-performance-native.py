#!/usr/bin/env python3
"""Owned-window native timing workload; never captures the desktop or reads notes.

Run after building performance-check with performance-diagnostics in release.
The program creates a synthetic notebook. Every key is guarded by its exact
process/window/app ID and the current workspace. Focus loss stops the run.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from rust_native_test import guard_nested_desktop


def run():
    if not __debug__:
        raise RuntimeError("Native ownership guards require Python assertions; do not use -O or PYTHONOPTIMIZE")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "target/release/examples/performance-check")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--source-condition", default="worktree diagnostic; revision label alone does not establish a clean source tree")
    parser.add_argument("--large", action="store_true", help="100 separate edits alternating the start/end of the 3,000-paragraph/20-table note")
    parser.add_argument("--large-typing-only", action="store_true", help="With --large, type 100 edits without navigation, for an edit-only latency distribution; edge-navigation coverage requires a separate run")
    parser.add_argument("--queue-probe", choices=("active", "passive"), help="Optional bounded foreground queue pressure or matched passive control")
    args = parser.parse_args()
    assert not args.large_typing_only or args.large, "--large-typing-only requires --large"
    assert not args.output.exists(), "Choose a new output directory"
    app_id = "org.omarchy.Notes.Performance"
    edits = 100 if args.large else 1000
    binary_sha256 = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    process = None
    outcome = {
        "status": "unverified",
        "source_revision": args.revision,
        "source_condition": args.source_condition,
        "binary_sha256": binary_sha256,
        "requested_edits": edits,
        "completed_input_commands": 0,
    }
    with tempfile.TemporaryDirectory(prefix="notes-performance-native-") as scratch:
        log_path = Path(scratch) / "app.log"
        environment = os.environ.copy()
        environment["OMARCHY_PERFORMANCE_REVISION"] = args.revision
        environment["OMARCHY_PERFORMANCE_BINARY_SHA256"] = binary_sha256
        environment["OMARCHY_PERFORMANCE_SOURCE_CONDITION"] = args.source_condition
        environment["XDG_CONFIG_HOME"] = str(Path(scratch) / "config")
        try:
            with log_path.open("w") as log:
                command = [str(args.binary), "--native", "45" if args.large else "90", "--output", str(args.output)]
                if args.large:
                    command.append("--large")
                if args.queue_probe:
                    command.append("--queue-probe" if args.queue_probe == "active" else "--passive")
                process = subprocess.Popen(command, env=environment, stdout=log, stderr=log)

            def compositor(kind):
                return json.loads(subprocess.check_output(["hyprctl", kind, "-j"], timeout=3))

            deadline = time.monotonic() + 30
            while "PERFORMANCE_WINDOW_READY" not in log_path.read_text():
                assert process.poll() is None, "Diagnostic process exited before readiness"
                assert time.monotonic() < deadline, "Diagnostic startup timed out"
                time.sleep(.05)
            owned = next((w for w in compositor("clients") if w.get("pid") == process.pid and w.get("class") == app_id), None)
            assert owned, "Owned diagnostic window is unavailable"
            address = owned["address"]

            def guard():
                guard_nested_desktop()
                assert process.poll() is None, "Diagnostic process exited during input"
                active = compositor("activewindow")
                workspace = compositor("activeworkspace")
                assert active.get("pid") == process.pid and active.get("address") == address and active.get("class") == app_id, "Refusing input: exact owned test window lost focus"
                assert active["workspace"]["id"] == workspace["id"], "Refusing input: active workspace changed"

            guard()
            started = time.monotonic()
            for index in range(edits):
                guard()
                if args.large and not args.large_typing_only:
                    subprocess.run(["wtype", "-M", "ctrl", "-k", "Home" if index % 2 == 0 else "End", "-m", "ctrl"], check=True, timeout=3)
                    guard()
                subprocess.run(["wtype", "--", "x"], check=True, timeout=3)
                outcome["completed_input_commands"] += 1
                guard()
                time.sleep(.02)
            elapsed = time.monotonic() - started
            process.wait(timeout=110)
            assert process.returncode == 0, "Diagnostic process failed"
            report = json.loads((args.output / "native.json").read_text())
            assert hashlib.sha256(args.binary.read_bytes()).hexdigest() == binary_sha256, "Diagnostic binary changed during sampling"
            expected_workload = "synthetic 3,000-paragraph/20-table note in 10,000-note library" if args.large else "synthetic ordinary note in 10,000-note library"
            assert report["native_workload"] == expected_workload, "The diagnostic selected the wrong workload"
            if args.large:
                assert report["selected_editor_baseline"]["root_paragraph_blocks"] == 3000, "The selected note lacks the required paragraphs"
                assert report["selected_editor_baseline"]["root_table_blocks"] == 20, "The selected note lacks the required tables"
            assert report["edit_revision_delta"] == edits, f"The editor did not observe all {edits} dispatched edits"
            assert report["typed_utf8_byte_delta"] == edits, "The synthetic text does not contain every dispatched edit"
            assert report["input_to_platform_submission"]["samples"] > 0, "No native input timing samples"
            assert report["represented_input_events"] >= edits, "Timing samples do not represent the requested input workload"
            assert report["input_to_platform_submission"]["mid_draw_events_dropped"] == 0, "Native input timing coverage is incomplete: events arrived during draw"
            assert report["final_write_boundary"] == "all pending synthetic edits acknowledged committed", "Final synthetic saves were not acknowledged"
            if args.large and not args.large_typing_only:
                baseline = report["selected_editor_baseline"]
                final = report["selected_editor_final"]
                assert final["leading_x_count"] - baseline["leading_x_count"] == edits // 2, "Control Home did not place half the edits at the document start"
                assert final["trailing_x_count"] - baseline["trailing_x_count"] == edits // 2, "Control End did not place half the edits at the document end"
                assert final["selection_collapsed"] and final["caret_at_end"], "Final edit did not leave the caret at the document end"
            if args.queue_probe:
                probe = report["queue_probe"]
                assert probe["complete"], "The foreground queue probe did not complete"
                requested_callbacks = 2560 if args.queue_probe == "active" else 0
                assert probe["config"]["bursts"] == 40, "The matched probe must record forty bursts"
                for field in ("requested", "enqueued", "completed"):
                    assert probe["callbacks"][field] == requested_callbacks, "The probe callback workload changed or did not finish"
                assert probe["callbacks"]["enqueue_lateness"]["samples"] == 40, "The probe did not record every burst"
                assert probe["observed_editor_progress"]["missing_target_samples"] == 0, "The pinned editor disappeared during the probe"
                assert probe["observed_editor_progress"]["backward_revision_samples"] == 0, "The pinned editor revision moved backward"
                assert probe["observed_editor_progress"]["revision_delta"] > 0, "No typing progress reached the pinned editor during the probe"
                assert report["queue_load_window"]["observed_edit_delta"] > 0, "No edits were observed inside the measured load window"
                assert report["queue_load_window"]["latency_samples"] > 0, "No native submission samples inside the measured load window"
                progress = probe["edit_progress"]
                outcome["queue_progress_assessment"] = "matched passive control; no pressure callbacks" if args.queue_probe == "passive" else (
                    "interleaved revision advances observed" if progress["bursts_with_interleaved_revision_advances"] > 0
                    else "no revision advances observed between callbacks; fairness remains unverified"
                )
                assert not progress["raw_truncated"], "The bounded edit progress journal overflowed"
                assert progress["backward_revision_deliveries"] == 0, "The pinned editor revision moved backward"
            procedure = {
                "source_revision": args.revision,
                "source_condition": args.source_condition,
                "binary_sha256": binary_sha256,
                "edits": edits,
                "selection_moves": edits if args.large and not args.large_typing_only else 0,
                "foreground_queue_probe": args.queue_probe,
                "input": "wtype printable x; exact owned PID/address/app ID and workspace guard before and after each input",
                "minimum_pause_seconds": .02,
                "typing_elapsed_seconds": elapsed,
                "viewport": owned["size"],
                "compositor": subprocess.check_output(["hyprctl", "version"], text=True, timeout=3),
                "scope": ("large note, edit-only typing without navigation" if args.large_typing_only else "large note, mixed Control Home/End navigation and edits" if args.large else "typical note, edit-only typing") + " in 10,000-note synthetic library; GPUI samples coalesced invalidating inputs per frame. Mixed navigation samples cannot establish an edit-only p95; remaining performance scenarios require separate evidence",
                "app_log": log_path.read_text(),
            }
            (args.output / "procedure.json").write_text(json.dumps(procedure, indent=2))
            outcome["status"] = "observed"
            print(f"PASS: {edits} observed native edits; reports:", args.output)
            print(json.dumps(report["input_to_platform_submission"]))
        except BaseException as error:
            outcome["status"] = "failed"
            outcome["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            if process and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            if process:
                outcome["pid"] = process.pid
                outcome["process_exit_code"] = process.poll()
                outcome["owned_process_closed"] = process.poll() is not None
            args.output.mkdir(parents=True, exist_ok=True)
            if log_path.exists():
                (args.output / "app.log").write_text(log_path.read_text())
            (args.output / "run-outcome.json").write_text(json.dumps(outcome, indent=2))


if __name__ == "__main__":
    run()
