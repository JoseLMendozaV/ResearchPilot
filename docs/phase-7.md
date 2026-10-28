# Cierre de Fase 7 — Matrices y exportaciones

## Exportaciones por revisión (pestaña Exportar)

| Formato | Contenido |
|---|---|
| **Excel compatible** | Exactamente las hojas `Leer`, `Control`, `RQs`, `Cadena`, `I_E`, `Calidad`, `Extraccion` con las cabeceras del formato de evidencia: `Id`/`Reference`; `RQ` + `Aspecto n`; `Base de datos`/`Fecha`/`Cadena`/`Cantidad`; `Id`, `Reference`, `Title`, `Keywords`, `Abstract`, `Year`, `Authors`, `Conference/Journal`, una columna por criterio (Sí/No/?) e `I/E`; `Q1…Qn` y `Total` (consenso); `Extraccion` con fila de grupos (General y cada RQ real, sin etiquetas repetidas) sobre la fila de aspectos y solo estudios incluidos |
| **Excel ampliado** | Las 7 hojas (Control y Cadena con columnas extra) + `Extraccion_procedencia` (valor, estado, cita, página, autor, fecha, notas), `I_E_decisiones` (por revisor y finales), `Calidad_revisores` (puntajes y justificaciones), `Auditoria` y `Exportacion` (versión, protocolo, leyenda de estados) |
| **CSV** | Matriz de extracción, una fila por estudio incluido; UTF-8 con BOM |
| **JSON** | Instantánea versionada (`researchpilot-project-export/1`): proyecto, protocolo, control, RQs, cadenas, cribado de ambas fases, calidad, matriz y auditoría |
| **RIS / BibTeX** | Estudios incluidos o todo el corpus; tipo revista/conferencia según el venue; claves BibTeX estables y únicas; caracteres especiales escapados |
| **Markdown / LaTeX** | Informe con resumen, cadenas, incluidos, tabla por RQ y **anexo de evidencias** (valor, estado, cita literal y página); LaTeX compilable con `longtable` |

Reglas comunes: los valores rechazados no se exportan; los estados sin valor se escriben (“No reportado”, “No aplica”…); las celdas con varios valores se unen con «; ». La plantilla original no se redistribuye: el contenido de `Leer` es propio.

Cada exportación se registra en `export_records` con ruta, **SHA-256**, variante, alcance, versión de la app y del protocolo, y genera un evento de auditoría. El destino se elige con el diálogo nativo de guardado (permiso `dialog:allow-save`).

## Pruebas

```text
cargo test               → 52 passed (4 nuevas de exportación)
cargo clippy --all-targets → 0 avisos
npm run test             → 22 passed
python unittest          → 21 OK
```

- El Excel generado se **lee de vuelta** con `calamine` para verificar nombres y orden de hojas, cabeceras exactas, marcas por criterio, `I/E`, totales de calidad, fila de grupos de `Extraccion` y que solo aparezcan incluidos.
- La versión ampliada se verifica hoja por hoja (procedencia con cita y página) y su SHA-256 coincide con el registrado.
- El RIS exportado se **reimporta** en otra base y conserva título, DOI y año; BibTeX y LaTeX escapan `&`, `%` y `_`.
- Se rechazan formatos desconocidos y rutas relativas.

## Corrección incluida

Los criterios de inclusión/exclusión se ordenaban alfabéticamente por tipo (exclusión primero). Ahora el orden es inclusión y luego exclusión, cada grupo en el orden de definición, en la interfaz, la instantánea del protocolo y la exportación.

## Limitaciones

- No se generan DOCX ni PDF (el informe Markdown/LaTeX cubre ese uso sin depender de servicios externos).
- La importación de XLSX con mapeo visual de columnas sigue pendiente.
- El diagrama y los conteos PRISMA son la Fase 8.
