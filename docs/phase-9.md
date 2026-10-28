# Cierre de Fase 9 — Colaboración diferida y expansión

## Paquete de intercambio entre revisores (revisión doble sin servidor)

- **Revisiones → Importar paquete de intercambio** y, dentro de una revisión, **Resumen → Paquete de intercambio**.
- El paquete (`researchpilot-exchange/1`, JSON) contiene:
  - la configuración del proyecto: protocolo y enmiendas, RQs, criterios I/E, preguntas de calidad, aspectos de extracción, cadenas y artículos de control;
  - los metadatos del corpus;
  - todas las decisiones humanas de cribado, los puntajes de calidad y los valores extraídos con su evidencia (cita y página).
- No incluye PDF ni rutas locales.
- Se conservan los identificadores, así que ambos equipos ven los mismos Id de estudio y los mismos criterios.
- Emparejamiento de artículos: por Id, luego DOI, luego huella del PDF y por último título y año. Los que no existen se crean como registros sin PDF, y el PDF que se importe después se **adjunta** a ese registro en lugar de duplicarlo.
- Al importar se añaden solo las filas nuevas (`INSERT OR IGNORE` por Id) y se recalcula todo:
  - **Cribado:** una resolución registrada en cualquiera de los dos equipos prevalece. Si no la hay, la revisión doble concilia (el acuerdo finaliza y el desacuerdo abre un conflicto) y la revisión simple conserva la decisión más reciente.
  - **Calidad:** un consenso registrado en el otro equipo cierra el conflicto; si no lo hay, se concilia como en local.
  - **Extracción:** los valores de ambos revisores conviven en la celda. Si difieren, se abre un **conflicto de extracción**, que se resuelve eligiendo el valor que se conserva; los demás quedan como rechazados, no se borran.
- **Idempotencia:** el SHA-256 de cada paquete aplicado se registra en `exchange_imports`, y reimportarlo no cambia nada.
- **Seguridad:** las columnas del paquete solo llegan al SQL si existen en el esquema local, y los paquetes de un esquema más nuevo se rechazan.
- **Validación de revisores:** se exige el nombre en «Perfil local». Se rechaza un paquete de otra persona con el mismo nombre, porque las decisiones serían indistinguibles.

## Respaldo y recuperación (Configuración → Respaldo y recuperación)

- **Crear respaldo:** un zip con una copia coherente de la base (`VACUUM INTO`), los PDF administrados y un `manifest.json` con el SHA-256 y el tamaño de cada archivo, la versión de la app y la del esquema. Las claves del Llavero no se copian.
- **Restaurar:**
  1. Se verifican todas las huellas y se rechazan las rutas sospechosas (`..`), las entradas no declaradas y los esquemas más nuevos.
  2. Se desempaqueta en `restore-pending/` y se comprueba `PRAGMA integrity_check`.
  3. Se crea automáticamente un **respaldo del estado actual** (`backups/pre-restore-*.zip`).
  4. Al reiniciar, antes de abrir la base, se intercambian los archivos. Si algo falla se devuelven los datos anteriores.
  5. Las rutas de los PDF se reajustan a la carpeta de datos del equipo, de modo que se puede restaurar en otro Mac.
- **Diagnóstico:** integridad de la base, claves foráneas, versión de esquema, tamaños y PDF que faltan.
- Al abrir la app tras una restauración aparece un aviso con la fecha del respaldo y la ubicación de la copia de seguridad previa.

## Importación desde Zotero (Configuración → Importar desde Zotero)

- Lee una **copia** de `zotero.sqlite`, por lo que Zotero puede estar abierto y su base no se toca. Detecta `~/Zotero` automáticamente.
- Importa los tipos bibliográficos con título, fecha (año), DOI, resumen, revista o congreso, autores en orden y etiquetas (como palabras clave).
- Omite notas, anotaciones, adjuntos sueltos y la papelera.
- Los **PDF adjuntos** (`storage:` y archivos vinculados con ruta absoluta) pasan por el importador normal: se validan, se calcula su huella y se encola la extracción si está activada.
- Las **colecciones** se recrean con su jerarquía («Tesis / Capítulo 2»).
- Cada elemento queda vinculado por su clave (`external_links`), así que reimportar solo añade lo nuevo. Un DOI ya presente enlaza con el artículo existente sin duplicarlo.

## Rendimiento

Prueba de rendimiento ignorada por defecto:

```bash
cargo test --release --lib perf -- --ignored --nocapture
```

