#!/usr/bin/env python3
"""Collect locked Linux dependency notices; this is not a legal approval gate."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import tomllib
from pathlib import Path

MAX_TEXT_BYTES = 2 * 1024 * 1024
TEXT_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT")
SUPPLEMENTS = Path(__file__).with_name("license-supplements.json")


def normal_build_closure(metadata: dict, package: str) -> tuple[list[dict], dict]:
    packages = {item["id"]: item for item in metadata["packages"]}
    roots = [item for item in packages.values() if item["name"] == package and item["id"] in metadata["workspace_members"]]
    if len(roots) != 1:
        raise ValueError(f"Expected one workspace package named {package!r}")
    nodes = {item["id"]: item for item in metadata["resolve"]["nodes"]}
    pending, seen = [roots[0]["id"]], set()
    while pending:
        ident = pending.pop()
        if ident in seen:
            continue
        seen.add(ident)
        for dependency in nodes[ident]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])
    return [packages[ident] for ident in seen], roots[0]


def stable_source(package: dict, workspace: Path) -> str:
    if package["source"]:
        return package["source"]
    manifest = Path(package["manifest_path"]).resolve()
    try:
        return "workspace:" + manifest.relative_to(workspace).as_posix()
    except ValueError:
        raise ValueError(f"External unversioned path dependency needs review: {package['name']}") from None


def is_notice(path: Path) -> bool:
    return path.name.upper().startswith(TEXT_PREFIXES) and path.is_file()


def source_boundary(root: Path) -> Path:
    for parent in (root, *root.parents):
        manifest = parent / "Cargo.toml"
        if manifest.is_file() and "workspace" in tomllib.loads(manifest.read_text()):
            return parent
    return root


def candidates(package: dict, workspace: Path) -> list[Path]:
    root = Path(package["manifest_path"]).resolve().parent
    found = []
    declared = package.get("license_file")
    if declared:
        path = Path(declared)
        found.append((root / path).resolve() if not path.is_absolute() else path.resolve())
    # Include packaged native/vendor notices inside this exact crate, too. Never
    # recursively scan a shared upstream workspace for unrelated editor assets.
    found.extend(path for path in root.rglob("*") if is_notice(path) and "target" not in path.relative_to(root).parts)
    if not found and package["id"] in package.get("_workspace_members", []):
        found.extend(path for path in workspace.iterdir() if is_notice(path))
    if not found and (package.get("source") or "").startswith("git+"):
        # GPUI's Apache crates share the pinned monorepo LICENSE-APACHE; the
        # monorepo also has GPL editor code. Do not imply those GPL crates ship.
        expression = package.get("license") or ""
        boundary = source_boundary(root)
        for parent in root.parents:
            if not parent.is_relative_to(boundary):
                break
            names = []
            if "Apache-2.0" in expression:
                names.append("LICENSE-APACHE")
            if "MIT" in expression:
                names.append("LICENSE-MIT")
            found.extend(parent / name for name in names if (parent / name).is_file())
            # XIM's pinned workspace uses one plain LICENSE. Accept that only
            # when its declaring package has exactly the same expression. This
            # must not apply Zed's GPL root license to Apache GPUI crates.
            manifest = parent / "Cargo.toml"
            if not found and manifest.is_file():
                declared = tomllib.loads(manifest.read_text()).get("package", {}).get("license")
                if declared == expression:
                    found.extend(parent / name for name in ("LICENSE", "COPYING") if (parent / name).is_file())
            if found or parent == boundary or (parent / ".git").exists() or parent == Path.home():
                break
    return sorted(set(found), key=str)


def exact_tree_packages(package: str, target: str, packages: list[dict]) -> set[str]:
    """Metadata unifies workspace features; cargo tree selects this app's graph."""
    result = subprocess.run([
        "cargo", "tree", "--locked", "--offline", "--package", package,
        "--target", target, "--edges", "normal,build", "--prefix", "none",
        "--format", "{p}",
    ], check=True, capture_output=True, text=True)
    selected = set()
    for line in result.stdout.splitlines():
        match = re.match(r"^(\S+) v([^\s]+)", line)
        if not match:
            raise ValueError(f"Unrecognised cargo tree package: {line!r}")
        name, version = match.groups()
        choices = [p for p in packages if p["name"] == name and p["version"] == version]
        if len(choices) != 1:
            raise ValueError(f"Ambiguous normal/build source identity: {name} {version}")
        selected.add(choices[0]["id"])
    return selected


