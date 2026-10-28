# Guía de uso de ResearchPilot

ResearchPilot organiza una revisión sistemática, un mapeo sistemático o una revisión de alcance en tu equipo, conservando la evidencia (cita literal, página y documento) de cada dato y el historial de cada decisión.

## 1. Primeros pasos

1. **Configuración → Perfil local**: escribe tu nombre; aparecerá como autor de tus decisiones.
2. **Configuración → Extracción científica → Comprobar**. Si falta `pdfplumber`, usa «Preparar entorno del extractor» (descarga una vez). Para PDF escaneados instala Tesseract (`brew install tesseract tesseract-lang`).
3. **Inteligencia artificial (opcional)**: instala Ollama, descarga un modelo de lenguaje y uno de embeddings, elígelos y pulsa «Indexar biblioteca». Sin IA, todo lo demás funciona.
4. **Privacidad**: el modo solo local impide cualquier envío fuera del equipo. Los proveedores externos (OpenAI, Anthropic) son opcionales y cada envío requiere tu autorización con coste estimado.

## 2. Biblioteca

- **Importar PDF** (uno o varios) o **RIS/BibTeX/CSV**. Los duplicados por huella SHA-256 o DOI se detectan sin fusionar nada automáticamente.
- **Importar desde Zotero** (Configuración → Respaldo e intercambio): lee una copia de tu base de Zotero, trae metadatos, PDF adjuntos y colecciones.
- Al importar un PDF se encola su **extracción científica** (ver **Tareas**). En la ficha del artículo:
  - *Ficha científica*: propuestas con cita y página; «Ver en página N» resalta la evidencia. Confirma, corrige, marca «No reportado» o «No aplica»; todo queda en el historial.
  - *Estructura*: secciones, tablas, figuras y referencias.
  - *Metadatos*: edición auditada; marca el artículo como **confidencial** si nunca debe enviarse a proveedores externos.
- **Asistente**: preguntas sobre tus artículos; solo se muestran respuestas con citas que coinciden literalmente con el texto.

## 3. Revisión (Revisiones → tu revisión)

| Pestaña | Para qué |
|---|---|
| Resumen | Tipo, estado, revisión simple o doble, umbral de calidad, revisores, añadir artículos de la biblioteca |
| Leer | Guía metodológica |
| Protocolo | Redactar, aprobar (congela RQs, criterios, calidad y cadenas) y registrar enmiendas |
| Control | Artículos clave y si las cadenas los recuperan |
| RQs | Preguntas, subpreguntas y aspectos tipados (taxonomías, unidades, vínculo con la ficha) |
| Cadena | Cadenas por base de datos, versiones e importación de resultados exportados |
| I/E | Cribado por metadatos y por texto completo con criterios y motivos |
| Calidad | Preguntas Q1…Qn con escala y pesos; consenso en revisión doble |
| Extracción | Matriz por RQ con citas y páginas; prellenado desde la ficha confirmada |
| Síntesis y PRISMA | Diagrama PRISMA calculado, síntesis descriptiva y borradores trazables |
| Conflictos | Desacuerdos entre revisores y su resolución |
| Exportar | Excel compatible (7 hojas) o ampliado, CSV, JSON, RIS, BibTeX, Markdown, LaTeX, PRISMA SVG |
| Auditoría | Diario de todos los cambios |

### Revisión doble entre dos equipos

1. Quien crea la revisión configura criterios, RQs y calidad, e importa las cadenas.
2. En **Resumen → Paquete de intercambio**, exporta un paquete y envíalo al segundo revisor (correo, carpeta compartida…).
3. El segundo revisor lo importa: obtiene la misma revisión (mismos Id de estudios y criterios), revisa de forma independiente y exporta su propio paquete.
4. Al importar el paquete del colega, sus decisiones aparecen a su nombre; los acuerdos se finalizan y los desacuerdos abren conflictos para resolver.

Importar el mismo paquete dos veces no duplica nada.

## 4. Respaldo y recuperación

- **Configuración → Respaldo e intercambio → Crear respaldo**: un archivo `.zip` con la base de datos y los PDF administrados, y un manifiesto con la huella de cada archivo. Las claves de API del Llavero no se incluyen.
- **Restaurar**: se verifica el respaldo, se crea automáticamente un respaldo del estado actual y la restauración se aplica al reiniciar la app.
- **Diagnóstico**: comprueba la integridad de la base de datos.

## 5. Actualizaciones

Configuración → Actualizaciones → «Buscar actualizaciones». Solo se instalan versiones firmadas por el proyecto; tus datos no se modifican.

## Principios

- La IA propone; las personas deciden. Nada se marca como confirmado, incluido o resuelto sin una acción humana registrada.
- «No encontrado» (el sistema no lo halló) es distinto de «No reportado» (una persona confirmó que el estudio no lo informa).
- Cada número exportado proviene de datos registrados y cada archivo exportado queda con su huella SHA-256.
