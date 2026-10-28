# ResearchPilot

ResearchPilot es una aplicación de escritorio local-first para administrar evidencia científica y construir revisiones sistemáticas, mapeos sistemáticos y revisiones de alcance con trazabilidad documental.

ResearchPilot dispone de una aplicación Tauri 2 real para macOS Apple Silicon con React, TypeScript, Tailwind CSS, Rust y SQLite. Las Fases 1 y 2 incluyen navegación, revisiones, configuración, biblioteca local, importación PDF/RIS/BibTeX/CSV, búsqueda FTS5, colecciones, visor PDF.js y metadatos auditables.

## Arquitectura

- `src/`: interfaz React tipada, navegación y acceso a comandos Tauri.
- `src-tauri/`: núcleo Rust, validación, comandos, migraciones y SQLite.
- `python/`: proceso auxiliar local para procesamiento científico futuro; en Fase 1 expone un chequeo de salud sin dependencias.
- `docs/`: decisiones de arquitectura, análisis metodológico y cierre de fase.

La UI no ejecuta SQL directamente. Toda mutación persistente atraviesa comandos Rust, que validan la entrada y escriben eventos de auditoría cuando corresponde.

## Requisitos en macOS

- macOS 10.15 o posterior.
- Xcode Command Line Tools: `xcode-select --install`.
- Node.js LTS y npm.
- Rust estable mediante `rustup`.
- Python 3.11 o posterior para el worker auxiliar.

## Ejecutar en desarrollo

```bash
npm install
npm run tauri dev
```

La base se crea automáticamente en:

```text
~/Library/Application Support/com.researchpilot.desktop/researchpilot.sqlite3
```

## Verificar

```bash
npm run test
npm run build
(cd src-tauri && cargo test)
(cd python && PYTHONPATH=. python3 -m unittest discover -s tests -p 'test_*.py')
```

## Construir la aplicación

```bash
npm run tauri build
```

Tauri genera `ResearchPilot.app` y un DMG en `src-tauri/target/release/bundle/`. Para una compilación de depuración más rápida:

```bash
npm run tauri build -- --debug
```

Documentación de referencia:

- [Arquitectura](docs/architecture.md)
- [Modelo de datos y ERD](docs/data-model.md)
- [Alineación con el Prompt Maestro](docs/master-spec-alignment.md)
- [Backlog por fases](docs/backlog.md)
- [Revisión de licencias](docs/license-review.md)
- [Cierre de Fase 1](docs/phase-1.md)
- [Cierre de Fase 2](docs/phase-2.md)
