import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, BookOpenCheck, ClipboardCopy, ExternalLink, FileSearch, LoaderCircle, MessageSquarePlus, Search, Send, ShieldAlert, Square, Trash2 } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalConsentDialog } from "../components/ExternalConsentDialog";
import { Link, useSearchParams } from "react-router-dom";
import { backend, errorMessage } from "../lib/backend";
import { evidenceLink, formatDateTime } from "../lib/extraction";
import type { AiProvider, AppSettings, AskInput, AssistantMessage, ConversationDetail, ConversationSummary, ExternalConsent, ExternalEstimate, LibraryArticle, LibraryCollection, OllamaStatus, ScopeType, SearchOutcome } from "../types";

interface Props {
  articles: LibraryArticle[];
  collections: LibraryCollection[];
  settings: AppSettings;
}

const statusLabels: Record<AssistantMessage["status"], { label: string; tone: string }> = {
  question: { label: "", tone: "" },
  answered: { label: "Respuesta con evidencia verificada", tone: "bg-emerald-50 text-emerald-700" },
  not_found: { label: "Sin evidencia en los documentos", tone: "bg-slate-100 text-slate-600" },
  unsupported: { label: "Respuesta descartada: citas no verificables", tone: "bg-amber-50 text-amber-800" },
  failed: { label: "Error", tone: "bg-rose-50 text-rose-700" },
};

