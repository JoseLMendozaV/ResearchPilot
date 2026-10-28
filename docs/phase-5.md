# Cierre de Fase 5 — AI Gateway opcional (OpenAI y Anthropic)

## Objetivos implementados

- **Gateway de proveedores** (`src-tauri/src/ai_gateway.rs`): interfaz común `ChatProvider` para Ollama (local), OpenAI y Anthropic. El asistente y las propuestas de la ficha usan el mismo contrato de respuesta con citas verificadas, sea cual sea el proveedor. Los embeddings siguen siendo siempre locales.
- **OpenAI** (Chat Completions oficial): salida estructurada `json_schema` estricta, `max_completion_tokens`, descubrimiento de modelos en `/v1/models` (filtrando modelos que no son de texto), errores explicados (clave inválida, cuenta sin crédito, límite de tasa, modelo no disponible).
- **Anthropic** (Messages API oficial, `anthropic-version: 2026-10-21`): `output_config.format` con esquema JSON, sin parámetros de muestreo (rechazados por esa versión), solo bloques de texto en la respuesta, `stop_reason` `refusal`/`max_tokens` gestionados, errores tipados (`authentication_error`, `billing_error`, `rate_limit_error`, `overloaded_error`). En los modelos que lo admiten se activa el respaldo del servidor ante rechazos (`fallbacks: "default"`); si la API lo rechazara para esa forma de petición, se reintenta una vez sin él.
- **Claves en el Llavero de macOS** (`secrets.rs`, crate `keyring` 3, backend nativo): nunca en SQLite, ajustes ni logs; la interfaz solo ve si existe una clave y sus últimos 4 caracteres.
- **Política de envío** comprobada en el backend, no solo en la interfaz:
  1. Modo solo local → ninguna conexión externa, ni siquiera pruebas de conexión o listados de modelos.
  2. Procesamiento externo permitido globalmente en Privacidad.
  3. **Consentimiento por solicitud o por lote** ligado a proveedor, modelo y coste máximo aceptado; se recalcula la estimación con el contexto real justo antes de enviar y se rechaza si la supera.
  4. Presupuesto mensual (opcional) aplicado con el peor caso; con presupuesto y sin precio configurado, no se envía.
  5. Los **artículos confidenciales** se excluyen de la recuperación para proveedores externos, y sus propuestas de IA externas se rechazan.
- **Estimación previa**: artículos incluidos, extractos, tokens de entrada, coste esperado y máximo (con reintentos), gasto del mes y extractos confidenciales excluidos, mostrado en un diálogo que hay que autorizar.
- **Costes**: tabla de precios por millón de tokens editable; se incluyen precios de referencia de Anthropic con fecha (2026-10-06) marcados como tales. OpenAI queda sin precios por defecto: sin precio no se puede estimar ni limitar, y la interfaz lo advierte.
- **Registro verificable**: cada llamada (asistente y cada campo propuesto) queda en `ai_runs` con proveedor, modelo, versión de prompt, tokens, coste, estado y marca de envío externo. Configuración → Privacidad muestra el total de envíos externos y el gasto del mes ("Ningún contenido ha salido de este equipo" cuando no hay ninguno).
- **Acceso manual a ChatGPT/Claude web**: en "Buscar fragmentos" se marcan extractos y se copian al portapapeles con sus referencias; botones para abrir los sitios. No hay automatización de sesiones ni uso de cookies.

## Migración

- `0007_ai_gateway.sql`: modelos por proveedor, presupuesto y límite de salida en `app_settings`; `articles.confidential`; `ai_runs.cost_usd` y `estimated_cost_usd`; tabla `ai_model_pricing` con precios de referencia de Anthropic.

## Pruebas

```text
cargo test               → 41 passed (4 nuevas de gateway sobre servidores OpenAI/Anthropic simulados)
cargo clippy --all-targets → 0 avisos
npm run test             → 15 passed
```

Las pruebas verifican las cabeceras y el cuerpo exactos enviados (clave, versión, `output_config`, ausencia de `temperature`, `strict`), el bloqueo por modo local, consentimiento ausente o de otro modelo, clave ausente, presupuesto y coste máximo; la exclusión de confidenciales; el coste registrado a partir del uso; los mensajes de error de crédito y de rechazo; y el filtrado de modelos.

**No se probó contra las API reales** (no hay claves en este equipo): las pruebas usan servidores simulados con el formato documentado.

## Limitaciones

- Las solicitudes externas no usan streaming: cancelar una pregunta externa surte efecto antes de enviarla o entre campos, no a mitad de una respuesta.
- Los precios no se actualizan solos; los de referencia pueden quedar desfasados.
- No hay todavía una política de IA externa por proyecto; la protección por documento es la marca de confidencialidad.
