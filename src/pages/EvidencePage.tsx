import { FileCheck2, Fingerprint, Quote, ScanText } from "lucide-react";
import type { DashboardStats } from "../types";

export function EvidencePage({ stats }: { stats: DashboardStats }) {
  return (
    <div className="mx-auto max-w-[1280px] p-8">
      <div className="grid grid-cols-[1fr_1.4fr] gap-6">
        <section className="panel p-6">
          <div className="grid size-11 place-items-center rounded-xl bg-teal-50 text-teal-700"><FileCheck2 className="size-5" /></div>
          <p className="mt-5 text-4xl font-semibold tracking-[-0.05em] text-slate-900">{stats.evidenceSpans}</p>
          <p className="mt-1 text-sm font-medium text-slate-500">fragmentos de evidencia registrados</p>
          <p className="mt-4 text-xs leading-5 text-slate-400">Cada valor extraído podrá enlazarse con páginas, texto citado, ruta local y huella SHA-256 del documento fuente.</p>
        </section>
        <section className="panel p-6">
          <h2 className="section-title">Cadena de procedencia</h2>
          <p className="section-subtitle">Modelo preparado para evidencia verificable</p>
          <div className="mt-5 grid grid-cols-3 gap-3">
            <Feature icon={ScanText} title="Localizador" text="Página y posición en PDF" />
            <Feature icon={Quote} title="Fragmento" text="Cita exacta conservada" />
            <Feature icon={Fingerprint} title="Integridad" text="Hash del documento" />
          </div>
        </section>
      </div>
      <div className="panel mt-6 grid min-h-64 place-items-center p-8 text-center">
        <div className="max-w-lg">
          <p className="text-sm font-semibold text-slate-700">No hay evidencias todavía</p>
          <p className="mt-2 text-xs leading-5 text-slate-400">La captura desde PDF y la validación humana forman parte de las siguientes fases. La base de datos ya exige procedencia y estado de revisión para los datos extraídos.</p>
        </div>
      </div>
    </div>
  );
}

function Feature({ icon: Icon, title, text }: { icon: typeof ScanText; title: string; text: string }) {
  return (
    <div className="rounded-xl bg-slate-50 p-4">
      <Icon className="size-4 text-teal-600" />
      <p className="mt-3 text-xs font-semibold text-slate-700">{title}</p>
      <p className="mt-1 text-[11px] text-slate-400">{text}</p>
    </div>
  );
}
