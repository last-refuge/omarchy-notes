#!/usr/bin/env python3
"""Qt-free PDF regression: structure, text, fonts, links, bounds, repeated headers.
Uses only generated notes and the repository's synthetic Qt fixture.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET


def run(*args):
    return subprocess.check_output(args, text=True)


def check(directory):
    for name, size in [("planning-letter", (612, 792)), ("planning-a4", (595.276, 841.89)), ("pagination", (612, 792)), ("quotes-letter", (612, 792)), ("quotes-a4", (595.276, 841.89))]:
        pdf = directory / f"{name}.pdf"
        run("qpdf", "--check", str(pdf))
        structure = json.loads(run("qpdf", "--json", str(pdf)))
        assert not structure["attachments"], "Attachment labels must not embed files"
        text = run("pdftotext", "-layout", str(pdf), "-")
        compact = re.sub(r"\s+", "", text)
        fonts = run("pdffonts", str(pdf)).splitlines()[2:]
        assert fonts and all(re.search(r"\byes\s+(yes|no)\s+yes\s+\d+\s+\d+\s*$", f) for f in fonts), fonts
        bbox = ET.fromstring(run("pdftotext", "-bbox", str(pdf), "-"))
        pages = bbox.findall(".//{*}page")
        for page in pages:
            assert abs(float(page.attrib["width"]) - size[0]) < .1
            assert abs(float(page.attrib["height"]) - size[1]) < .1
            for word in page.findall(".//{*}word"):
                assert 52 <= float(word.attrib["xMin"]) <= float(word.attrib["xMax"]) <= size[0] - 52, (name, word.text, word.attrib)
                assert 28 <= float(word.attrib["yMin"]) <= float(word.attrib["yMax"]) <= size[1] - 20, (name, word.text, word.attrib)
        for obj in structure["qpdf"][1].values():
            value = obj.get("value", {})
            if isinstance(value, dict) and value.get("/Subtype") == "/Link":
                x0, y0, x1, y1 = value["/Rect"]
                assert 52 <= x0 <= x1 <= size[0] - 52, (name, value)
                assert 20 <= y0 <= y1 <= size[1] - 20, (name, value)
        values = str(structure["qpdf"])
        if name.startswith("planning"):
            assert len(pages) == 2
            for phrase in ["Q3 planning — product sync", "Q3 roadmap draft.pdf", "contents not included", "Localized greeting for the demo:", "你好", "SQLite FTS5"]:
                assert re.sub(r"\s+", "", phrase) in compact, phrase
            assert "https://sqlite.org/fts5.html" in values
            assert "note://" not in values
            assert any(p["images"] for p in structure["pages"])
        elif name.startswith("quotes"):
            assert 2 < len(pages) < 15
            for i in range(140):
                assert compact.count(f"Thought{i:03}:") == 1, i
            assert compact.count("Aquotationinatablecell.") == 1
            # Gold rules and continuation layout also require raster review;
            # selectable text and nonempty content streams alone cannot prove them.
        else:
            for i in range(45):
                assert compact.count(f"Sentence{i:02}:") == 1, i
            for i in range(60):
                assert compact.count(f"Long-row-{i:02}:") == 1, i
            for row in range(1, 14):
                for col in range(2):
                    if row != 5 or col != 1:
                        assert compact.count(f"Cell-{row:02}-{col}:") == 1, (row, col)
            for phrase in ["A merged header", "Two rows, one thought", "Across two columns", "Left", "Right", "Final sentence", "日本語", "Ελληνικά"]:
                assert re.sub(r"\s+", "", phrase) in compact, phrase
            assert compact.count("Milestone") >= 3
            assert "https://example.com/a?x='quoted'&y=two" in values
        print(f"PASS {name}: {len(pages)} pages, selectable text, embedded fonts, valid links and printable bounds")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("example", help="Path to the built notes-export pdf-check example")
    parser.add_argument("--output", type=Path, help="Keep generated PDFs in a new directory for visual review")
    args = parser.parse_args()
    if args.output:
        args.output.mkdir(parents=True, exist_ok=False)
        subprocess.run([args.example, str(args.output)], check=True)
        check(args.output)
    else:
        with tempfile.TemporaryDirectory(prefix="notes-pdf-test-") as tmp:
            directory = Path(tmp)
            subprocess.run([args.example, str(directory)], check=True)
            check(directory)
