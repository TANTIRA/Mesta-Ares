"use client";

import { useParams } from "next/navigation";

import { CvssBadge } from "@/components/cvss-badge";
import { EmptyState } from "@/components/empty-state";
import { api, type CveRecord } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const panel =
  "rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-4 dark:border-neutral-100/14 dark:bg-neutral-900";
const label = "text-[11px] uppercase tracking-wide text-neutral-900/50 dark:text-neutral-100/50";
const chip =
  "inline-flex h-6 items-center rounded-[var(--rb-r-sm,6px)] border border-neutral-900/15 px-2 font-mono text-[11px] text-neutral-800 dark:border-neutral-100/20 dark:text-neutral-200";

/** Flatten NVD CPE applicability tree into `criteria` strings. */
function cpeCriteria(rec: CveRecord): string[] {
  const out = new Set<string>();
  const walk = (nodes: unknown) => {
    if (!Array.isArray(nodes)) return;
    for (const n of nodes as { criteria?: unknown[]; children?: unknown[] }[]) {
      for (const c of n?.criteria ?? []) {
        const crit = (c as { criteria?: string }).criteria;
        if (typeof crit === "string") out.add(crit);
      }
      walk(n?.children);
    }
  };
  const cfgs = rec.configurations as { nodes?: unknown[] }[] | undefined;
  if (Array.isArray(cfgs)) for (const c of cfgs) walk(c?.nodes);
  return [...out];
}

interface HistoryEvent {
  created?: string;
  cveId?: string;
  change?: { events?: { type?: string; details?: unknown[] }[] };
}

export default function VulnDetailPage() {
  const { cveId } = useParams<{ cveId: string }>();
  const id = decodeURIComponent(cveId ?? "").toUpperCase();
  const { data: cve, loading, error } = useApi(() => api.vuln(id), [id]);
  const { data: history } = useApi(() => api.vulnHistory(id), [id]);

  if (!loading && !cve) {
    return (
      <EmptyState
        title={`${id} — no NVD record`}
        hint={error ? String(error) : "Unknown, reserved, or rejected ID."}
      />
    );
  }
  if (!cve) return <div className="p-4 text-[13px]">Loading {id}…</div>;

  const versions = [...new Set(cve.metrics.map((m) => m.version))].sort(
    (a, b) => Number(b) - Number(a),
  );
  const cpes = cpeCriteria(cve);
  const changes =
    (history as { cveChanges?: HistoryEvent[] } | null)?.cveChanges ?? [];

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className={panel}>
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="font-mono text-[16px] font-[500] text-neutral-900 dark:text-neutral-100">
              {cve.id}
            </h1>
            {cve.has_kev && (
              <span className="rounded border border-red-600/40 bg-red-600/10 px-1.5 py-0.5 text-[10px] font-[600] text-red-700 dark:text-red-300">
                CISA KEV
              </span>
            )}
            {cve.vuln_status && <span className={chip}>{cve.vuln_status}</span>}
          </div>
          <dl className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div>
              <dt className={label}>Assigning CNA</dt>
              <dd className="text-[13px] text-neutral-900 dark:text-neutral-100">
                {cve.source_identifier ?? "—"}
              </dd>
            </div>
            <div>
              <dt className={label}>Published</dt>
              <dd className="text-[13px] text-neutral-900 dark:text-neutral-100">
                {cve.published?.slice(0, 10) ?? "—"}
              </dd>
            </div>
            <div>
              <dt className={label}>Last modified</dt>
              <dd className="text-[13px] text-neutral-900 dark:text-neutral-100">
                {cve.last_modified?.slice(0, 10) ?? "—"}
              </dd>
            </div>
            <div>
              <dt className={label}>References</dt>
              <dd className="text-[13px] text-neutral-900 dark:text-neutral-100">
                {cve.references.length}
              </dd>
            </div>
          </dl>
          <p className="mt-3 text-[13px] leading-relaxed text-neutral-900/85 dark:text-neutral-100/85">
            {cve.description ?? "No description published."}
          </p>
        </div>

        <div className={panel}>
          <h2 className={`${label} mb-3`}>CVSS metrics</h2>
          {cve.metrics.length === 0 ? (
            <p className="text-[13px] text-neutral-900/66 dark:text-neutral-100/66">
              No CVSS metrics published for this CVE yet.
            </p>
          ) : (
            versions.map((v) => (
              <div key={v} className="mb-3 last:mb-0">
                <div className="mb-1 text-[12px] font-[500] text-neutral-900/70 dark:text-neutral-100/70">
                  v{v}
                </div>
                {cve.metrics
                  .filter((m) => m.version === v)
                  .map((m, i) => (
                    <div
                      key={i}
                      className="mb-1 flex flex-wrap items-center gap-2 rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 p-2 dark:border-neutral-100/10"
                    >
                      <CvssBadge severity={m.severity} score={m.base_score} />
                      <span className={chip}>{m.kind || "Primary"}</span>
                      <span className="text-[12px] text-neutral-900/66 dark:text-neutral-100/66">
                        {m.source}
                      </span>
                      <code className="w-full break-all font-mono text-[11px] text-neutral-900/80 dark:text-neutral-100/80">
                        {m.vector}
                      </code>
                    </div>
                  ))}
              </div>
            ))
          )}
        </div>

        {cve.weaknesses.length > 0 && (
          <div className={panel}>
            <h2 className={`${label} mb-2`}>Weaknesses</h2>
            <div className="flex flex-wrap gap-1.5">
              {cve.weaknesses.map((w) => (
                <span key={w} className={chip}>
                  {w}
                </span>
              ))}
            </div>
          </div>
        )}

        {cpes.length > 0 && (
          <div className={panel}>
            <h2 className={`${label} mb-2`}>
              Affected configurations ({cpes.length})
            </h2>
            <div className="flex flex-col gap-1">
              {cpes.slice(0, 50).map((c) => (
                <code
                  key={c}
                  className="break-all font-mono text-[11px] text-neutral-900/75 dark:text-neutral-100/75"
                >
                  {c}
                </code>
              ))}
              {cpes.length > 50 && (
                <span className="text-[12px] text-neutral-900/50 dark:text-neutral-100/50">
                  … {cpes.length - 50} more
                </span>
              )}
            </div>
          </div>
        )}

        <div className={panel}>
          <h2 className={`${label} mb-2`}>References</h2>
          <ul className="flex flex-col gap-1">
            {cve.references.map((r) => (
              <li key={r.url} className="flex items-baseline gap-2">
                <a
                  href={r.url}
                  target="_blank"
                  rel="noreferrer"
                  className="truncate text-[12px] text-sky-700 hover:underline dark:text-sky-300"
                >
                  {r.url}
                </a>
                {r.tags.map((t) => (
                  <span key={t} className={chip}>
                    {t}
                  </span>
                ))}
              </li>
            ))}
          </ul>
        </div>

        <div className={panel}>
          <h2 className={`${label} mb-2`}>Change history</h2>
          {changes.length === 0 ? (
            <p className="text-[13px] text-neutral-900/66 dark:text-neutral-100/66">
              No recorded changes.
            </p>
          ) : (
            changes.map((c, i) => (
              <div key={i} className="mb-2 text-[12px] last:mb-0">
                <span className="text-neutral-900/60 dark:text-neutral-100/60">
                  {c.created?.slice(0, 16).replace("T", " ")}
                </span>
                <ul className="mt-0.5 list-inside list-disc">
                  {(c.change?.events ?? []).map((e, j) => (
                    <li key={j} className="text-neutral-900/80 dark:text-neutral-100/80">
                      {e.type}
                    </li>
                  ))}
                </ul>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
