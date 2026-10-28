"""Command-line entry point used by the Rust job runner.

Commands:
  health                         Liveness check without optional dependencies.
  capabilities [--grobid-url U]  Report optional engines available locally.
  extract                        Read a JSON request on stdin, stream JSON lines.
"""

from __future__ import annotations

import json
import platform
import sys
import traceback
from typing import Any

from . import PROTOCOL_VERSION, RULESET_VERSION, __version__
from .protocol import Emitter, WorkerError, read_request


def capabilities(grobid_url: str = "", allow_remote: bool = False) -> dict[str, Any]:
    from . import grobid, ocr

    try:
        import pdfplumber

        pdfplumber_version = getattr(pdfplumber, "__version__", "unknown")
    except ImportError:
        pdfplumber_version = None
    try:
        import pypdfium2  # noqa: F401

        has_pypdfium2 = True
    except ImportError:
        has_pypdfium2 = False

    report: dict[str, Any] = {
        "service": "researchpilot-worker",
        "workerVersion": __version__,
        "protocolVersion": PROTOCOL_VERSION,
        "rulesetVersion": RULESET_VERSION,
        "python": platform.python_version(),
        "pdfplumber": pdfplumber_version,
        "pypdfium2": has_pypdfium2,
        "tesseract": ocr.tesseract_info(),
    }
    if grobid_url:
        report["grobid"] = {"url": grobid_url, "reachable": grobid.is_alive(grobid_url, allow_remote)}
    return report


def limit_cpu(seconds: float) -> None:
    try:
        import resource
    except ImportError:  # not available on Windows
        return
    soft = int(seconds) + 30
    try:
        resource.setrlimit(resource.RLIMIT_CPU, (soft, soft + 30))
    except (ValueError, OSError):
        pass


def extract() -> int:
    emitter = Emitter(sys.stdout)
    emitter.emit("hello", protocolVersion=PROTOCOL_VERSION, workerVersion=__version__)
    try:
        request = read_request(sys.stdin)
        if request.get("protocolVersion") != PROTOCOL_VERSION:
            raise WorkerError("protocol_mismatch", "Versión de protocolo incompatible entre la aplicación y el extractor")
        limit_cpu(float((request.get("options") or {}).get("timeoutSeconds") or 600))
        from .extract import run_extract

        emitter.emit("result", payload=run_extract(request, emitter))
        return 0
    except WorkerError as error:
        emitter.emit("error", code=error.code, message=error.message, retryable=error.retryable)
        return 1
    except MemoryError:
        emitter.emit("error", code="out_of_memory", message="El documento excede la memoria disponible para el extractor", retryable=False)
        return 1
    except Exception as error:  # report, never crash silently
        traceback.print_exc(file=sys.stderr)
        emitter.emit("error", code="internal_error", message=f"Error interno del extractor: {error.__class__.__name__}: {str(error)[:300]}", retryable=True)
        return 1


def main() -> int:
    arguments = sys.argv[1:]
    if arguments == ["health"]:
        print(json.dumps({"service": "researchpilot-worker", "status": "ok", "workerVersion": __version__}))
        return 0
    if arguments and arguments[0] == "capabilities":
        grobid_url = arguments[arguments.index("--grobid-url") + 1] if "--grobid-url" in arguments[:-1] else ""
        print(json.dumps(capabilities(grobid_url, "--allow-remote" in arguments)))
        return 0
    if arguments == ["extract"]:
        return extract()

    print("Usage: researchpilot-worker health | capabilities [--grobid-url URL] | extract", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