def publisher_evidence(item: dict, checksums: dict) -> dict:
    root = Path(item["manifest_path"]).resolve().parent
    vcs = root / ".cargo_vcs_info.json"
    revision = None
    if vcs.is_file():
        revision = json.loads(vcs.read_text()).get("git", {}).get("sha1")
    elif (item.get("source") or "").startswith("git+"):
        revision = item["source"].rsplit("#", 1)[-1]
    original = root / "Cargo.toml.orig"
    return {
        "authors": item.get("authors", []), "repository": item.get("repository"),
        "homepage": item.get("homepage"), "vcs_revision": revision,
        "registry_package_sha256": checksums.get((item["name"], item["version"], item.get("source"))),
        "manifest_sha256": hashlib.sha256(Path(item["manifest_path"]).read_bytes()).hexdigest(),
        "original_manifest_sha256": hashlib.sha256(original.read_bytes()).hexdigest() if original.is_file() else None,
    }


def copyright_evidence(item: dict) -> list[dict]:
    """Bounded, literal source evidence for gaps; authors are not rights holders."""
    root = Path(item["manifest_path"]).resolve().parent
    result = []
    for name in ("src/lib.rs", "README.md", "README", "Cargo.toml.orig"):
        path = root / name
        if not path.is_file() or path.stat().st_size > MAX_TEXT_BYTES:
            continue
        data = path.read_bytes()
        lines = data.decode("utf-8").splitlines()
        excerpts = [{"line": n, "text": line} for n, line in enumerate(lines, 1)
                    if "copyright" in line.lower() or "licensed under" in line.lower()][:32]
        if excerpts:
            result.append({"upstream_path": name, "sha256": hashlib.sha256(data).hexdigest(), "excerpts": excerpts})
    return result


def supplement_texts(item: dict, evidence: dict, supplement_file: Path | None) -> list[tuple[dict, bytes]]:
    if supplement_file is None or not supplement_file.is_file():
        return []
    supplement = json.loads(supplement_file.read_text())
    if supplement.get("version") != 1:
        raise ValueError("Unsupported license supplement format")
    matches = [entry for entry in supplement["packages"] if
               (entry["name"], entry["version"], entry["source"]) == (item["name"], item["version"], item["source"])]
    if len(matches) > 1:
        raise ValueError(f"Duplicate license supplement: {item['name']}")
    if not matches:
        return []
    entry = matches[0]
    if (entry["license"] != item.get("license") or
        entry["registry_package_sha256"] != evidence["registry_package_sha256"] or
        entry["vcs_revision"] != evidence["vcs_revision"]):
        raise ValueError(f"License supplement does not match locked publisher identity: {item['name']}")
    result = []
    boundary = supplement_file.parent.resolve()
    for notice in entry["files"]:
        path = (boundary / notice["path"]).resolve(strict=True)
        if not path.is_file() or not path.is_relative_to(boundary) or path.stat().st_size > MAX_TEXT_BYTES:
            raise ValueError(f"Invalid supplement path: {notice['path']}")
        data = path.read_bytes()
        data.decode("utf-8")
        if not data or hashlib.sha256(data).hexdigest() != notice["sha256"]:
            raise ValueError(f"License supplement hash mismatch: {notice['path']}")
        if notice["upstream_revision"] != entry["vcs_revision"]:
            raise ValueError(f"License supplement revision mismatch: {notice['path']}")
        result.append((notice, data))
    return result


