import { AlertTriangle, LoaderCircle, RefreshCw } from "lucide-react";

export function LoadingView() {
  return (
    <div className="grid min-h-[calc(100vh-82px)] place-items-center p-8">
      <div className="text-center">
        <LoaderCircle className="mx-auto size-7 animate-spin text-teal-600" />
        <p className="mt-3 text-sm font-medium text-slate-600">Preparando el espacio local…</p>
      </div>
    </div>
  );
}

export function ErrorView({ message, retry }: { message: string; retry: () => void }) {
  return (
    <div className="grid min-h-[calc(100vh-82px)] place-items-center p-8">
      <div className="max-w-md rounded-2xl border border-rose-200 bg-white p-6 text-center shadow-sm">
        <AlertTriangle className="mx-auto size-8 text-rose-500" />
        <h2 className="mt-3 font-semibold text-slate-900">No se pudo abrir ResearchPilot</h2>
        <p className="mt-2 text-sm leading-6 text-slate-500">{message}</p>
        <button className="button-primary mx-auto mt-5" onClick={retry}>
          <RefreshCw className="size-4" /> Reintentar
        </button>
      </div>
    </div>
  );
}
