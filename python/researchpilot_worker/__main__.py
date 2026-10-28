import json
from sys import argv


def main() -> int:
    """Expose a dependency-free health command until extraction arrives in Phase 2."""
    if len(argv) == 2 and argv[1] == "health":
        print(json.dumps({"service": "researchpilot-worker", "status": "ok"}))
        return 0

    print("Usage: researchpilot-worker health")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
