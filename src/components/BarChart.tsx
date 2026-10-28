import { useState } from "react";

export interface BarDatum {
  label: string;
  value: number;
  /** Extra line for the hover readout (e.g. the study Ids behind the count). */
  detail?: string;
}

const BAR = "#0d9488"; // validated ≥3:1 on light (#ffffff) and dark (#111827) surfaces
const ROW = 22;
const GAP = 2;
const LABEL_WIDTH = 150;

/** Horizontal single-series bar chart: one hue, no legend, hover readout and a table view. */
export function BarChart({ title, data, unit = "estudios" }: { title: string; data: BarDatum[]; unit?: string }) {
  const [active, setActive] = useState<number | null>(null);
  const [table, setTable] = useState(false);
  const max = Math.max(1, ...data.map((item) => item.value));
  const width = 520;
  const plot = width - LABEL_WIDTH - 40;
  const height = data.length * ROW;
  const focused = active === null ? null : data[active];

  return (
    <figure className="min-w-0">
      <figcaption className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-slate-700">{title}</span>
        <button type="button" className="text-[10px] font-semibold text-teal-700 hover:text-teal-600" onClick={() => setTable(!table)}>{table ? "Ver gráfico" : "Ver tabla"}</button>
      </figcaption>
      {data.length === 0 ? <p className="mt-2 text-[11px] text-slate-400">Sin datos confirmados.</p> : table ? (
        <table className="mt-2 w-full text-left text-[11px]">
          <thead className="text-slate-400"><tr><th className="py-1 font-medium">Valor</th><th className="font-medium">n</th><th className="font-medium">Detalle</th></tr></thead>
          <tbody className="text-slate-600">{data.map((item) => <tr key={item.label} className="border-t border-slate-100"><td className="py-1">{item.label}</td><td>{item.value}</td><td className="text-slate-400">{item.detail}</td></tr>)}</tbody>
        </table>
      ) : (
        <>
          <p className="mt-1 h-4 text-[11px] text-slate-500" aria-live="polite">{focused ? `${focused.label}: ${focused.value} ${unit}${focused.detail ? ` · ${focused.detail}` : ""}` : ""}</p>
          <svg viewBox={`0 0 ${width} ${height}`} className="w-full max-w-[520px]" role="img" aria-label={`${title}: ${data.map((item) => `${item.label} ${item.value}`).join(", ")}`} onMouseLeave={() => setActive(null)}>
            {data.map((item, index) => {
              const y = index * ROW;
              const barHeight = ROW - GAP * 2 - 4;
              const length = Math.max(2, (item.value / max) * plot);
              const radius = Math.min(4, length / 2);
              // Rounded only at the data end; the baseline end stays square.
              const path = `M${LABEL_WIDTH},${y + 2} h${length - radius} a${radius},${radius} 0 0 1 ${radius},${radius} v${barHeight - radius * 2} a${radius},${radius} 0 0 1 ${-radius},${radius} h${-(length - radius)} z`;
              return (
                <g key={item.label} tabIndex={0} onMouseEnter={() => setActive(index)} onFocus={() => setActive(index)} onBlur={() => setActive(null)} className="outline-none">
                  <rect x={0} y={y} width={width} height={ROW} fill="transparent" />
                  <text x={LABEL_WIDTH - 8} y={y + ROW / 2 + 4} textAnchor="end" className="fill-current text-slate-500" fontSize={11}>{item.label.length > 24 ? `${item.label.slice(0, 23)}…` : item.label}</text>
                  <path d={path} fill={BAR} opacity={active === null || active === index ? 1 : 0.55} />
                  {active === index && <text x={LABEL_WIDTH + length + 6} y={y + ROW / 2 + 4} className="fill-current text-slate-700" fontSize={11}>{item.value}</text>}
                </g>
              );
            })}
            <line x1={LABEL_WIDTH} x2={LABEL_WIDTH} y1={0} y2={height} className="stroke-current text-slate-300" strokeWidth={1} />
          </svg>
        </>
      )}
    </figure>
  );
}
