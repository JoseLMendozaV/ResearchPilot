# Cierre de Fase 8 — Síntesis y PRISMA

## PRISMA 2020 (pestaña «Síntesis y PRISMA»)

Cada casilla se cuenta a partir de eventos registrados; nada se estima ni se completa:

| Casilla | Origen |
|---|---|
| Registros por base de datos | Registros originales de las importaciones de cada cadena (sin errores) |
| Otras fuentes | Estudios del corpus sin ningún registro de las cadenas del proyecto (p. ej., añadidos desde la biblioteca) |
| Duplicados eliminados | Registros que coincidieron con un estudio ya presente (DOI o reimportación) |
| Eliminados por otros motivos | Estudios retirados del corpus |
| Cribados / excluidos / pendientes | Decisión final de la fase de metadatos |
| Buscados / no recuperados | Aprobados en metadatos; «no recuperado» = sin PDF y sin decisión a texto completo |
| Evaluados / excluidos con motivo | Decisión final a texto completo y criterio registrado |
| Incluidos | Inclusión final a texto completo |

- Se comprueba la aritmética (identificados − duplicados − eliminados = cribados; cribados = excluidos + buscados + pendientes; buscados = no recuperados + evaluados + pendientes; evaluados = excluidos + incluidos; suma de motivos = excluidos). Cualquier descuadre se muestra como advertencia.
- Con decisiones pendientes el diagrama se marca **provisional**, en pantalla y en el SVG.
- El diagrama se genera en SVG en el núcleo Rust (texto escapado), se ve en la app y se exporta (`prisma` en Exportar). El informe Markdown y el JSON incluyen los conteos.
- PRISMA es una guía de reporte: el diagrama refleja lo registrado, no valida la metodología.

## Síntesis descriptiva

- Distribución de incluidos por año y por fuente, y frecuencias por aspecto categórico, lista o sí/no; resumen numérico (n, mínimo, mediana, máximo) por aspecto numérico con su unidad.
- **Solo valores confirmados**; los propuestos o por revisar se cuentan aparte y se avisan.
- Advertencia de comparabilidad en aspectos numéricos (dataset, configuración y métrica). No hay metaanálisis: queda como módulo posterior con verificación especializada.
- Gráficos de barras de una serie con tono validado (≥ 3:1 en tema claro y oscuro), lectura al pasar el cursor o con teclado, que indica los Id de los estudios, y vista de tabla.

## Borradores del informe

- Metodología, resultados y limitaciones generados de forma determinista (sin IA) a partir de protocolo, cadenas, criterios, PRISMA, calidad y valores confirmados.
- Cada afirmación lleva los **Id de los estudios** que la sustentan; los datos faltantes aparecen como «[Pendiente…]» o «[Provisional…]».
- Editables; regenerar un borrador editado pide confirmación y el texto anterior queda en la auditoría.

## Migración

- `0010_synthesis.sql`: tabla `synthesis_drafts`.

## Pruebas

```text
cargo test               → 55 passed (3 nuevas de Fase 8)
cargo clippy --all-targets → 0 avisos
npm run test             → 23 passed
python unittest          → 21 OK
```

Escenario con números conocidos:
- dos bases con un duplicado entre ellas, un estudio desde la biblioteca y otro retirado;
- exclusiones en metadatos y un estudio sin PDF;
- exclusión a texto completo con motivo y un estudio pendiente.

Se comprueban:
- cada casilla, el cuadre y la detección de descuadres;
- el SVG escapado y la marca de provisional;
- la síntesis ignorando valores sin confirmar, y la mediana;
- los borradores con trazas `[Id …]` y auditoría.

## Limitaciones

- Los borradores son plantillas basadas en datos, no redacción asistida por IA.
- No hay gráficos por país ni metodología salvo que se definan como aspectos de extracción.
- El diagrama sigue la plantilla PRISMA 2020 de bases de datos y otras fuentes en una sola columna de identificación; las plantillas específicas (PRISMA-ScR, actualizaciones de revisiones) quedan pendientes.
