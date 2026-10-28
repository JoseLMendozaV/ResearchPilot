"""Turn positioned words into lines, reading order and typed blocks.

The same pipeline serves native PDF text (pdfplumber) and OCR output
(Tesseract), so both produce comparable blocks with bounding boxes.
"""

from __future__ import annotations

import re
import statistics
from collections import Counter
from typing import Any, Iterable, Optional

from .document import (
    HEADER_FOOTER,
    Block,
    Line,
    Page,
    canonical_section,
    caption_kind,
    is_numbered_heading,
)


def words_to_lines(words: Iterable[dict[str, Any]]) -> list[Line]:
    """Group words into visual lines, splitting rows at column gutters."""
    ordered = sorted(
        (word for word in words if str(word.get("text", "")).strip()),
        key=lambda word: (round(float(word["top"]), 1), float(word["x0"])),
    )
    rows: list[list[dict[str, Any]]] = []
    for word in ordered:
        size = float(word.get("size") or (float(word["bottom"]) - float(word["top"])) or 10)
        tolerance = max(2.0, size * 0.4)
        if rows and abs(float(rows[-1][0]["top"]) - float(word["top"])) <= tolerance:
            rows[-1].append(word)
        else:
            rows.append([word])

    lines: list[Line] = []
    for row in rows:
        row.sort(key=lambda word: float(word["x0"]))
        segment: list[dict[str, Any]] = []
        for word in row:
            if segment:
                size = float(word.get("size") or 10)
                gap = float(word["x0"]) - float(segment[-1]["x1"])
                if gap > max(size * 1.0, 6.0):
                    lines.append(_line_from_words(segment))
                    segment = []
            segment.append(word)
        if segment:
            lines.append(_line_from_words(segment))
    return lines


def _line_from_words(words: list[dict[str, Any]]) -> Line:
    sizes = [float(word.get("size") or (float(word["bottom"]) - float(word["top"]))) for word in words]
    fonts = [str(word.get("fontname") or "") for word in words]
    bold_count = sum(1 for font in fonts if re.search(r"bold|black|heavy|semibold", font, re.IGNORECASE))
    return Line(
        text=" ".join(str(word["text"]) for word in words),
        x0=min(float(word["x0"]) for word in words),
        top=min(float(word["top"]) for word in words),
        x1=max(float(word["x1"]) for word in words),
        bottom=max(float(word["bottom"]) for word in words),
        size=statistics.median(sizes) if sizes else 10.0,
        bold=bold_count > len(words) / 2,
    )


def order_lines(lines: list[Line], page_width: float) -> tuple[int, list[tuple[Line, int]]]:
    """Return the detected column count and lines in reading order.

    Lines that cross the page middle (titles, full-width figures) split the page
    into regions; inside a region the left column is read before the right one.
    """
    if not lines:
        return 1, []
    middle = page_width / 2
    margin = page_width * 0.02

    def side(line: Line) -> int:
        if line.x1 <= middle + margin:
            return 1
        if line.x0 >= middle - margin:
            return 2
        return 0

    sides = [side(line) for line in lines]
    left = sides.count(1)
    right = sides.count(2)
    spanning = sides.count(0)
    two_columns = left >= 5 and right >= 5 and spanning <= (left + right) * 0.6
    if not two_columns:
        return 1, [(line, 0) for line in sorted(lines, key=lambda line: (line.top, line.x0))]

    ordered: list[tuple[Line, int]] = []
    region: list[tuple[Line, int]] = []

    def flush() -> None:
        for column in (1, 2):
            ordered.extend(sorted((item for item in region if item[1] == column), key=lambda item: (item[0].top, item[0].x0)))
        region.clear()

    for line, line_side in sorted(zip(lines, sides), key=lambda item: (item[0].top, item[0].x0)):
        if line_side == 0:
            flush()
            ordered.append((line, 0))
        else:
            region.append((line, line_side))
    flush()
    return 2, ordered


def body_font_size(lines: Iterable[Line]) -> float:
    weighted: Counter[float] = Counter()
    for line in lines:
        weighted[round(line.size, 1)] += len(line.text)
    if not weighted:
        return 10.0
    return weighted.most_common(1)[0][0]


def classify_line(line: Line, body_size: float) -> Optional[str]:
    """Return ``heading`` or ``caption`` for lines that must start their own block."""
    text = line.text.strip()
    if caption_kind(text):
        return "caption"
    if len(text) > 160 or len(text) < 2:
        return None
    larger = line.size >= body_size * 1.15
    emphasized = line.bold or line.size >= body_size * 1.05 or (text.isupper() and len(text) > 3)
    if emphasized and canonical_section(text, loose=True):
        return "heading"
    if len(text) <= 40 and canonical_section(text) and not text.rstrip().endswith((".", ",", ";")) and len(text.split()) <= 4:
        return "heading"
    if is_numbered_heading(text) and emphasized:
        return "heading"
    if larger and len(text) <= 120 and not text.endswith("."):
        return "heading"
    return None