def collect(metadata: dict, package: str, target: str, output: Path, selected: set[str] | None = None,
            supplement_file: Path | None = SUPPLEMENTS, candidate: bool = False) -> dict:
    if output.exists() or output.is_symlink():
        raise ValueError(f"Refusing existing output: {output}")
    workspace = Path(metadata["workspace_root"]).resolve()
    release_kind = workspace / "RELEASE_KIND"
    if candidate and release_kind.exists() and release_kind.read_text().strip() != "candidate":
        raise ValueError("Candidate collection refused for an archive not explicitly marked candidate")
    lock = workspace / "Cargo.lock"
    lock_data = tomllib.loads(lock.read_text()) if lock.is_file() else {}
    checksums = {(p["name"], p["version"], p.get("source")): p.get("checksum") for p in lock_data.get("package", [])}
    packages, root_package = normal_build_closure(metadata, package)
    if selected is not None:
        packages = [p for p in packages if p["id"] in selected]
    records, missing = [], []
    # Assemble everything before creating an output so malformed metadata cannot
    # leave a falsely successful inventory or partially overwrite old notices.
    texts = []
    for item in sorted(packages, key=lambda p: (p["name"], p["version"], stable_source(p, workspace))):
        source = stable_source(item, workspace)
        directory = f"{item['name']}-{item['version']}-{hashlib.sha256(source.encode()).hexdigest()[:12]}"
        record = {
            "name": item["name"], "version": item["version"], "source": source,
            "license": item.get("license"), "license_files": [],
            "publisher_evidence": publisher_evidence(item, checksums),
        }
        local = dict(item, _workspace_members=metadata["workspace_members"])
        for path in candidates(local, workspace):
            original = path
            boundary = source_boundary(Path(item["manifest_path"]).resolve().parent)
            path = path.resolve(strict=True)
            if not path.is_file() or not path.is_relative_to(boundary):
                raise ValueError(f"License text is not regular or escapes its source workspace: {original}")
            if path.stat().st_size > MAX_TEXT_BYTES:
                raise ValueError(f"License text exceeds 2 MiB: {path}")
            data = path.read_bytes()
            if not data:
                raise ValueError(f"Empty license text: {path}")
            data.decode("utf-8")
            digest = hashlib.sha256(data).hexdigest()
            destination = f"{directory}/{path.name}-{digest[:12]}"
            origin = Path(item["manifest_path"]).resolve().parent
            # Relative upstream paths preserve provenance without machine paths.
            provenance = Path(os.path.relpath(path, origin)).as_posix()
            record["license_files"].append({"path": destination, "upstream_path": provenance, "sha256": digest})
            texts.append((destination, data))
        if not record["license_files"]:
            for notice, data in supplement_texts(item, record["publisher_evidence"], supplement_file):
                destination = f"{directory}/{Path(notice['path']).name}"
                record["license_files"].append({
                    "path": destination, "sha256": notice["sha256"],
                    "upstream_url": notice["url"], "upstream_revision": notice["upstream_revision"],
                    "origin": "checked-in exact-revision source supplement; absent from published crate archive",
                })
                texts.append((destination, data))
        if not record["license"] or not record["license_files"]:
            missing.append(f"{item['name']} {item['version']}: missing declaration or redistribution text")
            record["source_copyright_evidence"] = copyright_evidence(item)
            record["source_copyright_evidence_limit"] = "Only bounded src/lib.rs, README and original manifest inspection; an empty result does not prove no copyright holder."
        record["manual_review"] = (
            "Review expression, copyright/NOTICE requirements, native vendored components, and any exceptions; collection does not select a legal licensing option."
        )
        records.append(record)
    inventory = {
        "version": 1, "package": package, "package_version": root_package["version"],
        "target": target,
        "graph": "cargo-tree-selected non-dev normal+build closure" if selected is not None else "supplied metadata non-dev normal+build closure; workspace feature unification may overinclude dependencies",
        "status": "collected; legal and native/runtime review remains required" if not missing else "incomplete",
        "purpose": "local candidate testing only; incomplete notices prohibit a stable completeness claim" if candidate else "require complete source notice collection; legal review remains separate",
        "collection_complete": not missing,
        "limitations": [
            "Build dependencies are included as conservative provenance, not a claim that their code is in the executable.",
            "Cargo license declarations are not independently verified SPDX conclusions.",
            "Dynamic system libraries, bundled fonts/icons, generated native code, and modified vendor notices require separate artifact review.",
            "This does not prove runtime loading, clean installation, or stable release acceptance.",
        ],
        "missing": missing, "packages": records,
    }
    output.mkdir(parents=True, mode=0o700)
    for destination, data in texts:
        path = output / destination
        path.parent.mkdir(exist_ok=True)
        path.write_bytes(data)
    (output / "inventory.json").write_text(json.dumps(inventory, indent=2, ensure_ascii=False, sort_keys=True) + "\n")
    return inventory


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", default="omarchy-notes-preview")
    parser.add_argument("--target", default="x86_64-unknown-linux-gnu")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, help="Use saved metadata without Cargo; closure may overinclude unified workspace features")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--candidate", action="store_true", help="Local candidate testing only: retain explicit gaps and allow an incomplete inventory; never a stable licensing gate")
    mode.add_argument("--require-complete", action="store_true", help="Fail on every missing declaration or source notice text (the default)")
    args = parser.parse_args()
    try:
        if args.metadata:
            metadata = json.loads(args.metadata.read_text())
            selected = None
        else:
            result = subprocess.run([
                "cargo", "metadata", "--locked", "--offline", "--filter-platform",
                args.target, "--format-version", "1",
            ], check=True, capture_output=True, text=True)
            metadata = json.loads(result.stdout)
            packages, _ = normal_build_closure(metadata, args.package)
            selected = exact_tree_packages(args.package, args.target, packages)
        inventory = collect(metadata, args.package, args.target, args.output, selected, candidate=args.candidate)
        print(f"Collected {len(inventory['packages'])} locked package notices; {len(inventory['missing'])} missing entries. Manual license/runtime review remains required.")
        for missing in inventory["missing"]:
            print(missing)
        if args.candidate and inventory["missing"]:
            print("LOCAL CANDIDATE ONLY: source notice collection is incomplete; stable acceptance remains blocked.")
        return 1 if inventory["missing"] and not args.candidate else 0
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"License collection failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
