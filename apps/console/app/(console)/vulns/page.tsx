"use client";

import Link from "next/link";
import { useState } from "react";

import { CvssBadge } from "@/components/cvss-badge";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const panel =
  "rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-4 dark:border-neutral-100/14 dark:bg-neutral-900";
const input =
  "h-8 rounded-[var(--rb-r-sm,6px)] border border-neutral-900/10 bg-transparent px-2 text-[13px] text-neutral-900 outline-none focus:border-neutral-900/30 dark:border-neutral-100/14 dark:text-neutral-100";

export default function VulnsPage() {
  const [days, setDays] = useState(1);
  const [keyword, setKeyword] = useState("");
  const [severity, setSeverity] = useState("");
  const [kevOnly, setKevOnly] = useState(false);
  const [submitted, setSubmitted] = useState(0);

  const { data: page, loading } = useApi(
    () =>
      api.vulns({
        days,
        keyword: keyword || undefined,
        cvss_v4_severity: severity || undefined,
        has_kev: kevOnly || undefined,
      }),
    [submitted],
  );
  const { data: advisories } = useApi(() => api.vulnAdvisories());

  const cves = page?.cves ?? [];

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className={panel}>
          <div className="mb-3 flex items-baseline justify-between">
            <h2 className="text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
              Vulnerability advisories
            </h2>
            <span className="text-[12px] text-neutral-900/66 dark:text-neutral-100/66">
              vuln-agent · watchlist + KEV
            </span>
          </div>
          {(advisories ?? []).length === 0 ? (
            <p className="text-[13px] text-neutral-900/66 dark:text-neutral-100/66">
              No advisories recorded yet. vuln-agent writes here as new NVD
              publications match the watchlist, KEV, or severity floor.
            </p>
          ) : (
            <table className="w-full text-left text-[13px]">
              <thead>
                <tr className="text-[11px] uppercase tracking-wide text-neutral-900/50 dark:text-neutral-100/50">
                  <th className="pb-2 font-[450]">CVE</th>
                  <th className="pb-2 font-[450]">Severity</th>
                  <th className="pb-2 font-[450]">KEV</th>
                  <th className="pb-2 font-[450]">Watchlist</th>
                  <th className="pb-2 font-[450]">CNA</th>
                  <th className="pb-2 font-[450]">Status</th>
                  <th className="pb-2 font-[450]">First seen</th>
                </tr>
              </thead>
              <tbody>
                {(advisories ?? []).map((a) => (
                  <tr
                    key={a.cve_id}
                    className="border-t border-neutral-900/5 dark:border-neutral-100/8"
                  >
                    <td className="py-1.5 font-mono text-[12px]">
                      <Link
                        href={`/vulns/${a.cve_id}`}
                        className="text-sky-700 hover:underline dark:text-sky-300"
                      >
                        {a.cve_id}
                      </Link>
                    </td>
                    <td className="py-1.5">
                      <CvssBadge severity={a.severity} score={a.score} />
                    </td>
                    <td className="py-1.5">{a.kev ? "yes" : ""}</td>
                    <td className="py-1.5">{a.watchlist_match ?? ""}</td>
                    <td className="py-1.5 text-neutral-900/66 dark:text-neutral-100/66">
                      {a.cna ?? ""}
                    </td>
                    <td className="py-1.5">{a.status ?? "new"}</td>
                    <td className="py-1.5 text-neutral-900/66 dark:text-neutral-100/66">
                      {a.first_seen?.slice(0, 19).replace("T", " ") ?? ""}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className={panel}>
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <h2 className="mr-auto text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
              NVD feed
            </h2>
            <select
              value={days}
              onChange={(e) => setDays(Number(e.target.value))}
              className={input}
              aria-label="Look-back window"
            >
              <option value={1}>Last 24h</option>
              <option value={7}>Last 7d</option>
              <option value={30}>Last 30d</option>
            </select>
            <select
              value={severity}
              onChange={(e) => setSeverity(e.target.value)}
              className={input}
              aria-label="CVSS v4 severity"
            >
              <option value="">Any severity</option>
              <option value="LOW">Low+</option>
              <option value="MEDIUM">Medium+</option>
              <option value="HIGH">High+</option>
              <option value="CRITICAL">Critical</option>
            </select>
            <input
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
              placeholder="keyword (e.g. opensearch)"
              className={`${input} w-52`}
            />
            <label className="flex items-center gap-1.5 text-[12px] text-neutral-900/66 dark:text-neutral-100/66">
              <input
                type="checkbox"
                checked={kevOnly}
                onChange={(e) => setKevOnly(e.target.checked)}
              />
              KEV only
            </label>
            <button
              onClick={() => setSubmitted((n) => n + 1)}
              className="h-8 rounded-[var(--rb-r-sm,6px)] border border-neutral-900/15 px-3 text-[12px] font-[500] text-neutral-900 hover:bg-neutral-900/5 dark:border-neutral-100/20 dark:text-neutral-100 dark:hover:bg-neutral-100/10"
            >
              Query
            </button>
          </div>
          {!loading && cves.length === 0 ? (
            <EmptyState
              title="No CVEs in window"
              hint="Widen the window or drop filters."
            />
          ) : (
            <div className="flex flex-col divide-y divide-neutral-900/5 dark:divide-neutral-100/8">
              {cves.map((c) => {
                const best = [...(c.metrics ?? [])].sort(
                  (a, b) => b.base_score - a.base_score,
                )[0];
                return (
                  <Link
                    key={c.id}
                    href={`/vulns/${c.id}`}
                    className="flex items-start gap-3 py-2 hover:bg-neutral-900/[0.02] dark:hover:bg-neutral-100/[0.04]"
                  >
                    <span className="w-32 shrink-0 font-mono text-[12px] text-sky-700 dark:text-sky-300">
                      {c.id}
                    </span>
                    <CvssBadge
                      severity={best?.severity}
                      score={best?.base_score}
                    />
                    <span className="line-clamp-1 flex-1 text-[13px] text-neutral-900/80 dark:text-neutral-100/80">
                      {c.description ?? ""}
                    </span>
                    {c.has_kev && (
                      <span className="shrink-0 rounded border border-red-600/40 bg-red-600/10 px-1.5 text-[10px] font-[600] text-red-700 dark:text-red-300">
                        KEV
                      </span>
                    )}
                  </Link>
                );
              })}
            </div>
          )}
          <p className="mt-2 text-[11px] text-neutral-900/50 dark:text-neutral-100/50">
            {page?.total_results ?? 0} total in NVD for this window
          </p>
        </div>
      </div>
    </div>
  );
}
