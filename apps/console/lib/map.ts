import type { Alert, AuditEvent, Case, PlanStep } from "@/lib/api";
import type { AlertRow } from "@/components/blocks/data-table-1";
import type { FacetAlert } from "@/components/blocks/filtering-1";
import type { BoardCard } from "@/components/blocks/kanban-1";
import type { ToolCall } from "@/components/blocks/tool-calls-1";
import type { Step as PlanBlockStep } from "@/components/blocks/agent-plan-1";

const SEVERITIES = ["critical", "high", "medium", "low", "info"] as const;
type Sev = (typeof SEVERITIES)[number];

const sev = (s: string | undefined): Sev =>
  (SEVERITIES as readonly string[]).includes(s ?? "")
    ? (s as Sev)
    : "medium";

export const alertToRow = (a: Alert): AlertRow => ({
  id: a.id,
  title: a.title,
  severity: sev(a.severity),
  status: a.status,
  source: a.source || "unknown",
  time: a.created_at,
});

const score = (s: Sev) =>
  ({ critical: 95, high: 75, medium: 50, low: 25, info: 5 })[s];

export const alertToFacet = (a: Alert): FacetAlert => ({
  id: a.id,
  title: a.title,
  severity: sev(a.severity),
  status: a.status || "open",
  source: a.source || "unknown",
  score: score(sev(a.severity)),
  time: a.created_at,
});

const PRIORITY: Record<Sev, BoardCard["priority"]> = {
  critical: "High",
  high: "High",
  medium: "Medium",
  low: "Low",
  info: "Low",
};

const COLUMN: Record<string, BoardCard["column"]> = {
  open: "backlog",
  new: "backlog",
  triage: "backlog",
  triaged: "backlog",
  investigating: "progress",
  in_progress: "progress",
  containment: "review",
  response: "review",
  review: "review",
  resolved: "done",
  closed: "done",
  done: "done",
};

export const caseToCard = (c: Case): BoardCard => ({
  id: c.id,
  ref: c.id.slice(0, 8).toUpperCase(),
  title: c.title,
  tag: c.severity ? c.severity : "case",
  assignee: c.assignee || "Unassigned",
  comments: c.alert_ids.length,
  files: 0,
  priority: PRIORITY[sev(c.severity)],
  column: COLUMN[c.status] ?? "backlog",
});

const CALL_STATUS: Record<string, ToolCall["status"]> = {
  done: "done",
  ok: "done",
  success: "done",
  failed: "failed",
  error: "failed",
  running: "running",
  streaming: "streaming",
};

const pretty = (v: unknown) =>
  v === undefined || v === null
    ? undefined
    : typeof v === "string"
      ? v
      : JSON.stringify(v, null, 2);

export const auditToCall = (e: AuditEvent, i: number): ToolCall => ({
  id: e.id ?? `audit-${i}`,
  name: e.tool || e.action || e.agent || "event",
  status: CALL_STATUS[e.status] ?? "done",
  input: pretty(e.input),
  output: pretty(e.output),
  startedAt: e.ts || undefined,
  duration:
    e.duration_ms != null ? `${Math.round(e.duration_ms)}ms` : undefined,
});

const PLAN_STATE: Record<
  string,
  "done" | "running" | "paused" | "queued" | "skipped"
> = {
  done: "done",
  complete: "done",
  success: "done",
  running: "running",
  in_progress: "running",
  paused: "paused",
  queued: "queued",
  pending: "queued",
  skipped: "skipped",
  failed: "skipped",
  error: "skipped",
};

export const planToStep = (s: PlanStep): PlanBlockStep => ({
  title: s.title,
  detail: s.detail,
  target: s.tool ?? null,
  estimate: s.estimate,
  skipped: PLAN_STATE[s.status] === "skipped" ? true : undefined,
});

export const planCurrentIndex = (steps: PlanStep[]): number => {
  const i = steps.findIndex(
    (s) => PLAN_STATE[s.status] === "running" || PLAN_STATE[s.status] === "paused",
  );
  return i === -1 ? steps.length : i;
};
