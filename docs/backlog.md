# Backlog de implementación

Cada fase exige compilación, pruebas pertinentes, regresión de fases anteriores y documentación de límites reales.

| Fase | Dependencias | Criterios de aceptación medibles |
|---|---|---|
| 0 — Arquitectura | Prompt Maestro y Excel | Arquitectura, ERD, licencias, riesgos y backlog documentados |
| 1 — Aplicación base | Rust, Node, Xcode CLI | `.app` y DMG ARM64; SQLite migra; navegación, revisión y configuración persisten; pruebas verdes |
| 2 — Biblioteca y visor ✅ | PDF.js, parser RIS/BibTeX/CSV, fixtures | Cumplido: importación individual/lote; SHA-256; candidatos duplicados; colecciones; FTS5; página navegable; PDF inválido aislado |
| 3 — Extracción | Sidecar empaquetado, parser PDF, GROBID/OCR opcionales | JSON tipado; timeout/cancelación/progreso; texto por página; ficha editable; cada propuesta enlaza evidencia; reinicio recupera trabajos |
| 4 — Ollama y RAG | Ollama detectado, interfaz vectorial | Descubrimiento de modelos; LLM/embedding separados; FTS5+vector; respuestas con citas navegables; “no encontrado” sin invención |
| 5 — AI Gateway | Keychain, SDK oficiales | OpenAI/Anthropic opt-in por tarea; prueba de conexión; confirmación de envío; tokens/coste registrados; modo sin red verificable |
| 6 — Metodología SLR | Biblioteca estable | CRUD completo de Leer/Control/RQs/Cadena/I_E/Calidad/Extraccion; protocolo versionado; doble revisión y conflictos; auditoría |
| 7 — Matrices/exportación | Datos validados Fase 6 | XLSX de siete hojas compatible; versión ampliada; CSV/JSON/RIS/BibTeX/Markdown/LaTeX; celdas enlazan procedencia |
| 8 — Síntesis y PRISMA | Eventos SLR consistentes | Conteos derivados y validados; diagrama exportable; tablas descriptivas; borradores solo con datos verificados y citas |
| 9 — Intercambio/expansión | Formato portable y backups | Paquetes entre revisores, recuperación probada, Zotero futuro, rendimiento con corpus grande y preparación Win/Linux |

## Riesgos prioritarios

1. **PDF heterogéneo:** columnas, escaneos y tablas requieren resultados parciales explícitos y revisión humana.
2. **Licencias:** evitar motores AGPL incorporados; revisar modelos, OCR y artefactos antes de empaquetar.
3. **Consumo de RAM:** perfiles distintos para M1 Max 64 GB y M4 16 GB; no descargar ni ejecutar modelos automáticamente.
4. **Prompt injection documental:** el contenido recuperado se trata como datos no confiables, separado de instrucciones.
5. **Cambios de esquema:** migraciones incrementales con fixtures de actualización y respaldo previo.
6. **Falsos duplicados:** nunca fusionar automáticamente sin historial y reversión.
7. **Métricas incompatibles:** síntesis debe conservar dataset, configuración, unidad y varianza antes de comparar.