export function AssistantPage({ articles, collections, settings }: Props) {
  const [searchParams] = useSearchParams();
  const [tab, setTab] = useState<"ask" | "search">("ask");
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [detail, setDetail] = useState<ConversationDetail | null>(null);
  const [scopeType, setScopeType] = useState<ScopeType>((searchParams.get("scope") as ScopeType) ?? "library");
  const [scopeId, setScopeId] = useState(searchParams.get("id") ?? "");
  const [yearFrom, setYearFrom] = useState("");
  const [yearTo, setYearTo] = useState("");
  const [provider, setProvider] = useState<AiProvider>(settings.aiProvider);
  const [model, setModel] = useState(settings.ollamaModel);
  const [consentRequest, setConsentRequest] = useState<{ estimate: ExternalEstimate; input: AskInput } | null>(null);
  const externalModel = provider === "openai" ? settings.openaiModel : provider === "anthropic" ? settings.anthropicModel : "";
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [question, setQuestion] = useState("");
  const [pending, setPending] = useState<{ requestId: string; question: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchResult, setSearchResult] = useState<SearchOutcome | null>(null);
  const [searching, setSearching] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const chatModels = useMemo(() => ollama?.models.filter((item) => item.kind !== "embedding") ?? [], [ollama]);

  const loadConversations = useCallback(async () => {
    try {
      setConversations(await backend.listConversations());
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, []);

  useEffect(() => {
    void loadConversations();
    backend.ollamaStatus().then(setOllama).catch(() => setOllama(null));
  }, [loadConversations]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ block: "end" });
  }, [detail?.messages.length, pending]);

  async function openConversation(id: string) {
    setError(null);
    try {
      const loaded = await backend.getConversation(id);
      setDetail(loaded);
      setScopeType(loaded.conversation.scopeType);
      setScopeId(loaded.conversation.scopeId ?? "");
      setTab("ask");
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function removeConversation(id: string) {
    await backend.deleteConversation(id);
    if (detail?.conversation.id === id) setDetail(null);
    await loadConversations();
  }

  const scope = () => ({
    scopeType,
    scopeId: scopeType === "library" ? undefined : scopeId || undefined,
    yearFrom: yearFrom ? Number(yearFrom) : undefined,
    yearTo: yearTo ? Number(yearTo) : undefined,
  });

  async function ask(event: FormEvent) {
    event.preventDefault();
    const text = question.trim();
    if (!text || pending) return;
    if (scopeType !== "library" && !scopeId) {
      setError("Elige el artículo o la colección para este ámbito.");
      return;
    }
    const input: AskInput = { requestId: crypto.randomUUID(), conversationId: detail?.conversation.id, question: text, provider, model: provider === "ollama" ? model || undefined : externalModel || undefined, ...scope() };
    setError(null);
    if (provider !== "ollama") {
      // Nothing leaves the machine until the user authorizes this exact estimate.
      try {
        setConsentRequest({ estimate: await backend.estimateAssistant(input), input });
      } catch (cause) {
        setError(errorMessage(cause));
      }
      return;
    }
    await send(input);
  }

  async function send(input: AskInput, consent?: ExternalConsent) {
    setPending({ requestId: input.requestId, question: input.question });
    setQuestion("");
    try {
      setDetail(await backend.askAssistant({ ...input, consent }));
      await loadConversations();
    } catch (cause) {
      setError(errorMessage(cause));
      setQuestion(input.question);
    } finally {
      setPending(null);
    }
  }

  async function search(event: FormEvent) {
    event.preventDefault();
    if (!question.trim()) return;
    if (scopeType !== "library" && !scopeId) {
      setError("Elige el artículo o la colección para este ámbito.");
      return;
    }
    setSearching(true);
    setError(null);
    try {
      setSearchResult(await backend.searchChunks({ query: question.trim(), ...scope() }));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSearching(false);
    }
  }

  return (
    <div className="grid h-full min-h-0 grid-cols-[260px_1fr]">
      <aside className="flex min-h-0 flex-col border-r border-slate-200 bg-white">
        <div className="p-4">
          <button className="button-primary w-full" onClick={() => { setDetail(null); setTab("ask"); }}><MessageSquarePlus className="size-4" /> Nueva conversación</button>
        </div>
        <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-4" aria-label="Conversaciones">
          {conversations.length === 0 && <p className="px-2 text-[11px] text-slate-400">Las conversaciones se guardan en este equipo.</p>}
          {conversations.map((conversation) => (
            <div key={conversation.id} className={`group flex items-start gap-1 rounded-lg px-2 py-2 ${detail?.conversation.id === conversation.id ? "bg-teal-50" : "hover:bg-slate-50"}`}>
              <button className="min-w-0 flex-1 text-left" onClick={() => void openConversation(conversation.id)}>
                <p className="truncate text-xs font-medium text-slate-700">{conversation.title}</p>
                <p className="truncate text-[10px] text-slate-400">{conversation.scopeLabel ?? (conversation.scopeType === "library" ? "Toda la biblioteca" : "Ámbito eliminado")} · {formatDateTime(conversation.updatedAt)}</p>
              </button>
              <button className="invisible rounded p-1 text-slate-300 hover:text-rose-600 group-hover:visible" aria-label="Eliminar conversación" onClick={() => void removeConversation(conversation.id)}><Trash2 className="size-3.5" /></button>
            </div>
          ))}
        </nav>
      </aside>

      <section className="flex min-h-0 flex-col">
        <div className="flex flex-wrap items-end gap-3 border-b border-slate-200 bg-white px-6 py-3">
          <div className="flex rounded-lg bg-slate-100 p-0.5 text-[11px] font-semibold" role="tablist">
            {([["ask", "Preguntar"], ["search", "Buscar fragmentos"]] as const).map(([key, label]) => (
              <button key={key} role="tab" aria-selected={tab === key} className={`rounded-md px-3 py-1.5 ${tab === key ? "bg-white text-slate-800 shadow-sm" : "text-slate-500"}`} onClick={() => setTab(key)}>{label}</button>
            ))}
          </div>
          <label className="label">Ámbito
            <select className="input mt-1 w-40" value={scopeType} onChange={(event) => { setScopeType(event.target.value as ScopeType); setScopeId(""); }} disabled={Boolean(detail)}>
              <option value="library">Toda la biblioteca</option>
              <option value="collection">Colección</option>
              <option value="article">Artículo</option>
            </select>
          </label>
          {scopeType === "collection" && (
            <label className="label">Colección
              <select className="input mt-1 w-56" value={scopeId} onChange={(event) => setScopeId(event.target.value)} disabled={Boolean(detail)}>
                <option value="">Elige…</option>
                {collections.map((collection) => <option key={collection.id} value={collection.id}>{collection.name}</option>)}
              </select>
            </label>
          )}
          {scopeType === "article" && (
            <label className="label">Artículo
              <select className="input mt-1 w-72" value={scopeId} onChange={(event) => setScopeId(event.target.value)} disabled={Boolean(detail)}>
                <option value="">Elige…</option>
                {articles.map((article) => <option key={article.id} value={article.id}>{article.title}</option>)}
              </select>
            </label>
          )}
          <label className="label">Años
            <span className="mt-1 flex items-center gap-1">
              <input className="input w-20" inputMode="numeric" placeholder="desde" value={yearFrom} onChange={(event) => setYearFrom(event.target.value.replace(/\D/g, "").slice(0, 4))} />
              <input className="input w-20" inputMode="numeric" placeholder="hasta" value={yearTo} onChange={(event) => setYearTo(event.target.value.replace(/\D/g, "").slice(0, 4))} />
            </span>
          </label>
          {tab === "ask" && (
            <label className="label">Proveedor
              <select className="input mt-1 w-36" value={provider} onChange={(event) => setProvider(event.target.value as AiProvider)}>
                <option value="ollama">Ollama (local)</option>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
              </select>
            </label>
          )}
          {tab === "ask" && provider !== "ollama" && (
            <p className="mb-2 max-w-48 text-[10px] leading-4 text-amber-700">Externo · {externalModel || "sin modelo"}. Se pedirá confirmación con coste antes de enviar.</p>
          )}
          {tab === "ask" && provider === "ollama" && (
            <label className="label">Modelo
              <select className="input mt-1 w-52" value={model} onChange={(event) => setModel(event.target.value)}>
                {!model && <option value="">Sin modelo</option>}
                {model && !chatModels.some((item) => item.name === model) && <option value={model}>{model}</option>}
                {chatModels.map((item) => <option key={item.name} value={item.name}>{item.name}{item.parameterSize ? ` · ${item.parameterSize}` : ""}</option>)}
              </select>
            </label>
          )}
        </div>

        {consentRequest && (
          <ExternalConsentDialog
            estimate={consentRequest.estimate}
            purpose="Vas a hacer una pregunta al asistente con un proveedor externo."
            onCancel={() => setConsentRequest(null)}
            onConfirm={(consent) => { const input = consentRequest.input; setConsentRequest(null); void send(input, consent); }}
          />
        )}
        {tab === "ask" && provider === "ollama" && ollama && !ollama.reachable && (
          <p className="mx-6 mt-4 flex items-start gap-2 rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-900"><AlertTriangle className="mt-px size-4 shrink-0" />{ollama.message} <Link className="font-semibold underline" to="/configuracion">Configuración</Link></p>
        )}
        {error && <p className="mx-6 mt-4 rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-700">{error}</p>}

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          {tab === "ask" ? (
            <div className="mx-auto max-w-3xl space-y-5">
              {!detail && !pending && (
                <div className="mt-10 text-center">
                  <BookOpenCheck className="mx-auto size-8 text-teal-600" />
                  <h2 className="mt-3 text-sm font-semibold text-slate-700">Pregunta a tus artículos</h2>
                  <p className="mx-auto mt-1 max-w-md text-xs leading-5 text-slate-400">El modelo local solo ve los fragmentos recuperados de tu biblioteca. Cada cita se comprueba contra el texto original; si no hay evidencia, la respuesta lo dice en lugar de inventar.</p>
                </div>
              )}
              {detail?.messages.map((message) => <MessageView key={message.id} message={message} />)}
              {pending && (
                <>
                  <div className="ml-auto max-w-[80%] rounded-2xl rounded-br-sm bg-teal-600 px-4 py-2.5 text-sm text-white">{pending.question}</div>
                  <div className="flex items-center gap-3 text-xs text-slate-500">
                    <LoaderCircle className="size-4 animate-spin text-teal-600" /> Buscando evidencia y consultando el modelo local…
                    <button className="button-secondary px-2.5 py-1.5" onClick={() => void backend.cancelAssistant(pending.requestId)}><Square className="size-3" /> Cancelar</button>
                  </div>
                </>
              )}
              <div ref={bottomRef} />
            </div>
          ) : (
            <SearchResults result={searchResult} searching={searching} question={question} />
          )}
        </div>

        <form className="border-t border-slate-200 bg-white px-6 py-4" onSubmit={tab === "ask" ? ask : search}>
          <div className="mx-auto flex max-w-3xl items-end gap-2">
            <textarea
              className="input min-h-11 flex-1 resize-none"
              rows={2}
              placeholder={tab === "ask" ? "Escribe una pregunta sobre los artículos…" : "Términos o descripción del fragmento que buscas…"}
              value={question}
              onChange={(event) => setQuestion(event.target.value)}
              onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }}
              aria-label={tab === "ask" ? "Pregunta" : "Búsqueda"}
            />
            <button className="button-primary h-11" disabled={Boolean(pending) || searching || !question.trim()}>
              {tab === "ask" ? <Send className="size-4" /> : <Search className="size-4" />} {tab === "ask" ? "Preguntar" : "Buscar"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

export function MessageView({ message }: { message: AssistantMessage }) {
  if (message.role === "user") {
    return <div className="ml-auto max-w-[80%] whitespace-pre-wrap rounded-2xl rounded-br-sm bg-teal-600 px-4 py-2.5 text-sm text-white">{message.content}</div>;
  }
  const status = statusLabels[message.status];
  return (
    <article className="panel p-4">
      <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold ${status.tone}`}>{message.status === "unsupported" && <ShieldAlert className="size-3" />}{status.label}</span>
      <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-slate-700">{message.content}</p>
      {message.citations.length > 0 && (
        <ol className="mt-3 space-y-2 border-t border-slate-100 pt-3">
          {message.citations.map((citation, index) => (
            <li key={`${citation.source}-${index}`} className="rounded-lg bg-slate-50 p-2.5">
              <p className="text-xs italic leading-5 text-slate-600"><span className="mr-1 not-italic font-semibold text-teal-700">[{citation.source}]</span>“{citation.quote}”</p>
              <Link className="mt-1 flex items-center gap-1 text-[11px] font-semibold text-teal-700 hover:text-teal-600" to={evidenceLink(citation.articleId, citation.page, citation.bbox, citation.pageWidth)}>
                <FileSearch className="size-3" /> {citation.articleTitle} · p. {citation.page}{citation.section ? ` · ${citation.section}` : ""}
              </Link>
            </li>
          ))}
        </ol>
      )}
      {message.sources?.warnings && message.sources.warnings.length > 0 && (
        <ul className="mt-2 space-y-1">{message.sources.warnings.map((warning) => <li key={warning} className="text-[10px] text-amber-700">{warning}</li>)}</ul>
      )}
      {message.sources?.chunks && message.sources.chunks.length > 0 && (
        <details className="mt-2 text-[10px] text-slate-400">
          <summary className="cursor-pointer">Fragmentos consultados ({message.sources.chunks.length}) · búsqueda {message.sources.mode}</summary>
          <ul className="mt-1 space-y-0.5">{message.sources.chunks.map((chunk) => <li key={chunk.source}>{chunk.source}: {chunk.articleTitle}, p. {chunk.page}</li>)}</ul>
        </details>
      )}
    </article>
  );
}

function SearchResults({ result, searching, question }: { result: SearchOutcome | null; searching: boolean; question: string }) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [copied, setCopied] = useState(false);

  async function copyForWeb() {
    if (!result) return;
    const chosen = result.chunks.filter((chunk) => selected.has(chunk.chunkId));
    const text = [
      question.trim() ? `Pregunta: ${question.trim()}` : "",
      "Responde solo con los extractos siguientes y cita la fuente [n] de cada afirmación.",
      ...chosen.map((chunk, index) => `[${index + 1}] ${chunk.articleTitle}, p. ${chunk.page}:\n${chunk.text}`),
    ].filter(Boolean).join("\n\n");
    await navigator.clipboard.writeText(text);
    setCopied(true);
  }
  if (searching) return <div className="grid h-40 place-items-center"><LoaderCircle className="size-5 animate-spin text-teal-600" /></div>;
  if (!result) {
    return <p className="mx-auto mt-10 max-w-md text-center text-xs leading-5 text-slate-400">La búsqueda combina el índice léxico (FTS5) y, si hay un modelo de embeddings, el índice semántico local. No usa el modelo de lenguaje.</p>;
  }
  return (
    <div className="mx-auto max-w-3xl">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-[11px] text-slate-400">{result.chunks.length} fragmento(s) · búsqueda {result.mode === "hybrid" ? "híbrida" : result.mode === "semantic" ? "semántica" : "léxica"}</p>
        <div className="flex items-center gap-1.5">
          <button className="button-secondary px-2.5 py-1.5 text-[11px]" disabled={selected.size === 0} onClick={() => void copyForWeb()} title="Copia solo los extractos marcados; no se envía nada automáticamente">
            <ClipboardCopy className="size-3" /> {copied ? "Copiado" : `Copiar ${selected.size} para ChatGPT/Claude web`}
          </button>
          <button className="button-secondary px-2.5 py-1.5 text-[11px]" onClick={() => void openUrl("https://chatgpt.com/")}><ExternalLink className="size-3" /> ChatGPT</button>
          <button className="button-secondary px-2.5 py-1.5 text-[11px]" onClick={() => void openUrl("https://claude.ai/new")}><ExternalLink className="size-3" /> Claude</button>
        </div>
      </div>
      {selected.size > 0 && <p className="mt-1 text-[10px] text-amber-700">Lo que pegues en un servicio web sale de tu equipo bajo sus condiciones. Revisa que no incluya material confidencial.</p>}
      {result.warnings.map((warning) => <p key={warning} className="mt-1 text-[11px] text-amber-700">{warning}</p>)}
      <ol className="mt-3 space-y-3">
        {result.chunks.map((chunk) => (
          <li key={chunk.chunkId} className="panel p-4">
            <div className="flex items-start justify-between gap-3">
              <label className="flex min-w-0 items-start gap-2">
                <input type="checkbox" className="mt-0.5 accent-teal-600" checked={selected.has(chunk.chunkId)} onChange={(event) => { setCopied(false); setSelected((current) => { const next = new Set(current); if (event.target.checked) next.add(chunk.chunkId); else next.delete(chunk.chunkId); return next; }); }} aria-label={`Seleccionar extracto de ${chunk.articleTitle}, página ${chunk.page}`} />
                <Link className="text-xs font-semibold text-slate-700 hover:text-teal-700" to={evidenceLink(chunk.articleId, chunk.page)}>{chunk.articleTitle}</Link>
              </label>
              <span className="shrink-0 text-[10px] text-slate-400">p. {chunk.page}{chunk.lexicalRank ? ` · léxico #${chunk.lexicalRank}` : ""}{chunk.semanticRank ? ` · semántico #${chunk.semanticRank}` : ""}</span>
            </div>
            {chunk.section && <p className="mt-0.5 text-[10px] text-slate-400">{chunk.section}</p>}
            <p className="mt-2 line-clamp-5 whitespace-pre-wrap text-xs leading-5 text-slate-600">{chunk.text}</p>
          </li>
        ))}
      </ol>
      {result.chunks.length === 0 && <p className="mt-6 text-center text-xs text-slate-400">Sin coincidencias. ¿Está indexada la biblioteca? Revisa Configuración → Inteligencia artificial.</p>}
    </div>
  );
}
