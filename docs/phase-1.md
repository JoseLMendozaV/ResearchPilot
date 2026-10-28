# Cierre de Fase 1

## Objetivos alcanzados

- Proyecto Tauri 2 independiente con React 19, TypeScript, Vite y Tailwind CSS.
- Aplicación nativa de macOS con navegación funcional y diseño profesional adaptable al espacio de escritorio.
- SQLite local inicializada por Rust y migración transaccional versionada.
- Modelo normalizado para las siete hojas del formato metodológico.
- Búsqueda FTS5 preparada sobre título, resumen, autores y palabras clave.
- Creación real de revisiones, con validación y evento de auditoría atómico.
- Configuración persistente de identidad, idioma, proveedor y política de privacidad.
- Valores seguros por defecto: Ollama, modo local y procesamiento externo bloqueado.
- Worker Python empaquetable con contrato de salud comprobable.
- CSP y frontera IPC configuradas; la interfaz no tiene acceso SQL directo.
- Tema claro/oscuro/sistema aplicado y catálogo i18n tipado español–inglés para la navegación.
- Migración incremental de alineación con el Prompt Maestro, sin eliminar la base existente.

## Verificación real

Ejecutada el 28 de octubre de 2026 en macOS Apple Silicon:

| Comprobación | Resultado |
|---|---|
| Vitest | 1 prueba aprobada |
| TypeScript + Vite | compilación aprobada |
| Rust/SQLite | 4 pruebas aprobadas |
| Worker Python | 1 prueba aprobada |
| Paquete Tauri debug | `.app` y DMG `aarch64` generados |
| Arranque nativo | proceso iniciado y SQLite creada correctamente |

Artefactos verificados:

- `src-tauri/target/debug/bundle/macos/ResearchPilot.app`
- `src-tauri/target/debug/bundle/dmg/ResearchPilot_0.1.0_aarch64.dmg`

## Limitaciones conscientes

- Las vistas de Biblioteca y Evidencia explican el estado, pero todavía no importan PDFs ni capturan fragmentos.
- No hay editores interactivos para RQs, cadenas, criterios, calidad o extracción; sus entidades sí están migradas.
- PDF.js, GROBID, Ollama, embeddings, RAG, PRISMA e importación/exportación quedan para fases posteriores.
- Las claves de OpenAI/Anthropic no se solicitan ni almacenan todavía; deberán guardarse en Keychain.
- El icono actual proviene del scaffold y deberá sustituirse por identidad visual propia antes de distribución.
- La traducción inglesa completa de las pantallas de contenido queda pendiente; la navegación y estructura i18n ya funcionan.

## Próxima fase recomendada

1. Construir importación local de PDF y metadatos con hash SHA-256 y deduplicación.
2. Integrar lector PDF.js y captura de evidencia por página.
3. Implementar colecciones, etiquetas y búsqueda FTS5 en la Biblioteca.
4. Añadir pruebas con fixtures PDF licenciados y copias de seguridad de SQLite.
