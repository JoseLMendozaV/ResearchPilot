# Cierre de Fase 4 — Ollama y RAG local

## Objetivos implementados

- **Cliente Ollama local** (`src-tauri/src/ollama.rs`, `ureq`): solo URLs de loopback salvo autorización explícita de procesamiento externo (validado al guardar y al conectar); variables de proxy ignoradas; errores legibles ("Ollama no responde", "modelo no descargado").
- **Descubrimiento de modelos**: `/api/tags` + `/api/show`; tipo (lenguaje/embeddings) por capacidades declaradas, longitud de contexto y encaje estimado en la RAM del equipo (`sysctl hw.memsize`). Ningún nombre de modelo está fijado en el código.
- **Modelos por tarea**: asistente, ficha (opcional, si no usa el del asistente) y embeddings, configurables por separado; el asistente permite cambiar de modelo por pregunta.
- **Descarga de modelos** como tarea de la cola (`model_pull`) con progreso, cancelación y confirmación previa en la interfaz.
- **Índice híbrido**:
  - Fragmentos de ~1200 caracteres que nunca cruzan página ni sección, construidos desde los bloques de la Fase 3 (o del texto por página si no hubo extracción con diseño); excluyen referencias y cabeceras/pies; conservan las cajas de los bloques de origen.
  - FTS5 (`unicode61 remove_diacritics 2`) sobre fragmentos.
  - Vectores normalizados en SQLite (`chunk_embeddings`, f32), indexados por hash de contenido + modelo: re-fragmentar no recalcula embeddings de textos idénticos.
  - Búsqueda vectorial detrás de la interfaz `VectorIndex` (implementación exacta por fuerza bruta, sustituible por sqlite-vec/HNSW).
  - Fusión por rangos recíprocos (RRF, k = 60); modos híbrido, léxico o semántico; filtros por biblioteca, colección, artículo y rango de años.
  - Indexación como tarea (`rag_index`), encadenada automáticamente tras cada extracción y relanzable desde Configuración; sin Ollama queda el índice léxico y se informa.
- **Asistente con citas verificables** (`rag.rs`, prompt `rag-answer-v1`):
  - Contexto con presupuesto de tokens; `num_ctx` ajustado para que Ollama no trunque en silencio.
  - Instrucciones solo en el mensaje de sistema; los fragmentos van como datos delimitados y marcados como no confiables; los delimitadores dentro del texto del PDF se neutralizan (prompt injection).
  - Salida estructurada con esquema JSON (`format`), hasta 3 intentos ante JSON inválido o citas no verificables.
  - Cada cita debe ser un extracto literal (≥ 10 caracteres, normalizando espacios/comillas) del fragmento citado; las demás se descartan y se informa cuántas.
  - `found=false` → "No encontré evidencia…"; respuesta sin ninguna cita verificable → descartada con mensaje explícito, nunca mostrada como evidencia.
  - Cada consulta queda en `ai_runs` (modelo, versión de prompt, hashes, tokens, duración, intentos, estado) y la conversación en SQLite.
  - Cancelación de una consulta en curso (streaming con comprobación entre fragmentos).
  - Las citas abren el PDF en la página y resaltan el bloque citado.
- **Propuestas de IA para la ficha** (`ai_fields.rs`, prompt `ai-fields-v1`): 32 campos narrativos/bibliográficos que las reglas no cubren; una consulta por campo sobre los fragmentos del propio artículo; solo se guardan valores con cita literal verificada (origen `ai`, estado `proposed`), "no encontrado" se guarda como `not_found` (nunca `not_reported`), los descartes y sus motivos quedan en la ejecución. Mismo flujo de revisión, historial y auditoría que la Fase 3; no se vuelve a preguntar por campos ya resueltos.
- **Interfaz**: página **Asistente** (conversaciones, ámbito, años, modelo, cancelar, modo "Buscar fragmentos" sin LLM); sección de IA en Configuración (estado, modelos, descarga, parámetros RAG, índice); botones "Proponer campos con IA local" y "Preguntar a este artículo" en la ficha.

## Migración

