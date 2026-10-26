"use client";

import { useState } from "react";

import { CvssBadge } from "@/components/cvss-badge";
import { api, type CvssScore } from "@/lib/api";

const panel =
  "rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-4 dark:border-neutral-100/14 dark:bg-neutral-900";

const EXAMPLES = [
  "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H",
  "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
  "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H",
];

export default function CvssCalculatorPage() {
  const [vector, setVector] = useState(EXAMPLES[0]);
  const [result, setResult] = useState<CvssScore | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const score = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await api.cvssScore(vector.trim()));
    } catch (e) {
      setResult(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex max-w-3xl flex-col gap-2 p-2">
        <div className={panel}>
          <h1 className="mb-1 text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
            CVSS calculator
          </h1>
          <p className="mb-3 text-[12px] text-neutral-900/60 dark:text-neutral-100/60">
            Paste a CVSS v3.1 or v4.0 vector; scoring runs server-side on the
            FIRST-compliant engine (<code>POST /cvss/score</code>).
          </p>
          <textarea
            value={vector}
            onChange={(e) => setVector(e.target.value)}
            rows={3}
            spellCheck={false}
            className="w-full resize-y rounded-[var(--rb-r-sm,6px)] border border-neutral-900/10 bg-transparent p-2 font-mono text-[12px] text-neutral-900 outline-none focus:border-neutral-900/30 dark:border-neutral-100/14 dark:text-neutral-100"
            placeholder="CVSS:4.0/AV:N/AC:L/AT:N/..."
          />
          <div className="mt-2 flex items-center gap-2">
            <button
              onClick={score}
              disabled={busy || !vector.trim()}
              className="h-8 rounded-[var(--rb-r-sm,6px)] bg-neutral-900 px-3 text-[12px] font-[500] text-white disabled:opacity-50 dark:bg-neutral-100 dark:text-neutral-900"
            >
              {busy ? "Scoring…" : "Score vector"}
            </button>
            {EXAMPLES.map((ex) => (
              <button
                key={ex}
                onClick={() => setVector(ex)}
                className="h-8 rounded-[var(--rb-r-sm,6px)] border border-neutral-900/15 px-2 font-mono text-[10px] text-neutral-700 hover:bg-neutral-900/5 dark:border-neutral-100/20 dark:text-neutral-300 dark:hover:bg-neutral-100/10"
              >
                v{ex.split("/")[0].replace("CVSS:", "")}
              </button>
            ))}
          </div>
          {error && (
            <p className="mt-2 text-[12px] text-red-600 dark:text-red-400">
              {error}
            </p>
          )}
        </div>

        {result && (
          <div className={panel}>
            <div className="flex items-center gap-3">
              <span className="text-[32px]/[36px] font-[550] tabular-nums text-neutral-900 dark:text-neutral-100">
                {result.score.toFixed(1)}
              </span>
              <CvssBadge severity={result.severity} />
              <div className="ml-auto text-right text-[12px] text-neutral-900/60 dark:text-neutral-100/60">
                <div>v{result.version}</div>
                {result.nomenclature && <div>{result.nomenclature}</div>}
              </div>
            </div>
            <code className="mt-2 block break-all font-mono text-[11px] text-neutral-900/70 dark:text-neutral-100/70">
              {result.vector}
            </code>
            <div className="mt-3 grid grid-cols-2 gap-1 sm:grid-cols-4">
              {Object.entries(result.metrics).map(([k, v]) => (
                <div
                  key={k}
                  className="flex justify-between rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 px-2 py-1 font-mono text-[11px] dark:border-neutral-100/10"
                >
                  <span className="text-neutral-900/60 dark:text-neutral-100/60">
                    {k}
                  </span>
                  <span className="text-neutral-900 dark:text-neutral-100">
                    {v}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
