"""Optional local OCR through the Tesseract command-line tool (Apache-2.0).

Pages are rasterized with pypdfium2 (bundled with pdfplumber) and recognized
by a ``tesseract`` executable found on a fixed search path. Nothing leaves the
machine. OCR output is always flagged for human review by the caller.
"""

from __future__ import annotations

import csv
import io
import os
import re
import shutil
import subprocess
import tempfile
from typing import Any, Optional

from .document import Line

SEARCH_PATH = os.pathsep.join(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"])
LANGUAGE_PATTERN = re.compile(r"^[A-Za-z_]{3,16}(\+[A-Za-z_]{3,16}){0,4}$")


def tesseract_path() -> Optional[str]:
    return shutil.which("tesseract", path=os.environ.get("PATH", "") + os.pathsep + SEARCH_PATH)


def tesseract_info() -> dict[str, Any]:
    executable = tesseract_path()
    if not executable:
        return {"available": False, "version": None, "languages": []}
    try:
        version = subprocess.run([executable, "--version"], capture_output=True, text=True, timeout=10)
        langs = subprocess.run([executable, "--list-langs"], capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return {"available": False, "version": None, "languages": []}
    first_line = (version.stdout or version.stderr).strip().splitlines()
    languages = [line.strip() for line in langs.stdout.splitlines()[1:] if line.strip() and line.strip() != "osd"]
    return {
        "available": version.returncode == 0,
        "version": first_line[0] if first_line else None,
        "languages": languages,
    }


def validate_languages(languages: str) -> str:
    value = (languages or "eng").strip()
    if not LANGUAGE_PATTERN.match(value):
        raise ValueError("Idiomas de OCR inválidos; usa códigos Tesseract como eng o eng+spa")
    return value


class PageRasterizer:
    """Keeps one pypdfium2 document open while several pages are OCR-ed."""

    def __init__(self, pdf_path: str) -> None:
        import pypdfium2 as pdfium

        self._document = pdfium.PdfDocument(pdf_path)

    def render_png(self, page_index: int, dpi: int, destination: str) -> None:
        page = self._document[page_index]
        try:
            image = page.render(scale=dpi / 72).to_pil()
            image.save(destination, format="PNG")
        finally:
            page.close()

    def close(self) -> None:
        self._document.close()


def ocr_page(
    rasterizer: PageRasterizer,
    page_index: int,
    dpi: int,
    languages: str,
    timeout_seconds: float,
) -> tuple[list[Line], Optional[float]]:
    executable = tesseract_path()
    if not executable:
        raise FileNotFoundError("tesseract")
    with tempfile.TemporaryDirectory(prefix="researchpilot-ocr-") as directory:
        image_path = os.path.join(directory, "page.png")
        rasterizer.render_png(page_index, dpi, image_path)
        completed = subprocess.run(
            [executable, image_path, "stdout", "-l", languages, "--psm", "3", "tsv"],
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
        )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip()[:400] or "Tesseract falló")
    return parse_tsv(completed.stdout, dpi)


def parse_tsv(tsv: str, dpi: int) -> tuple[list[Line], Optional[float]]:
    """Group Tesseract TSV words by (block, paragraph, line) into lines in PDF points."""
    scale = 72 / dpi
    grouped: dict[tuple[str, str, str], list[dict[str, Any]]] = {}
    confidences: list[float] = []
    reader = csv.DictReader(io.StringIO(tsv), delimiter="\t", quoting=csv.QUOTE_NONE)
    for row in reader:
        text = (row.get("text") or "").strip()
        if row.get("level") != "5" or not text:
            continue
        try:
            confidence = float(row.get("conf") or -1)
            left, top = float(row["left"]), float(row["top"])
            width, height = float(row["width"]), float(row["height"])
        except (KeyError, ValueError):
            continue
        if confidence >= 0:
            confidences.append(confidence)
        key = (row.get("block_num", ""), row.get("par_num", ""), row.get("line_num", ""))
        grouped.setdefault(key, []).append(
            {
                "text": text,
                "x0": left * scale,
                "top": top * scale,
                "x1": (left + width) * scale,
                "bottom": (top + height) * scale,
                "size": height * scale,
            }
        )
    lines = []
    for words in grouped.values():
        words.sort(key=lambda word: word["x0"])
        sizes = sorted(word["size"] for word in words)
        lines.append(
            Line(
                text=" ".join(word["text"] for word in words),
                x0=min(word["x0"] for word in words),
                top=min(word["top"] for word in words),
                x1=max(word["x1"] for word in words),
                bottom=max(word["bottom"] for word in words),
                size=sizes[len(sizes) // 2],
            )
        )
    mean_confidence = round(sum(confidences) / len(confidences) / 100, 3) if confidences else None
    return lines, mean_confidence
