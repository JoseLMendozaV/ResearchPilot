"""In-memory document model produced by the layout, OCR and basic-text readers.

Coordinates follow pdfplumber: PDF points with the origin at the top-left
corner of the page, as ``[x0, top, x1, bottom]``.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Optional

BBox = list[float]

HEADER_FOOTER = "header_footer"


@dataclass
class Line:
    text: str
    x0: float
    top: float
    x1: float
    bottom: float
    size: float
    bold: bool = False

    @property
    def bbox(self) -> BBox:
        return [round(self.x0, 2), round(self.top, 2), round(self.x1, 2), round(self.bottom, 2)]


@dataclass
class Block:
    page_number: int
    block_type: str
    text: str
    bbox: Optional[BBox]
    font_size: Optional[float]
    # (start, end, bbox) of every source line inside ``text``.
    line_spans: list[tuple[int, int, Optional[BBox]]] = field(default_factory=list)
    column: int = 0
    reading_order: int = 0
    section: Optional[str] = None
    canonical_section: Optional[str] = None

    @classmethod
    def from_lines(cls, page_number: int, lines: list[Line], block_type: str, column: int = 0) -> "Block":
        text = ""
        spans: list[tuple[int, int, Optional[BBox]]] = []
        for line in lines:
            content = line.text.strip()
            if not content:
                continue
            if text:
                # Keep hyphenated line breaks literal ("evalu-ation") so quotes stay verifiable.
                text += "" if text.endswith("-") else " "
            start = len(text)
            text += content
            spans.append((start, len(text), line.bbox))
        boxes = [span[2] for span in spans if span[2] is not None]
        sizes = sorted(line.size for line in lines if line.size)
        return cls(
            page_number=page_number,
            block_type=block_type,
            text=text,
            bbox=union_bbox(boxes) if boxes else None,
            font_size=round(sizes[len(sizes) // 2], 2) if sizes else None,
            line_spans=spans,
            column=column,
        )

    def bbox_for_range(self, start: int, end: int) -> Optional[BBox]:
        boxes = [span[2] for span in self.line_spans if span[2] is not None and span[0] < end and span[1] > start]
        return union_bbox(boxes) if boxes else self.bbox

    def to_json(self) -> dict[str, Any]:
        return {
            "pageNumber": self.page_number,
            "readingOrder": self.reading_order,
            "blockType": self.block_type,
            "text": self.text,
            "bbox": self.bbox,
            "fontSize": self.font_size,
            "column": self.column,
            "sectionPath": self.section,
            "canonicalSection": self.canonical_section,
        }


@dataclass
class Page:
    page_number: int
    width: Optional[float]
    height: Optional[float]
    blocks: list[Block]
    text_source: str
    status: str
    ocr_confidence: Optional[float] = None
    columns: int = 1

    @property
    def text(self) -> str:
        return "\n\n".join(block.text for block in self.blocks if block.text)

    def to_json(self) -> dict[str, Any]:
        return {
            "pageNumber": self.page_number,
            "width": self.width,
            "height": self.height,
            "text": self.text,
            "textSource": self.text_source,
            "status": self.status,
            "ocrConfidence": self.ocr_confidence,
            "columns": self.columns,
        }


def union_bbox(boxes: list[BBox]) -> BBox:
    return [
        round(min(box[0] for box in boxes), 2),
        round(min(box[1] for box in boxes), 2),
        round(max(box[2] for box in boxes), 2),
        round(max(box[3] for box in boxes), 2),
    ]


# --- Section headings -------------------------------------------------------

_NUMBERING = re.compile(r"^\s*(?:(?:\d+(?:\.\d+)*)|(?:[IVXLC]+)|(?:[A-H]))[.)]?\s+(?=\S)")

# Ordered: the first canonical section whose phrase matches wins.
CANONICAL_SECTIONS: list[tuple[str, tuple[str, ...]]] = [
    ("abstract", ("abstract", "resumen", "summary")),
    ("keywords", ("keywords", "key words", "index terms", "palabras clave")),
    ("threats_to_validity", ("threats to validity", "threats to the validity", "validity threats", "amenazas a la validez")),
    ("limitations", ("limitations", "limitaciones")),
    ("future_work", ("future work", "future research", "future directions", "trabajo futuro", "trabajos futuros")),
    ("related_work", ("related work", "related works", "literature review", "state of the art", "previous work", "trabajos relacionados", "estado del arte")),
    ("introduction", ("introduction", "introducción", "introduccion")),
    ("background", ("background", "preliminaries", "antecedentes", "marco teórico", "theoretical framework")),
    ("methods", ("materials and methods", "materials methods", "methodology", "methods", "method", "research method", "experimental setup", "study design", "proposed method", "proposed approach", "metodología", "metodologia", "métodos", "metodos")),
    ("results", ("results", "findings", "experimental results", "evaluation", "experiments", "resultados")),
    ("discussion", ("discussion", "discusión", "discusion")),
    ("conclusion", ("conclusions", "conclusion", "concluding remarks", "conclusiones", "conclusión")),
    ("acknowledgments", ("acknowledgments", "acknowledgements", "acknowledgment", "acknowledgement", "agradecimientos")),
    ("funding", ("funding", "financial support", "financiamiento", "financiación")),
    ("data_availability", ("data availability", "availability of data", "data and code availability", "code availability", "data access")),
    ("ethics", ("ethics statement", "ethical approval", "ethical considerations", "ethics", "consideraciones éticas")),
    ("conflicts", ("conflict of interest", "conflicts of interest", "competing interests", "competing interest", "declaration of competing interest", "conflicto de intereses")),
    ("references", ("references", "bibliography", "referencias", "works cited", "literature cited", "bibliografía")),
    ("appendix", ("appendix", "appendices", "supplementary material", "supplementary materials", "anexo", "apéndice")),
]


def strip_numbering(text: str) -> str:
    return _NUMBERING.sub("", text, count=1).strip()


def canonical_section(heading: str, loose: bool = False) -> Optional[str]:
    """Map a heading to a canonical section.

    Matching is by prefix. ``loose`` also accepts multi-word phrases anywhere
    in the heading and is only safe for text already known to be a heading
    (bold or larger font), never for ordinary sentences.
    """
    normalized = re.sub(r"[^\w\s]", " ", strip_numbering(heading).lower())
    normalized = re.sub(r"\s+", " ", normalized).strip()
    if not normalized or len(normalized) > 80:
        return None
    for canonical, phrases in CANONICAL_SECTIONS:
        for phrase in phrases:
            if normalized == phrase or normalized.startswith(phrase + " "):
                return canonical
    if loose:
        for canonical, phrases in CANONICAL_SECTIONS:
            for phrase in phrases:
                if " " in phrase and phrase in normalized:
                    return canonical
    return None


def is_numbered_heading(text: str) -> bool:
    stripped = text.strip()
    if len(stripped) > 100 or stripped.endswith((".", ",", ";", ":")):
        return False
    match = _NUMBERING.match(stripped)
    if not match:
        return False
    rest = stripped[match.end():]
    return bool(rest) and rest[0].isupper() and len(rest.split()) <= 12


CAPTION = re.compile(r"^(?:Table|TABLE|Tab\.|Fig\.|Figure|FIGURE|FIG\.|Tabla|Figura)\s*([0-9]+|[IVXL]+)\b")


def caption_kind(text: str) -> Optional[tuple[str, str]]:
    match = CAPTION.match(text.strip())
    if not match:
        return None
    word = text.strip().split()[0].lower()
    kind = "table" if word.startswith(("tab",)) else "figure"
    return kind, match.group(0).strip()


def assign_sections(pages: list[Page]) -> list[dict[str, Any]]:
    """Propagate the current heading to every following block in reading order."""
    current: Optional[str] = None
    canonical: Optional[str] = None
    sections: list[dict[str, Any]] = []
    for page in pages:
        for block in page.blocks:
            if block.block_type == HEADER_FOOTER:
                continue
            if block.block_type == "heading":
                current = strip_numbering(block.text) or block.text
                found = canonical_section(block.text, loose=True)
                # Sub-headings inside a known section keep the parent canonical name.
                if found or not is_subsection(block.text):
                    canonical = found
                sections.append(
                    {
                        "title": block.text,
                        "canonical": found,
                        "pageNumber": page.page_number,
                        "bbox": block.bbox,
                    }
                )
            block.section = current
            block.canonical_section = canonical
            if canonical == "references" and block.block_type == "paragraph":
                block.block_type = "reference"
    return sections


def is_subsection(text: str) -> bool:
    return bool(re.match(r"^\s*\d+\.\d+", text))
