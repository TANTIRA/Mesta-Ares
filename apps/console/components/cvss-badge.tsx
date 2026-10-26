const TONE: Record<string, string> = {
  critical:
    "border-red-600/30 bg-red-600/10 text-red-700 dark:text-red-300",
  high: "border-orange-600/30 bg-orange-600/10 text-orange-700 dark:text-orange-300",
  medium:
    "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300",
  low: "border-sky-600/30 bg-sky-600/10 text-sky-700 dark:text-sky-300",
  none: "border-neutral-500/20 bg-neutral-500/10 text-neutral-600 dark:text-neutral-300",
};

/** CVSS severity chip (None/Low/Medium/High/Critical). */
export function CvssBadge({
  severity,
  score,
}: {
  severity?: string | null;
  score?: number | null;
}) {
  const sev = (severity ?? "").toLowerCase() || "none";
  const tone = TONE[sev] ?? TONE.none;
  return (
    <span
      className={`inline-flex h-6 items-center gap-1 rounded-[var(--rb-r-sm,6px)] border px-2 text-[11px]/[14px] font-[500] ${tone}`}
    >
      {score != null && <span className="tabular-nums">{score.toFixed(1)}</span>}
      <span className="capitalize">{sev}</span>
    </span>
  );
}
