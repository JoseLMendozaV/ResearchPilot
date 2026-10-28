"""JSON-lines protocol shared with the Rust job runner.

The worker reads one JSON request from stdin and writes one JSON object per
line to stdout. Every line has a ``type`` field: ``hello``, ``progress``,
``warning``, ``result`` or ``error``. Exactly one ``result`` or ``error`` line
ends a run. Nothing else is written to stdout; diagnostics go to stderr.
"""

from __future__ import annotations

import json
from typing import IO, Any, Optional


class WorkerError(Exception):
    """An expected failure that is reported to Rust with a stable code."""

    def __init__(self, code: str, message: str, retryable: bool = False) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.retryable = retryable


class Emitter:
    def __init__(self, stream: IO[str]) -> None:
        self._stream = stream
        self.warnings: list[dict[str, Any]] = []

    def emit(self, event_type: str, **fields: Any) -> None:
        payload = {"type": event_type, **fields}
        self._stream.write(json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n")
        self._stream.flush()

    def progress(self, stage: str, current: int, total: int, message: str) -> None:
        self.emit("progress", stage=stage, current=current, total=max(total, 1), message=message)

    def warning(self, code: str, message: str, page: Optional[int] = None) -> None:
        warning: dict[str, Any] = {"code": code, "message": message}
        if page is not None:
            warning["page"] = page
        self.warnings.append(warning)
        self.emit("warning", **warning)


def read_request(stream: IO[str], max_bytes: int = 512 * 1024 * 1024) -> dict[str, Any]:
    raw = stream.read(max_bytes + 1)
    if len(raw) > max_bytes:
        raise WorkerError("request_too_large", "La solicitud supera el tamaño permitido")
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as error:
        raise WorkerError("invalid_request", f"Solicitud JSON inválida: {error}") from error
    if not isinstance(request, dict):
        raise WorkerError("invalid_request", "La solicitud debe ser un objeto JSON")
    return request
