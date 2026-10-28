"""Tiny PDF writer for test fixtures (project-owned content, no third-party rights).

Text uses the standard Helvetica fonts; coordinates are given from the top of
an A4 page so tests can reason in the same space as pdfplumber.
"""

from __future__ import annotations

import zlib
from dataclasses import dataclass
from typing import Optional, Union

PAGE_WIDTH = 595
PAGE_HEIGHT = 842


@dataclass
class Text:
    x: float
    top: float
    size: float
    value: str
    bold: bool = False


@dataclass
class Image:
    x: float
    top: float
    width: float
    height: float
    rgb: bytes
    pixel_width: int
    pixel_height: int


Item = Union[Text, Image]


def _escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def write_pdf(path: str, pages: list[list[Item]], title: Optional[str] = None) -> None:
    objects: list[bytes] = []

    def add(body: bytes) -> int:
        objects.append(body)
        return len(objects)

    regular = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    bold = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
    pages_id = add(b"")  # patched after the page objects exist
    page_ids = []
    for items in pages:
        stream = bytearray()
        xobjects = []
        for item in items:
            if isinstance(item, Text):
                y = PAGE_HEIGHT - item.top - item.size
                font = "F2" if item.bold else "F1"
                stream += f"BT /{font} {item.size} Tf {item.x} {y:.2f} Td ({_escape(item.value)}) Tj ET\n".encode("cp1252")
            else:
                data = zlib.compress(item.rgb)
                image_id = add(
                    f"<< /Type /XObject /Subtype /Image /Width {item.pixel_width} /Height {item.pixel_height} "
                    f"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {len(data)} >>\nstream\n".encode()
                    + data
                    + b"\nendstream"
                )
                name = f"Im{len(xobjects) + 1}"
                xobjects.append((name, image_id))
                y = PAGE_HEIGHT - item.top - item.height
                stream += f"q {item.width} 0 0 {item.height} {item.x} {y:.2f} cm /{name} Do Q\n".encode()
        content_id = add(f"<< /Length {len(stream)} >>\nstream\n".encode() + bytes(stream) + b"endstream")
        xobject_dict = " ".join(f"/{name} {ref} 0 R" for name, ref in xobjects)
        resources = f"<< /Font << /F1 {regular} 0 R /F2 {bold} 0 R >> /XObject << {xobject_dict} >> >>"
        page_ids.append(
            add(
                f"<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] "
                f"/Resources {resources} /Contents {content_id} 0 R >>".encode()
            )
        )
    kids = " ".join(f"{page_id} 0 R" for page_id in page_ids)
    objects[pages_id - 1] = f"<< /Type /Pages /Kids [{kids}] /Count {len(page_ids)} >>".encode()
    catalog = add(f"<< /Type /Catalog /Pages {pages_id} 0 R >>".encode())
    info = add(f"<< /Title ({_escape(title)}) >>".encode() if title else b"<< >>")

    output = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(output))
        output += f"{number} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(output)
    output += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for offset in offsets:
        output += f"{offset:010d} 00000 n \n".encode()
    output += f"trailer\n<< /Size {len(objects) + 1} /Root {catalog} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    with open(path, "wb") as handle:
        handle.write(bytes(output))


def lines(x: float, top: float, values: list[str], size: float = 10, leading: float = 13) -> list[Text]:
    return [Text(x, top + index * leading, size, value) for index, value in enumerate(values)]


def scientific_paper(path: str) -> None:
    """Three-page article: single-column front matter, two-column body, references."""
    page1: list[Item] = [
        Text(72, 70, 18, "Traceable Evidence Extraction for Systematic Reviews", bold=True),
        Text(72, 100, 11, "Ana Researcher, Luis Scientist"),
        Text(72, 116, 9, "https://doi.org/10.1234/rp.2026.001"),
        Text(72, 150, 12, "Abstract", bold=True),
        *lines(72, 170, [
            "This paper studies how literal quotes can support every extracted value in a review.",
            "We evaluate the approach with 120 participants and report an accuracy of 93.2% on a",
            "held-out set. The main contribution of this work is a provenance model for evidence.",
        ]),
        Text(72, 222, 10, "Keywords: systematic review; provenance; information extraction"),
        Text(72, 260, 12, "1 Introduction", bold=True),
        *lines(72, 280, [
            "The aim of this study is to make data extraction verifiable by researchers.",
            "Little research has examined page-level provenance for extracted values.",
            "RQ1: How accurate are rule-based proposals for review fields?",
            "RQ2: Which fields require human validation most often?",
        ]),
    ]
    left = 56
    right = 312
    page2: list[Item] = [
        Text(left, 70, 12, "2 Methods", bold=True),
        *lines(left, 90, [
            "We used the PaperSet dataset with",
            "N = 120 annotated documents.",
            "Models were implemented in PyTorch",
            "1.12 and trained on an NVIDIA",
            "A100 GPU. We applied 10-fold",
            "cross-validation for model selection.",
            "A paired t-test compared the methods",
            "against two baselines from prior work.",
        ], size=10, leading=13),
        Text(right, 70, 12, "3 Results", bold=True),
        *lines(right, 90, [
            "The proposed method achieved an",
            "F1-score of 0.91 and outperformed",
            "the baselines (p < 0.05). Results",
            "were stable across folds in 2025.",
            "Table 1: Field-level agreement.",
        ], size=10, leading=13),
        Text(right, 170, 12, "4 Limitations", bold=True),
        *lines(right, 190, [
            "The corpus covers only English",
            "papers from computer science.",
        ], size=10, leading=13),
    ]
    page3: list[Item] = [
        Text(72, 70, 12, "5 Conclusion", bold=True),
        *lines(72, 90, [
            "Rule-based proposals with literal quotes reduce review effort. Future work will add OCR.",
        ]),
        Text(72, 130, 12, "Funding", bold=True),
        *lines(72, 150, ["This work was supported by the Open Science Foundation under grant number OSF-42."]),
        Text(72, 180, 12, "Data Availability", bold=True),
        *lines(72, 200, ["The code is publicly available at https://github.com/example/researchpilot-fixtures."]),
        Text(72, 240, 12, "References", bold=True),
        *lines(72, 260, [
            "[1] A. Author, Evidence in reviews, Journal of Methods, 2020. doi:10.5555/jm.2020.1.",
            "[2] B. Writer, Provenance graphs for science, in: Proc. of Data Conference, 2021.",
            "[3] C. Analyst, Systematic mapping guidelines revisited, Information and Software",
            "Technology 64 (2015) 1-18.",
        ], size=9, leading=12),
    ]
    write_pdf(path, [page1, page2, page3], title="Traceable Evidence Extraction")


def scanned_page(path: str) -> bool:
    """Single page whose only content is an image of text. Returns False without Pillow."""
    try:
        from PIL import Image as PILImage, ImageDraw, ImageFont
    except ImportError:
        return False
    width, height = 1240, 400
    canvas = PILImage.new("RGB", (width, height), "white")
    draw = ImageDraw.Draw(canvas)
    try:
        font = ImageFont.load_default(size=48)
    except TypeError:  # Pillow < 10.1 has no scalable default font
        return False
    draw.text((40, 60), "Scanned evidence page", fill="black", font=font)
    draw.text((40, 160), "Accuracy reached 88.5% overall", fill="black", font=font)
    write_pdf(path, [[Image(36, 72, 520, 168, canvas.tobytes(), width, height)]])
    return True