Resultados con 1500 estudios en Apple Silicon:

```text
insertar artículos                             87 ms
añadir al corpus (Id secuencial)               16 ms
1500 decisiones de cribado                   3760 ms   (antes 76 598 ms)
tabla de cribado                               52 ms
búsqueda en biblioteca (FTS)                    2 ms
conteos PRISMA                                  9 ms
matriz de extracción                           21 ms
síntesis descriptiva                           54 ms
exportar Excel ampliado                       142 ms
respaldo completo                              36 ms
exportar paquete de intercambio                13 ms
```

La prueba detectó un coste cuadrático: tras cada decisión se reconstruía la tabla de cribado completa para devolver una sola fila. Ahora se consulta solo la fila afectada, con una mejora de unas 20 veces. La migración `0011` añade índices para el emparejamiento por DOI y por título y año, los conflictos y las búsquedas de evidencia.

## Preparación multiplataforma (no probada)

Solo se compila y prueba en macOS Apple Silicon. Queda preparado:

- **Python por plataforma:**
  - rutas de búsqueda en Linux;
  - en Windows, intérpretes de python.org en `%LOCALAPPDATA%\Programs\Python` y el entorno virtual en `Scripts\python.exe`;
  - el entorno aislado conserva las variables que Windows necesita.
- **Llavero:** `keyring` con Secret Service (GNOME Keyring o KWallet) en Linux y el almacén de credenciales en Windows. Estas dependencias no se compilan en macOS.
- **Rutas:** las del respaldo se escriben con `/` y se reajustan al restaurar.

Pendiente para cada plataforma:
- compilar e instalar;
- empaquetar el extractor Python;
- configurar el actualizador (`latest.json` con las nuevas plataformas);
- probar OCR y Tesseract.

## Documentación

- [Guía de uso](user-guide.md): primeros pasos, biblioteca, revisión, intercambio entre revisores, respaldo y actualizaciones.

## Migración

`0011_collaboration.sql` crea:
- `external_links`, el vínculo de Zotero;
- `exchange_imports`, los paquetes aplicados;
- índices de rendimiento.

## Pruebas

```text
cargo test                  → 65 passed, 1 ignored (benchmark)
cargo clippy --all-targets  → 0 avisos
npm run test                → 30 passed
python unittest             → 21 OK
```

- **Respaldo:**
  - contenido y huellas;
  - rechazo de archivos alterados, de rutas `..`, de zip ajenos y de esquemas futuros;
  - restauración en otra carpeta de datos con rutas reajustadas y copia previa conservada;
  - descarte de una preparación interrumpida y cancelación.
- **Intercambio, ida y vuelta entre «Ana» y «Bruno» con bases separadas:**
  - emparejamiento por DOI con un Id distinto;
  - mismos Id de estudio;
  - acuerdos finalizados y conflictos de cribado y de extracción;
  - resolución propagada al otro equipo;
  - idempotencia;
  - rechazo de nombres repetidos, paquetes ajenos y columnas inyectadas.
- **PDF posterior:** se adjunta al registro recibido.
- **Zotero:** base simulada con el esquema de Zotero, con elementos, autores (incluido uno institucional), etiquetas, colecciones anidadas, adjunto `storage:`, papelera, nota y DOI ya existente. Reimportar no duplica nada.
- **Interfaz:**
  - respaldo con confirmación antes de preparar la restauración;
  - error de huella y diagnóstico;
  - importación desde Zotero;
  - exportación e importación de paquetes;
  - resolución de conflictos de extracción.

## Limitaciones

- El intercambio es por archivo; no hay sincronización en tiempo real ni servidor.
- Los PDF no viajan en el paquete; cada revisor importa los suyos.
- Los adjuntos de Zotero con ruta relativa a un «directorio base» no se resuelven.
- Las ediciones de metadatos entre equipos no se fusionan: prevalece el registro local.
- Windows y Linux no se han compilado ni probado.

## Checklist de regresión

- [ ] Crear un respaldo, restaurarlo y comprobar que la biblioteca, las revisiones y los PDF se abren.
- [ ] Exportar un paquete, importarlo en otra instalación, decidir, devolverlo y ver los conflictos.
- [ ] Importar desde Zotero dos veces sin duplicados.
- [ ] Probar el cribado con un corpus grande: cada decisión debe sentirse instantánea.
- [ ] Revisar que las fases 1–8 siguen funcionando: importación, extracción, asistente, metodología, exportaciones y PRISMA.
