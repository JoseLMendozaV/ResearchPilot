import logoUrl from "../assets/logo.png";

export function AppLogo({ compact = false }: { compact?: boolean }) {
  return (
    <div className="flex items-center gap-3">
      <img src={logoUrl} alt="" aria-hidden="true" className="size-10 shrink-0 drop-shadow-[0_6px_14px_rgba(30,64,175,0.28)]" draggable={false} />
      {!compact && (
        <div>
          <p className="text-[17px] font-semibold tracking-[-0.02em] text-slate-900">ResearchPilot</p>
          <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-slate-400">Evidence workspace</p>
        </div>
      )}
    </div>
  );
}
