#!/usr/bin/env bash
# Build a signed release and prepare the GitHub release assets.
#
#   scripts/release.sh NOTES_FILE            # build and prepare dist-release/vX.Y.Z
#   scripts/release.sh NOTES_FILE --publish  # also create the GitHub release (gh CLI)
#
# The updater private key stays in ~/.tauri and its password in the macOS
# Keychain (service "researchpilot-updater-key-password"); neither is printed
# nor stored in the repository.
set -euo pipefail

NOTES_FILE="${1:?Uso: scripts/release.sh NOTAS.md [--publish]}"
PUBLISH="${2:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="JoseLMendozaV/ResearchPilot"
KEY_FILE="${RESEARCHPILOT_UPDATER_KEY:-$HOME/.tauri/researchpilot-updater.key}"

test -f "$NOTES_FILE" || { echo "No existe $NOTES_FILE" >&2; exit 1; }
test -f "$KEY_FILE" || { echo "Falta la clave privada del actualizador en $KEY_FILE" >&2; exit 1; }

VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$ROOT/src-tauri/tauri.conf.json")"
TAG="v$VERSION"
OUT="$ROOT/dist-release/$TAG"

export TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY_FILE")"
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s researchpilot-updater-key-password -w)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD

(cd "$ROOT" && npm run tauri build)
unset TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD

BUNDLE="$ROOT/src-tauri/target/release/bundle"
DMG="$BUNDLE/dmg/ResearchPilot_${VERSION}_aarch64.dmg"
ARCHIVE="$BUNDLE/macos/ResearchPilot.app.tar.gz"
test -f "$DMG" && test -f "$ARCHIVE" && test -f "$ARCHIVE.sig" || { echo "Faltan artefactos del build (¿createUpdaterArtifacts?)" >&2; exit 1; }

# Without a valid bundle signature macOS reports downloaded copies as "damaged".
verify_app() {
  codesign --verify --deep --strict "$1" || { echo "Firma de código inválida en $1" >&2; exit 1; }
}
verify_app "$BUNDLE/macos/ResearchPilot.app"
MOUNT="$(mktemp -d)"
hdiutil attach -nobrowse -readonly -mountpoint "$MOUNT" "$DMG" >/dev/null
trap 'hdiutil detach "$MOUNT" >/dev/null 2>&1 || true' EXIT
verify_app "$MOUNT/ResearchPilot.app"
hdiutil detach "$MOUNT" >/dev/null
trap - EXIT

rm -rf "$OUT" && mkdir -p "$OUT"
cp "$DMG" "$OUT/"
cp "$ARCHIVE" "$OUT/ResearchPilot_${VERSION}_aarch64.app.tar.gz"
cp "$ARCHIVE.sig" "$OUT/ResearchPilot_${VERSION}_aarch64.app.tar.gz.sig"

python3 - "$OUT" "$VERSION" "$TAG" "$REPO" "$NOTES_FILE" <<'PY'
import datetime, json, pathlib, sys
out, version, tag, repo, notes_file = sys.argv[1:]
archive = f"ResearchPilot_{version}_aarch64.app.tar.gz"
manifest = {
    "version": version,
    "notes": pathlib.Path(notes_file).read_text(encoding="utf-8").strip(),
    "pub_date": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    "platforms": {
        "darwin-aarch64": {
            "signature": (pathlib.Path(out) / f"{archive}.sig").read_text().strip(),
            "url": f"https://github.com/{repo}/releases/download/{tag}/{archive}",
        }
    },
}
(pathlib.Path(out) / "latest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

(cd "$OUT" && shasum -a 256 ./* > SHA256SUMS.txt)
echo "Artefactos listos en $OUT"
cat "$OUT/SHA256SUMS.txt"

if [ "$PUBLISH" = "--publish" ]; then
  gh release create "$TAG" "$OUT"/* --repo "$REPO" --title "ResearchPilot $TAG" --notes-file "$NOTES_FILE" --target main
fi