- `0006_local_rag.sql`: ajustes de IA/RAG en `app_settings`; `document_chunks` + `document_chunks_fts` con disparadores; `chunk_embeddings`; `assistant_conversations` y `assistant_messages`; columnas de estado, tokens, duración e intentos en `ai_runs`. Elimina `embedding_indexes`, que nunca tuvo datos y queda sustituida.

## Pruebas ejecutadas (28 oct 2026, Apple Silicon)

```text
cd src-tauri && cargo test               → 37 passed; 0 failed
cd src-tauri && cargo clippy --all-targets → 0 avisos
npm run test                              → 12 passed (5 archivos)
npm run build                             → OK
python (.venv): unittest                  → 21 tests OK (sin cambios en el worker)
```

**Ollama no está instalado en el equipo de desarrollo**, así que las pruebas usan un servidor Ollama simulado (TCP local) que implementa `/api/version`, `/api/tags`, `/api/show`, `/api/embed`, `/api/chat` en streaming y `/api/pull` con el formato real, con embeddings deterministas por bolsa de palabras. No se ha verificado todavía con modelos reales; la calidad de respuestas y propuestas dependerá del modelo elegido.

Casos cubiertos: indexación incremental con reutilización de embeddings, búsqueda léxica/semántica/híbrida, filtros de ámbito y año, aviso cuando falta la parte semántica, citas literales aceptadas y alteradas rechazadas, respuesta sin citas válidas descartada tras 3 intentos, "no encontrado", JSON inválido reintentado, cancelación registrada, separación instrucciones/datos con texto de inyección en el PDF, esquema y `num_ctx` enviados, registro en `ai_runs`, propuestas de IA con cita verificada / descartadas / no encontradas, campo numérico normalizado, no repetir campos resueltos, descubrimiento de modelos y tipo, descarga como tarea, Ollama remoto rechazado, encadenamiento extracción → indexación.

## Cómo comprobar con Ollama real

```bash
# Instala Ollama desde https://ollama.com y ábrelo
npm run tauri dev
```

1. Configuración → Inteligencia artificial → **Comprobar**: aparecen versión, modelos y su encaje en memoria.
2. Si no hay modelos, escribe un nombre de la biblioteca de Ollama y **Descargar** (requiere internet una vez). Necesitas un modelo de lenguaje y, para búsqueda semántica, uno de embeddings.
3. Elige los modelos, **Guardar cambios** y **Indexar biblioteca**.
4. Asistente → pregunta algo sobre tus artículos; abre una cita y verifica la región resaltada.
5. En la ficha de un artículo, **Proponer campos con IA local**; revisa y confirma las propuestas.

## Checklist de regresión

- [x] Pruebas de Fases 1–3 en verde (Rust, Vitest, Python).
- [x] Una nueva extracción con reglas ya no reemplaza propuestas de IA no revisadas, y viceversa.
- [x] Sin Ollama: biblioteca, extracción, ficha, búsqueda léxica y tareas funcionan; la IA informa que no está disponible.
- [x] Modo solo local: Ollama y GROBID remotos rechazados.

## Limitaciones y riesgos

- No hay reordenamiento con modelo cruzado (cross-encoder); el "re-ranking configurable" es la elección de modo y la fusión RRF.
- La búsqueda vectorial es exacta y lineal; con decenas de miles de fragmentos convendrá sqlite-vec o HNSW (la interfaz ya lo permite).
- Cada pregunta del asistente recupera contexto de forma independiente; no se reenvía el historial de la conversación al modelo.
- La cola procesa una tarea a la vez: una propuesta de IA larga retrasa otras extracciones.
- La cancelación de una propuesta de IA ocurre entre campos o entre fragmentos de la respuesta en streaming.
- El encaje en memoria es una estimación (tamaño del modelo × 1,3 frente a la RAM total).
- OpenAI/Anthropic siguen bloqueados hasta la Fase 5.

## Siguiente fase

Fase 5 — AI Gateway opcional: OpenAI y Anthropic con claves en el Llavero de macOS, autorización explícita por tarea, estimación de coste, registro de tokens/coste y modo sin red verificable, reutilizando el mismo contrato de respuestas con citas verificadas.
