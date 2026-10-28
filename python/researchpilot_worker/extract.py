"""Full extraction pipeline: layout → OCR → sections → references → rules."""

from __future__ import annotations

import hashlib
import os
import time
from typing import Any, Optional

from . import RULESET_VERSION, __version__
from .document import Block, Line, Page, assign_sections, caption_kind
from .heuristics import Candidate, Corpus, normalize_space, run_rules
from .layout import (
    basic_text_page,
    body_font_size,
    build_blocks,
    finalize_reading_order,
    mark_headers_footers,
    order_lines,
    words_to_lines,
)
from .protocol import Emitter, WorkerError
from .references import parse_references

MAX_TABLE_ROWS = 300
MAX_TABLE_COLUMNS = 40
MAX_CELL_CHARS = 300


def sha256_file(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_pdf_path(path: Any) -> str:
    if not isinstance(path, str) or not path:
        raise WorkerError("invalid_request", "Falta la ruta del PDF")
    if not os.path.isfile(path):
        raise WorkerError("file_not_found", "El PDF administrado no existe")
    with open(path, "rb") as handle:
        if handle.read(5) != b"%PDF-":
            raise WorkerError("invalid_pdf", "El archivo no tiene firma PDF")
    return path


class Deadline:
    def __init__(self, seconds: float) -> None:
        self.expires = time.monotonic() + max(seconds, 1)

    def check(self) -> None:
        if time.monotonic() > self.expires:
            raise WorkerError("timeout", "La extracción superó el tiempo máximo configurado", retryable=False)

    def remaining(self) -> float:
        return max(self.expires - time.monotonic(), 1)


def run_extract(request: dict[str, Any], emitter: Emitter) -> dict[str, Any]:
    started = time.monotonic()
    path = validate_pdf_path(request.get("pdfPath"))
    options = request.get("options") or {}
    deadline = Deadline(float(options.get("timeoutSeconds") or 600) - 5)
    max_pages = int(options.get("maxPages") or 2000)

    emitter.progress("hash", 0, 1, "Verificando integridad del documento")
    document_hash = sha256_file(path)
    expected = request.get("expectedSha256")
    if expected and expected != document_hash:
        raise WorkerError("hash_mismatch", "El PDF administrado no coincide con la huella SHA-256 registrada")

    try:
        import pdfplumber  # noqa: F401

        engine = "pdfplumber"
    except ImportError:
        engine = "basic_text"

    tables_raw: list[dict[str, Any]] = []
    images: dict[int, list[list[float]]] = {}
    if engine == "pdfplumber":
        pages, tables_raw, images, body_size, page_count = read_layout(path, options, emitter, deadline, max_pages)
    else:
        emitter.warning(
            "layout_unavailable",
            "pdfplumber no está instalado: se usa el texto básico por página, sin coordenadas, tablas ni OCR. Prepara el entorno del extractor desde Configuración.",
        )
        fallback = request.get("fallbackPages") or []
        pages = [basic_text_page(int(item["pageNumber"]), str(item.get("text") or "")) for item in fallback][:max_pages]
        body_size = None
        page_count = len(fallback)

    mark_headers_footers(pages)
    sections = assign_sections(pages)
    finalize_reading_order(pages)
    deadline.check()

    emitter.progress("references", 0, 1, "Separando referencias bibliográficas")
    references = parse_references(pages)

    emitter.progress("rules", 0, 1, "Aplicando reglas de extracción con evidencia")
    candidates, not_found = run_rules(pages, references, body_size)

    grobid_options = options.get("grobid") or {}
    grobid_used = False
    if grobid_options.get("enabled"):
        grobid_used, grobid_candidates, grobid_references = run_grobid(path, pages, grobid_options, emitter, deadline)
        candidates += grobid_candidates
        if grobid_references:
            references = grobid_references
        found = {candidate.field_key for candidate in candidates}
        not_found = [key for key in not_found if key not in found]

    tables_figures = build_tables_figures(pages, tables_raw, images)
    if any(item["objectType"] == "table" and item["content"].get("rows") for item in tables_figures):
        emitter.warning("tables_approximate", "Las tablas se reconstruyeron automáticamente; verifica celdas combinadas y encabezados contra el PDF.")

    ocr_pages = [page.page_number for page in pages if page.text_source == "ocr"]
    empty_pages = [page.page_number for page in pages if page.status != "extracted"]
    if empty_pages:
        emitter.warning(
            "pages_without_text",
            f"{len(empty_pages)} página(s) sin texto utilizable; requieren OCR o revisión manual.",
        )

    emitter.progress("done", 1, 1, "Extracción completada")
    return {
        "extractorVersion": __version__,
        "rulesetVersion": RULESET_VERSION,
        "engine": engine,
        "grobidUsed": grobid_used,
        "documentSha256": document_hash,
        "pageCount": page_count,
        "processedPages": len(pages),
        "pages": [page.to_json() for page in pages],
        "blocks": [block.to_json() for page in pages for block in page.blocks],
        "sections": sections,
        "tablesFigures": tables_figures,
        "references": references,
        "candidates": [candidate.to_json() for candidate in candidates],
        "notFound": not_found,
        "warnings": emitter.warnings,
        "stats": {
            "blocks": sum(len(page.blocks) for page in pages),
            "ocrPages": ocr_pages,
            "emptyPages": empty_pages,
            "tables": sum(1 for item in tables_figures if item["objectType"] == "table"),
            "figures": sum(1 for item in tables_figures if item["objectType"] == "figure"),
            "references": len(references),
            "candidates": len(candidates),
            "bodyFontSize": body_size,
            "durationMs": int((time.monotonic() - started) * 1000),
        },
    }


def read_layout(
    path: str,
    options: dict[str, Any],
    emitter: Emitter,
    deadline: Deadline,
    max_pages: int,
) -> tuple[list[Page], list[dict[str, Any]], dict[int, list[list[float]]], Optional[float], int]:
    import pdfplumber

    raw_pages: list[dict[str, Any]] = []
    tables_raw: list[dict[str, Any]] = []
    images: dict[int, list[list[float]]] = {}
    try:
        pdf = pdfplumber.open(path)
    except Exception as error:  # pdfminer raises many unrelated exception types
        message = str(error) or error.__class__.__name__
        code = "encrypted_pdf" if "password" in message.lower() or "encrypt" in message.lower() else "invalid_pdf"
        raise WorkerError(code, f"No se pudo abrir el PDF: {message[:300]}") from error

    with pdf:
        page_count = len(pdf.pages)
        limit = min(page_count, max_pages)
        if page_count > max_pages:
            emitter.warning("page_limit", f"Se procesaron {max_pages} de {page_count} páginas por el límite configurado.")
        for index in range(limit):
            deadline.check()
            page_number = index + 1
            page = pdf.pages[index]
            entry: dict[str, Any] = {"page_number": page_number, "width": float(page.width), "height": float(page.height), "lines": [], "failed": False}
            try:
                words = page.extract_words(extra_attrs=["size", "fontname"], keep_blank_chars=False)
                entry["lines"] = words_to_lines(words)
                for table in page.find_tables():
                    rows = table.extract() or []
                    tables_raw.append({"page_number": page_number, "bbox": [round(float(value), 2) for value in table.bbox], "rows": rows})
                images[page_number] = [
                    [round(float(image["x0"]), 2), round(float(image["top"]), 2), round(float(image["x1"]), 2), round(float(image["bottom"]), 2)]
                    for image in page.images
                ]
            except Exception as error:  # a single broken page must not abort the document
                entry["failed"] = True
                emitter.warning("page_failed", f"No se pudo analizar la página: {str(error)[:200]}", page=page_number)
            finally:
                close = getattr(page, "close", None)
                if callable(close):
                    close()
            raw_pages.append(entry)
            emitter.progress("layout", page_number, limit, f"Analizando diseño: página {page_number} de {limit}")

    run_ocr(path, raw_pages, options.get("ocr") or {}, emitter, deadline)

    body_size = body_font_size(line for entry in raw_pages if entry.get("source") != "ocr" for line in entry["lines"])
    pages: list[Page] = []
    for entry in raw_pages:
        lines: list[Line] = entry["lines"]
        columns, ordered = order_lines(lines, entry["width"])
        size = body_font_size(lines) if entry.get("source") == "ocr" else body_size
        blocks = build_blocks(entry["page_number"], ordered, size)
        has_text = any(block.text.strip() for block in blocks)
        pages.append(
            Page(
                page_number=entry["page_number"],
                width=entry["width"],
                height=entry["height"],
                blocks=blocks,
                text_source=entry.get("source", "pdf_text") if has_text else "none",
                status="failed" if entry["failed"] and not has_text else ("extracted" if has_text else "ocr_required"),
                ocr_confidence=entry.get("ocr_confidence"),
                columns=columns,
            )
        )
    return pages, tables_raw, images, body_size, page_count


def run_ocr(path: str, raw_pages: list[dict[str, Any]], options: dict[str, Any], emitter: Emitter, deadline: Deadline) -> None:
    pending = [entry for entry in raw_pages if not entry["failed"] and sum(len(line.text) for line in entry["lines"]) < 20]
    if not pending:
        return
    if not options.get("enabled"):
        emitter.warning("ocr_disabled", f"{len(pending)} página(s) parecen escaneadas; el OCR local está desactivado.")
        return

    from . import ocr

    if not ocr.tesseract_path():
        emitter.warning("ocr_unavailable", "Tesseract no está instalado; instala `tesseract` (por ejemplo con Homebrew) para reconocer páginas escaneadas.")
        return
    try:
        languages = ocr.validate_languages(str(options.get("languages") or "eng"))
        rasterizer = ocr.PageRasterizer(path)
    except (ValueError, ImportError, OSError) as error:
        emitter.warning("ocr_unavailable", f"OCR no disponible: {error}")
        return

    dpi = min(max(int(options.get("dpi") or 300), 150), 400)
    try:
        for position, entry in enumerate(pending, start=1):
            deadline.check()
            emitter.progress("ocr", position, len(pending), f"OCR local: página {entry['page_number']}")
            try:
                lines, confidence = ocr.ocr_page(rasterizer, entry["page_number"] - 1, dpi, languages, min(deadline.remaining(), 180))
            except Exception as error:
                emitter.warning("ocr_failed", f"Falló el OCR de la página: {str(error)[:200]}", page=entry["page_number"])
                continue
            if lines:
                entry["lines"] = lines
                entry["source"] = "ocr"
                entry["ocr_confidence"] = confidence
    finally:
        rasterizer.close()
    emitter.warning("ocr_review", "Se aplicó OCR local; el texto reconocido puede contener errores y todos sus datos requieren revisión.")


def build_tables_figures(pages: list[Page], tables_raw: list[dict[str, Any]], images: dict[int, list[list[float]]]) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    captions: list[tuple[Block, str, str]] = []
    for page in pages:
        for block in page.blocks:
            if block.block_type == "caption":
                kind = caption_kind(block.text)
                if kind:
                    captions.append((block, kind[0], kind[1]))

    used: set[int] = set()
    for table in tables_raw:
        best: Optional[int] = None
        best_distance = float("inf")
        for index, (block, kind, _) in enumerate(captions):
            if kind != "table" or index in used or block.page_number != table["page_number"] or block.bbox is None:
                continue
            distance = min(abs(block.bbox[3] - table["bbox"][1]), abs(block.bbox[1] - table["bbox"][3]))
            if distance < best_distance:
                best, best_distance = index, distance
        caption_block, label = (captions[best][0], captions[best][2]) if best is not None and best_distance < 120 else (None, None)
        if best is not None and caption_block is not None:
            used.add(best)
        items.append(
            {
                "objectType": "table",
                "label": label,
                "caption": caption_block.text[:2000] if caption_block else None,
                "pageNumber": table["page_number"],
                "bbox": table["bbox"],
                "captionBbox": caption_block.bbox if caption_block else None,
                "content": {"rows": clean_rows(table["rows"]), "source": "pdfplumber"},
            }
        )

    for index, (block, kind, label) in enumerate(captions):
        if index in used:
            continue
        region = None
        if kind == "figure" and block.bbox is not None:
            above = [box for box in images.get(block.page_number, []) if box[3] <= block.bbox[1] + 5]
            if above:
                region = max(above, key=lambda box: box[3])
        items.append(
            {
                "objectType": kind,
                "label": label,
                "caption": block.text[:2000],
                "pageNumber": block.page_number,
                "bbox": region,
                "captionBbox": block.bbox,
                "content": {"rows": [], "source": "caption_only"} if kind == "table" else {},
            }
        )
    items.sort(key=lambda item: (item["pageNumber"], (item["captionBbox"] or item["bbox"] or [0, 0])[1]))
    return items


def clean_rows(rows: list[list[Any]]) -> list[list[str]]:
    cleaned = []
    for row in rows[:MAX_TABLE_ROWS]:
        cleaned.append([normalize_space(str(cell or ""))[:MAX_CELL_CHARS] for cell in (row or [])[:MAX_TABLE_COLUMNS]])
    return cleaned


def run_grobid(path: str, pages: list[Page], options: dict[str, Any], emitter: Emitter, deadline: Deadline) -> tuple[bool, list[Candidate], list[dict[str, Any]]]:
    from . import grobid

    url = str(options.get("url") or "")
    emitter.progress("grobid", 0, 1, "Consultando GROBID local")
    try:
        tei = grobid.process_fulltext(path, url, bool(options.get("allowRemote")), min(float(options.get("timeoutSeconds") or 120), deadline.remaining()))
    except WorkerError as error:
        emitter.warning(error.code, f"GROBID no se usó: {error.message}")
        return False, [], []

    corpus = Corpus(pages)
    candidates: list[Candidate] = []

    def add(field_key: str, value: Any, display: str, value_type: str, probe: Optional[str]) -> None:
        if value in (None, "", []):
            return
        evidence = locate(corpus, probe) if probe else None
        candidates.append(
            Candidate(
                field_key=field_key,
                value=value,
                display_value=display,
                value_type=value_type,
                method="grobid:tei",
                origin="grobid",
                evidence=[evidence] if evidence else [],
                needs_review=evidence is None,
                notes="" if evidence else "GROBID no aportó una ubicación verificable en el texto extraído; contrasta con el PDF.",
            )
        )

    add("bibliography.title", tei.get("title"), tei.get("title") or "", "text", tei.get("title"))
    add("bibliography.doi", tei.get("doi"), tei.get("doi") or "", "text", tei.get("doi"))
    add("bibliography.abstract", tei.get("abstract"), tei.get("abstract") or "", "text", (tei.get("abstract") or "")[:80])
    keywords = tei.get("keywords") or []
    add("bibliography.keywords", keywords, "; ".join(keywords), "list", keywords[0] if keywords else None)
    add("bibliography.venue", tei.get("venue"), tei.get("venue") or "", "text", tei.get("venue"))
    add("bibliography.publication_date", tei.get("publicationDate"), tei.get("publicationDate") or "", "date", None)
    authors = tei.get("authors") or []
    names = [author["name"] for author in authors]
    add("bibliography.authors", names, "; ".join(names), "list", names[0] if names else None)
    affiliations = unique(value for author in authors for value in author["affiliations"])
    add("bibliography.affiliations", affiliations, "; ".join(affiliations), "list", affiliations[0][:60] if affiliations else None)
    countries = unique(value for author in authors for value in author["countries"])
    add("bibliography.countries", countries, "; ".join(countries), "list", countries[0] if countries else None)
    orcids = unique(author["orcid"] for author in authors if author.get("orcid"))
    add("bibliography.orcid", orcids, "; ".join(orcids), "list", orcids[0] if orcids else None)

    references = []
    for reference in tei.get("references") or []:
        evidence = locate(corpus, reference["raw"][:60], blocks_filter="reference")
        references.append({**reference, "page": evidence["page"] if evidence else None, "bbox": evidence["bbox"] if evidence else None})
    emitter.progress("grobid", 1, 1, "GROBID completado")
    return True, candidates, references


def locate(corpus: Corpus, probe: str, blocks_filter: Optional[str] = None) -> Optional[dict[str, Any]]:
    """Find ``probe`` in the extracted text and return verifiable evidence."""
    needle = normalize_space(probe).lower()
    if len(needle) < 3:
        return None
    for block in corpus.blocks:
        if blocks_filter and block.block_type != blocks_filter:
            continue
        # A header value found inside a cited reference would be misleading evidence.
        if not blocks_filter and (block.block_type == "reference" or block.canonical_section == "references"):
            continue
        haystack = block.text.lower()
        if len(haystack) != len(block.text):
            continue
        position = haystack.find(needle)
        if position >= 0:
            return corpus.evidence(block, position, position + len(needle))
    return None


def unique(values: Any) -> list[str]:
    seen: list[str] = []
    for value in values:
        if value and value not in seen:
            seen.append(value)
    return seen
