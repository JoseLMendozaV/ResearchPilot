# ResearchPilot

ResearchPilot es una aplicación de escritorio local-first para administrar evidencia científica y construir revisiones sistemáticas, mapeos sistemáticos y revisiones de alcance con trazabilidad documental.

ResearchPilot dispone de una aplicación Tauri 2 real para macOS Apple Silicon con React, TypeScript, Tailwind CSS, Rust y SQLite. Las Fases 1 y 2 incluyen navegación, revisiones, configuración, biblioteca local, importación PDF/RIS/BibTeX/CSV, búsqueda FTS5, colecciones, visor PDF.js y metadatos auditables. La Fase 3 añade extracción científica local en cola persistente: diseño por página con coordenadas, OCR y GROBID opcionales, y una ficha maestra cuyos valores son propuestas con cita literal, página y caja en el PDF, revisables y con historial. La Fase 4 integra Ollama local: índice híbrido (FTS5 + vectorial), asistente que solo muestra respuestas con citas verificadas literalmente y propuestas de IA para los campos narrativos de la ficha. La Fase 5 añade OpenAI y Anthropic opcionales (claves en el Llavero, consentimiento con coste por envío, presupuesto y registro verificable). La Fase 6 convierte las siete hojas del formato de evidencia en módulos por revisión: protocolo versionado, control, RQs, cadenas, cribado I/E con doble revisión, calidad y matriz de extracción. La Fase 7 exporta el libro de Excel compatible con las siete hojas (y una versión ampliada con procedencia), CSV, JSON, RIS, BibTeX, Markdown y LaTeX, registrando cada archivo con su SHA-256. La Fase 8 calcula el diagrama PRISMA 2020 desde los eventos registrados (con verificación de la contabilidad), la síntesis descriptiva con valores confirmados y borradores trazables del informe.

## Arquitectura

- `src/`: interfaz React tipada, navegación y acceso a comandos Tauri.
- `src-tauri/`: núcleo Rust, validación, comandos, migraciones y SQLite.
- `python/`: extractor científico local (sidecar) invocado por Rust mediante JSON por líneas; sin servidor ni red salvo GROBID local opcional.
- `docs/`: decisiones de arquitectura, análisis metodológico y cierre de fase.

La UI no ejecuta SQL directamente. Toda mutación persistente atraviesa comandos Rust, que validan la entrada y escriben eventos de auditoría cuando corresponde.

## Requisitos en macOS

- macOS 10.15 o posterior.
- Xcode Command Line Tools: `xcode-select --install`.
- Node.js LTS y npm.
- Rust estable mediante `rustup`.
- Python 3.9 o posterior para el extractor (recomendado Homebrew: `brew install python`).
- Opcional: `brew install tesseract tesseract-lang` para OCR de páginas escaneadas.
- Opcional: [Ollama](https://ollama.com) con un modelo de lenguaje y uno de embeddings para el asistente y las propuestas de IA. Sin Ollama, todo lo demás funciona.

## Ejecutar en desarrollo

```bash
npm install
# Extractor científico (una vez): entorno de desarrollo con pdfplumber
(cd python && python3 -m venv .venv && .venv/bin/pip install -r requirements.txt)
npm run tauri dev
```

En compilaciones de depuración el extractor se ejecuta desde `python/` con `python/.venv`. La app empaquetada incluye el código del extractor y ofrece en Configuración «Preparar entorno del extractor», que descarga `pdfplumber` una sola vez tras confirmación.

La base se crea automáticamente en:

```text
~/Library/Application Support/com.researchpilot.desktop/researchpilot.sqlite3
```

## Verificar

```bash
npm run test
npm run build
(cd src-tauri && cargo test)
(cd python && PYTHONPATH=. .venv/bin/python -B -m unittest discover -s tests -p 'test_*.py')
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
- [Cierre de Fase 3](docs/phase-3.md)
- [Cierre de Fase 4](docs/phase-4.md)
- [Cierre de Fase 5](docs/phase-5.md)
- [Cierre de Fase 6](docs/phase-6.md)
- [Cierre de Fase 7](docs/phase-7.md)
- [Cierre de Fase 8](docs/phase-8.md)
- [Publicar versiones y actualizaciones](docs/releasing.md)