def build_blocks(page_number: int, ordered: list[tuple[Line, int]], body_size: float) -> list[Block]:
    blocks: list[Block] = []
    current: list[Line] = []
    current_column = 0
    current_type = "paragraph"

    def flush() -> None:
        nonlocal current
        if current:
            blocks.append(Block.from_lines(page_number, current, current_type, current_column))
        current = []

    for line, column in ordered:
        line_type = classify_line(line, body_size)
        if line_type == "heading":
            flush()
            # Multi-line headings share size and boldness; merge them.
            if blocks and blocks[-1].block_type == "heading" and blocks[-1].column == column and _close_below(blocks[-1].bbox, line):
                previous = blocks.pop()
                merged = Block.from_lines(page_number, [_line_from_block(previous), line], "heading", column)
                blocks.append(merged)
            else:
                current_type, current_column = "heading", column
                current = [line]
                flush()
            current_type = "paragraph"
            continue
        if line_type == "caption":
            flush()
            current_type, current_column = "caption", column
            current = [line]
            continue
        if current:
            previous = current[-1]
            gap = line.top - previous.bottom
            line_height = max(previous.bottom - previous.top, line.bottom - line.top, 1.0)
            size_changed = abs(line.size - previous.size) > 1.0
            if column != current_column or gap > line_height * 0.9 or gap < -line_height or size_changed:
                flush()
                current_type = "paragraph"
        if not current:
            current_column = column
        current.append(line)
    flush()
    return blocks


def _close_below(bbox: Optional[list[float]], line: Line) -> bool:
    return bbox is not None and 0 <= line.top - bbox[3] <= (line.bottom - line.top) * 0.8


def _line_from_block(block: Block) -> Line:
    box = block.bbox or [0.0, 0.0, 0.0, 0.0]
    return Line(block.text, box[0], box[1], box[2], box[3], block.font_size or 10.0)


def mark_headers_footers(pages: list[Page]) -> None:
    """Flag running heads, footers and page numbers repeated across pages."""
    if not pages:
        return

    def key(text: str) -> str:
        return re.sub(r"\d+", "#", text.strip().lower())

    edge_blocks: list[tuple[Page, Block]] = []
    for page in pages:
        if not page.height:
            continue
        for block in page.blocks:
            if block.bbox is None:
                continue
            near_edge = block.bbox[1] < page.height * 0.07 or block.bbox[3] > page.height * 0.93
            if near_edge and len(block.text) < 200:
                edge_blocks.append((page, block))

    counts = Counter(key(block.text) for _, block in edge_blocks)
    threshold = max(2, int(len(pages) * 0.5))
    for _, block in edge_blocks:
        if re.fullmatch(r"(?:page\s*)?\d+(?:\s*(?:/|of)\s*\d+)?", block.text.strip().lower()) or (
            len(pages) >= 3 and counts[key(block.text)] >= threshold
        ):
            block.block_type = HEADER_FOOTER


def basic_text_page(page_number: int, text: str) -> Page:
    """Build blocks from plain text when no layout engine is available."""
    blocks: list[Block] = []
    paragraph: list[str] = []

    def flush() -> None:
        if paragraph:
            lines = [Line(value, 0, 0, 0, 0, 0) for value in paragraph]
            block = Block.from_lines(page_number, lines, "paragraph")
            block.bbox = None
            block.font_size = None
            block.line_spans = [(start, end, None) for start, end, _ in block.line_spans]
            blocks.append(block)
            paragraph.clear()

    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            flush()
            continue
        if caption_kind(line) or (len(line) <= 100 and (canonical_section(line) and len(line) <= 40 or is_numbered_heading(line))):
            flush()
            kind = "caption" if caption_kind(line) else "heading"
            block = Block(page_number, kind, line, None, None, [(0, len(line), None)])
            blocks.append(block)
            continue
        paragraph.append(line)
    flush()
    return Page(
        page_number=page_number,
        width=None,
        height=None,
        blocks=blocks,
        text_source="basic_text" if blocks else "none",
        status="extracted" if blocks else "ocr_required",
    )


def finalize_reading_order(pages: list[Page]) -> None:
    for page in pages:
        for index, block in enumerate(page.blocks):
            block.reading_order = index
