import { useState } from "react";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { reviewBackend } from "../../lib/reviewBackend";
import type { ExtractionField, FieldInput, FieldType, QuestionInput, ResearchQuestion } from "../../reviewTypes";
import { Card, Empty, ErrorNote, Loading, useAction, useLoad } from "./shared";

const typeLabels: Record<FieldType, string> = { text: "Texto", number: "Número", boolean: "Sí/No", date: "Fecha", category: "Categoría (taxonomía)", list: "Lista" };
const categories = ["Técnicas, herramientas y procesos", "Evaluación", "Contexto", "Beneficios y retos"];

export function QuestionsTab({ projectId }: { projectId: string }) {
  const overview = useLoad(() => reviewBackend.questions(projectId), [projectId]);
  const masterFields = useLoad(() => reviewBackend.masterFields(), []);
  const [question, setQuestion] = useState<QuestionInput | null>(null);
  const [field, setField] = useState<FieldInput | null>(null);
  const action = useAction();
  const data = overview.data;

  const emptyField = (researchQuestionId?: string): FieldInput => ({ projectId, researchQuestionId, name: "", description: "", dataType: "text", taxonomy: [], instructions: "", required: false, examples: [] });
  const editField = (item: ExtractionField): FieldInput => ({ ...item, projectId, researchQuestionId: item.researchQuestionId });

  async function saveQuestion() {
    if (!question) return;
    const next = await action.run(() => reviewBackend.saveQuestion(question));
    if (next) {
      overview.setData(next);
      setQuestion(null);
    }
  }

  async function saveField() {
    if (!field) return;
    const next = await action.run(() => reviewBackend.saveField(field));
    if (next) {
      overview.setData(next);
      setField(null);
    }
  }

  async function remove(kind: "question" | "field", id: string) {
    const next = await action.run(() => kind === "question" ? reviewBackend.deleteQuestion(projectId, id) : reviewBackend.deleteField(projectId, id));
    if (next) overview.setData(next);
  }

  if (!data) return overview.loading ? <Loading /> : <ErrorNote message={overview.error} />;

  const fieldList = (fields: ExtractionField[]) => (
    <ul className="mt-2 space-y-1">
      {fields.map((item) => (
        <li key={item.id} className="flex items-start justify-between gap-2 rounded-lg bg-slate-50 px-3 py-2 text-xs">
          <div>
            <p className="font-medium text-slate-700">{item.name}{item.required ? " *" : ""} <span className="font-normal text-slate-400">· {typeLabels[item.dataType]}{item.unit ? ` (${item.unit})` : ""}</span></p>
            {item.taxonomy.length > 0 && <p className="text-[11px] text-slate-400">{item.taxonomy.join(" · ")}</p>}
            {item.masterFieldKey && <p className="text-[11px] text-teal-700">Vinculado a la ficha: {masterFields.data?.find((master) => master.fieldKey === item.masterFieldKey)?.labelEs ?? item.masterFieldKey}</p>}
            {item.instructions && <p className="text-[11px] text-slate-400">{item.instructions}</p>}
          </div>
          <div className="flex shrink-0 gap-1">
            <button className="viewer-button size-7" aria-label={`Editar ${item.name}`} onClick={() => setField(editField(item))}><Pencil className="size-3.5" /></button>
            <button className="viewer-button size-7" aria-label={`Eliminar ${item.name}`} onClick={() => void remove("field", item.id)}><Trash2 className="size-3.5" /></button>
          </div>
        </li>
      ))}
    </ul>
  );

  return (
    <div className="grid grid-cols-[1.4fr_1fr] gap-6">
      <div className="space-y-4">
        <Card title="Aspectos generales" actions={<button className="button-secondary px-2.5 py-1.5 text-[11px]" onClick={() => setField(emptyField())}><Plus className="size-3" /> Aspecto</button>}>
          {data.generalFields.length ? fieldList(data.generalFields) : <p className="text-xs text-slate-400">Datos comunes a todos los estudios (país, tipo de estudio, etc.).</p>}
        </Card>
        {data.questions.length === 0 && <Empty title="Sin preguntas de investigación" detail="Las categorías sugeridas son ejemplos, no preguntas obligatorias." />}
        {data.questions.map((item: ResearchQuestion) => (
          <Card key={item.id} title={`${item.code}${item.parentId ? " (subpregunta)" : ""} · ${item.question}`} actions={
            <>
              <button className="button-secondary px-2.5 py-1.5 text-[11px]" onClick={() => setField(emptyField(item.id))}><Plus className="size-3" /> Aspecto</button>
              <button className="viewer-button size-7" aria-label={`Editar ${item.code}`} onClick={() => setQuestion({ id: item.id, projectId, parentId: item.parentId, code: item.code, question: item.question, objective: item.objective, category: item.category })}><Pencil className="size-3.5" /></button>
              <button className="viewer-button size-7" aria-label={`Eliminar ${item.code}`} onClick={() => void remove("question", item.id)}><Trash2 className="size-3.5" /></button>
            </>
          }>
            {item.category && <p className="text-[11px] text-slate-400">{item.category}{item.objective ? ` · ${item.objective}` : ""}</p>}
            {item.fields.length ? fieldList(item.fields) : <p className="text-xs text-slate-400">Agrega los aspectos que extraerás para responder esta pregunta.</p>}
          </Card>
        ))}
        <button className="button-primary" onClick={() => setQuestion({ projectId, question: "", objective: "", category: "" })}><Plus className="size-4" /> Nueva pregunta</button>
      </div>

      <div className="space-y-4">
        <ErrorNote message={action.error} />
        {question && (
          <Card title={question.id ? "Editar pregunta" : "Nueva pregunta"}>
            <div className="space-y-3">
              <div className="grid grid-cols-[90px_1fr] gap-2">
                <label className="label">Código<input className="input mt-1" placeholder="RQn" value={question.code ?? ""} onChange={(event) => setQuestion({ ...question, code: event.target.value })} /></label>
                <label className="label">Categoría
                  <input className="input mt-1" list="rq-categories" value={question.category} onChange={(event) => setQuestion({ ...question, category: event.target.value })} />
                  <datalist id="rq-categories">{categories.map((category) => <option key={category} value={category} />)}</datalist>
                </label>
              </div>
              <label className="label">Pregunta<textarea className="input mt-1 min-h-16" value={question.question} onChange={(event) => setQuestion({ ...question, question: event.target.value })} /></label>
              <label className="label">Objetivo<input className="input mt-1" value={question.objective} onChange={(event) => setQuestion({ ...question, objective: event.target.value })} /></label>
              <label className="label">Subpregunta de
                <select className="input mt-1" value={question.parentId ?? ""} onChange={(event) => setQuestion({ ...question, parentId: event.target.value || undefined })}>
                  <option value="">Ninguna (pregunta principal)</option>
                  {data.questions.filter((item) => item.id !== question.id).map((item) => <option key={item.id} value={item.id}>{item.code}</option>)}
                </select>
              </label>
              <div className="flex justify-end gap-2"><button className="button-secondary" onClick={() => setQuestion(null)}>Cancelar</button><button className="button-primary" disabled={action.busy} onClick={() => void saveQuestion()}>Guardar</button></div>
            </div>
          </Card>
        )}
        {field && (
          <Card title={field.id ? "Editar aspecto" : "Nuevo aspecto"}>
            <div className="space-y-3">
              <label className="label">Nombre<input className="input mt-1" value={field.name} onChange={(event) => setField({ ...field, name: event.target.value })} /></label>
              <div className="grid grid-cols-2 gap-2">
                <label className="label">Tipo de dato
                  <select className="input mt-1" value={field.dataType} onChange={(event) => setField({ ...field, dataType: event.target.value as FieldType })}>
                    {Object.entries(typeLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                  </select>
                </label>
                <label className="label">Unidad<input className="input mt-1" value={field.unit ?? ""} onChange={(event) => setField({ ...field, unit: event.target.value || undefined })} /></label>
              </div>
              <label className="label">RQ
                <select className="input mt-1" value={field.researchQuestionId ?? ""} onChange={(event) => setField({ ...field, researchQuestionId: event.target.value || undefined })}>
                  <option value="">General</option>
                  {data.questions.map((item) => <option key={item.id} value={item.id}>{item.code}</option>)}
                </select>
              </label>
              {field.dataType === "category" && <label className="label">Taxonomía (una categoría por línea)<textarea className="input mt-1 min-h-20" value={field.taxonomy.join("\n")} onChange={(event) => setField({ ...field, taxonomy: event.target.value.split("\n") })} /></label>}
              <label className="label">Instrucciones para el extractor<textarea className="input mt-1 min-h-14" value={field.instructions} onChange={(event) => setField({ ...field, instructions: event.target.value })} /></label>
              <label className="label">Ejemplos (uno por línea)<textarea className="input mt-1 min-h-12" value={field.examples.join("\n")} onChange={(event) => setField({ ...field, examples: event.target.value.split("\n") })} /></label>
              <label className="label">Vincular con la ficha maestra (prellenado)
                <select className="input mt-1" value={field.masterFieldKey ?? ""} onChange={(event) => setField({ ...field, masterFieldKey: event.target.value || undefined })}>
                  <option value="">Sin vínculo</option>
                  {masterFields.data?.map((master) => <option key={master.fieldKey} value={master.fieldKey}>{master.labelEs}</option>)}
                </select>
              </label>
              <label className="flex items-center gap-2 text-xs text-slate-600"><input type="checkbox" className="accent-teal-600" checked={field.required} onChange={(event) => setField({ ...field, required: event.target.checked })} /> Obligatorio</label>
              <div className="flex justify-end gap-2"><button className="button-secondary" onClick={() => setField(null)}>Cancelar</button><button className="button-primary" disabled={action.busy} onClick={() => void saveField()}>Guardar</button></div>
            </div>
          </Card>
        )}
      </div>
    </div>
  );
}
