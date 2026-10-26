"use client";

import Monitoring1, {
  type Incident,
  type Kpi,
} from "@/components/blocks/monitoring-1";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const chip =
  "inline-flex h-7 items-center gap-1.5 rounded-[var(--rb-r-sm,6px)] border px-2.5 text-[12px]/[16px] font-[450] select-none";

export default function CoveragePage() {
  const { data: coverage, loading } = useApi(() => api.coverage());

  if (!loading && !coverage) {
    return (
      <EmptyState title="Coverage unavailable" hint="GET /coverage returned nothing." />
    );
  }

  const techniques = coverage?.techniques ?? [];
  const covered = techniques.filter((t) => t.covered).length;
  const pct = techniques.length
    ? Math.round((covered / techniques.length) * 100)
    : 0;

  const kpis: Kpi[] = [
    { id: "req", label: "Techniques covered", base: covered, drift: 0.02, unit: "" },
    { id: "err", label: "Techniques total", base: techniques.length, drift: 0.01, unit: "" },
    { id: "p95", label: "Coverage", base: pct, drift: 0.03, unit: "%" },
    { id: "sat", label: "Tactics", base: coverage?.tactics?.length ?? 0, drift: 0, unit: "" },
  ];

  const incidents: Incident[] = techniques
    .filter((t) => !t.covered)
    .slice(0, 6)
    .map((t) => ({
      id: t.id,
      title: `${t.id} ${t.name}`.trim(),
      severity: "Gap",
      tone: "warning" as const,
      service: t.tactic || "MITRE ATT&CK",
      at: "",
      body: "No detection coverage reported for this technique.",
    }));

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className="h-[560px] shrink-0">
          <Monitoring1
            incidents={incidents}
            kpis={kpis}
            title="Detection coverage"
          />
        </div>

        <div className="rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-4 dark:border-neutral-100/14 dark:bg-neutral-900">
          <div className="mb-3 flex items-baseline justify-between gap-3">
            <h2 className="text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
              ATT&amp;CK techniques
            </h2>
            <span className="text-[12px] tabular-nums text-neutral-900/66 dark:text-neutral-100/66">
              {covered} of {techniques.length} covered
            </span>
          </div>
          {techniques.length === 0 ? (
            <p className="text-[13px] text-neutral-900/66 dark:text-neutral-100/66">
              No technique data returned by /coverage.
            </p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {techniques.map((t) => (
                <span
                  key={t.id}
                  title={`${t.id} ${t.name}`.trim()}
                  className={
                    t.covered
                      ? `${chip} border-neutral-900/12 bg-neutral-900/5 text-neutral-900 dark:border-neutral-100/14 dark:bg-neutral-100/5 dark:text-neutral-100`
                      : `${chip} border-[oklch(60%_0.1_68)]/40 bg-transparent text-[oklch(49%_0.1_68)] dark:text-[oklch(80%_0.13_68)]`
                  }
                >
                  {t.id}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
