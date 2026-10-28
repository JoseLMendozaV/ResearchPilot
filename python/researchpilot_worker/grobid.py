"""Optional client for a GROBID service running on this machine.

Only loopback URLs are accepted unless the caller explicitly allows remote
processing (decided in Rust from the privacy settings). Consolidation is
disabled because it would make GROBID query external bibliographic services.
"""

from __future__ import annotations

import ipaddress
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ElementTree
from typing import Any, Optional

from .protocol import WorkerError

TEI = "{http://www.tei-c.org/ns/1.0}"
MAX_RESPONSE_BYTES = 64 * 1024 * 1024


def is_loopback(url: str) -> bool:
    host = urllib.parse.urlparse(url).hostname or ""
    if host == "localhost":
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def _opener() -> urllib.request.OpenerDirector:
    # Ignore proxy environment variables: a proxy would turn a local call into a remote one.
    return urllib.request.build_opener(urllib.request.ProxyHandler({}))


def check_url(url: str, allow_remote: bool) -> str:
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme not in ("http", "https") or not parsed.hostname:
        raise WorkerError("grobid_invalid_url", "La URL de GROBID no es válida")
    if not allow_remote and not is_loopback(url):
        raise WorkerError(
            "grobid_remote_blocked",
            "GROBID debe ejecutarse en este equipo (localhost) mientras el procesamiento externo esté desactivado",
        )
    return url.rstrip("/")


def is_alive(url: str, allow_remote: bool, timeout: float = 3.0) -> bool:
    try:
        base = check_url(url, allow_remote)
        with _opener().open(base + "/api/isalive", timeout=timeout) as response:
            return response.status == 200 and response.read(16).strip().lower() == b"true"
    except (WorkerError, OSError, urllib.error.URLError, ValueError):
        return False


def process_fulltext(pdf_path: str, url: str, allow_remote: bool, timeout: float) -> dict[str, Any]:
    base = check_url(url, allow_remote)
    boundary = uuid.uuid4().hex
    with open(pdf_path, "rb") as handle:
        pdf_bytes = handle.read()
    fields = {
        "consolidateHeader": "0",
        "consolidateCitations": "0",
        "consolidateFunders": "0",
        "includeRawCitations": "1",
        "includeRawAffiliations": "1",
    }
    body = bytearray()
    for name, value in fields.items():
        body += f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n".encode()
    body += (
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"input\"; filename=\"document.pdf\"\r\n"
        "Content-Type: application/pdf\r\n\r\n"
    ).encode()
    body += pdf_bytes + f"\r\n--{boundary}--\r\n".encode()
    request = urllib.request.Request(
        base + "/api/processFulltextDocument",
        data=bytes(body),
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}", "Accept": "application/xml"},
        method="POST",
    )
    try:
        with _opener().open(request, timeout=timeout) as response:
            payload = response.read(MAX_RESPONSE_BYTES + 1)
    except urllib.error.HTTPError as error:
        raise WorkerError("grobid_failed", f"GROBID respondió {error.code}", retryable=error.code >= 500) from error
    except (OSError, urllib.error.URLError) as error:
        raise WorkerError("grobid_unreachable", f"No se pudo contactar GROBID: {error}", retryable=True) from error
    if len(payload) > MAX_RESPONSE_BYTES:
        raise WorkerError("grobid_failed", "La respuesta de GROBID excede el tamaño permitido")
    return parse_tei(payload)


def _text(element: Optional[ElementTree.Element]) -> Optional[str]:
    if element is None:
        return None
    value = " ".join("".join(element.itertext()).split())
    return value or None


def _person_name(element: Optional[ElementTree.Element]) -> Optional[str]:
    """Join name parts explicitly: TEI often has no whitespace between them."""
    if element is None:
        return None
    parts = [_text(part) for part in element if part.tag in (f"{TEI}forename", f"{TEI}surname")]
    return " ".join(part for part in parts if part) or _text(element)


def parse_tei(payload: bytes) -> dict[str, Any]:
    try:
        root = ElementTree.fromstring(payload)
    except ElementTree.ParseError as error:
        raise WorkerError("grobid_failed", f"TEI inválido: {error}") from error

    header = root.find(f".//{TEI}teiHeader")
    analytic = header.find(f".//{TEI}sourceDesc//{TEI}analytic") if header is not None else None
    monogr = header.find(f".//{TEI}sourceDesc//{TEI}monogr") if header is not None else None

    authors: list[dict[str, Any]] = []
    for author in analytic.findall(f"{TEI}author") if analytic is not None else []:
        name = _person_name(author.find(f"{TEI}persName"))
        if not name:
            continue
        affiliations = [
            value
            for value in (_text(node.find(f"{TEI}note[@type='raw_affiliation']")) or _text(node) for node in author.findall(f"{TEI}affiliation"))
            if value
        ]
        countries = [value for value in (_text(node) for node in author.findall(f".//{TEI}country")) if value]
        orcid = _text(author.find(f"{TEI}idno[@type='ORCID']"))
        authors.append({"name": name, "affiliations": affiliations, "countries": countries, "orcid": orcid})

    date = monogr.find(f".//{TEI}imprint/{TEI}date[@type='published']") if monogr is not None else None
    references = []
    for index, bibl in enumerate(root.findall(f".//{TEI}back//{TEI}listBibl/{TEI}biblStruct"), start=1):
        raw = _text(bibl.find(f"{TEI}note[@type='raw_reference']")) or _text(bibl)
        if raw:
            references.append(
                {
                    "ordinal": index,
                    "raw": raw[:4000],
                    "doi": (_text(bibl.find(f".//{TEI}idno[@type='DOI']")) or "").lower() or None,
                    "title": _text(bibl.find(f"{TEI}analytic/{TEI}title")) or _text(bibl.find(f"{TEI}monogr/{TEI}title")),
                }
            )

    return {
        "title": _text(header.find(f".//{TEI}titleStmt/{TEI}title")) if header is not None else None,
        "doi": (_text(header.find(f".//{TEI}idno[@type='DOI']")) or "").lower() or None if header is not None else None,
        "abstract": _text(header.find(f".//{TEI}profileDesc/{TEI}abstract")) if header is not None else None,
        "keywords": [value for value in (_text(term) for term in root.findall(f".//{TEI}keywords/{TEI}term")) if value],
        "venue": _text(monogr.find(f"{TEI}title")) if monogr is not None else None,
        "publicationDate": date.get("when") if date is not None else None,
        "authors": authors,
        "headings": [value for value in (_text(head) for head in root.findall(f".//{TEI}body//{TEI}head")) if value],
        "references": references,
    }
