"""Split the references section into individual entries."""

from __future__ import annotations

import re
from typing import Any, Optional

from .document import Page, union_bbox
from .heuristics import DOI, clean_doi

BRACKET = re.compile(r"^\s*\[(\d{1,4})\]\s*")
NUMBERED = re.compile(r"^\s*(\d{1,4})\.\s+(?=[A-Z\[“\"])")
AUTHOR_START = re.compile(r"^[A-Z][A-Za-zÀ-ÿ'’\-]+(?:\s[A-Z][A-Za-zÀ-ÿ'’\-]+)?,\s+(?:[A-Z]\.|[A-Z][a-zà-ÿ]+)")
MAX_REFERENCES = 3000


def parse_references(pages: list[Page]) -> list[dict[str, Any]]:
    lines: list[tuple[str, int, Optional[list[float]]]] = []
    for page in pages:
        for block in page.blocks:
            if block.block_type != "reference":
                continue
            for start, end, bbox in block.line_spans:
                text = block.text[start:end].strip()
                if text:
                    lines.append((text, page.page_number, bbox))
    if not lines:
        return []

    bracket = sum(1 for text, _, _ in lines if BRACKET.match(text))
    numbered = sum(1 for text, _, _ in lines if NUMBERED.match(text))
    if bracket >= 3:
        marker = BRACKET
    elif numbered >= 3:
        marker = NUMBERED
    else:
        marker = None

    entries: list[list[tuple[str, int, Optional[list[float]]]]] = []
    for line in lines:
        text = line[0]
        if marker is not None:
            starts = bool(marker.match(text))
        else:
            previous = entries[-1][-1][0] if entries else ""
            starts = not entries or (bool(AUTHOR_START.match(text)) and previous.rstrip().endswith((".", ")")))
        if starts or not entries:
            entries.append([line])
        else:
            entries[-1].append(line)

    references: list[dict[str, Any]] = []
    for index, entry in enumerate(entries[:MAX_REFERENCES], start=1):
        raw = ""
        for text, _, _ in entry:
            raw += ("" if raw.endswith("-") or not raw else " ") + text
        ordinal = index
        if marker is not None:
            match = marker.match(raw)
            if match:
                ordinal = int(match.group(1))
        doi_match = DOI.search(raw)
        page = entry[0][1]
        boxes = [bbox for _, entry_page, bbox in entry if bbox is not None and entry_page == page]
        references.append(
            {
                "ordinal": ordinal,
                "raw": raw[:4000],
                "doi": clean_doi(doi_match.group(1)) if doi_match else None,
                "title": None,
                "page": page,
                "bbox": union_bbox(boxes) if boxes else None,
            }
        )
    return references
