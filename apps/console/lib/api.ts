import { z } from "zod";

// All API access flows through the same-origin BFF proxy
// (app/api/proxy/[...path]/route.ts), which injects the service API key
// server-side — no credential ever reaches the browser.
const API_URL = "/api/proxy";

/* ── Schemas ─────────────────────────────────────────────────────────── */

export const SeveritySchema = z.enum([
  "critical",
  "high",
  "medium",
  "low",
  "info",
]);
export type Severity = z.infer<typeof SeveritySchema>;

export const AlertSchema = z.object({
  id: z.string(),
  title: z.string(),
  severity: SeveritySchema.catch("medium"),
  status: z.string().catch("open"),
  source: z.string().optional().default(""),
  description: z.string().optional().default(""),
  created_at: z.string().optional().default(""),
  case_id: z.string().optional().nullable(),
  // Optional enrichment blobs a backend may attach.
  enrichment: z.record(z.unknown()).optional(),
  verdict: z.string().optional(),
});
export type Alert = z.infer<typeof AlertSchema>;

export const CaseSchema = z.object({
  id: z.string(),
  title: z.string(),
  status: z.string().catch("open"),
  severity: SeveritySchema.catch("medium").optional(),
  assignee: z.string().optional().default(""),
  alert_ids: z.array(z.string()).optional().default([]),
  created_at: z.string().optional().default(""),
  updated_at: z.string().optional().default(""),
});
export type Case = z.infer<typeof CaseSchema>;

export const AuditEventSchema = z.object({
  id: z.string().optional(),
  ts: z.string().optional().default(""),
  actor: z.string().optional().default(""),
  agent: z.string().optional().default(""),
  tool: z.string().optional().default(""),
  action: z.string().optional().default(""),
  input: z.unknown().optional(),
  output: z.unknown().optional(),
  status: z.string().optional().default("done"),
  case_id: z.string().optional().nullable(),
  workflow_run_id: z.string().optional().nullable(),
  duration_ms: z.number().optional().nullable(),
});
export type AuditEvent = z.infer<typeof AuditEventSchema>;

export const PlanStepSchema = z.object({
  id: z.string().optional(),
  title: z.string(),
  detail: z.string().optional().default(""),
  status: z.string().optional().default("queued"),
  tool: z.string().optional().nullable(),
  estimate: z.string().optional().default(""),
  /** Inverse tool that undoes this action, when one was recorded. */
  rollback: z.string().optional().nullable().default(null),
});
export type PlanStep = z.infer<typeof PlanStepSchema>;

/** One row of the `response_actions` table behind `GET /plans/:case_id`. */
export const PlanActionSchema = z.object({
  id: z.string().optional(),
  case_id: z.string().optional().default(""),
  tool: z.string().optional().default(""),
  args: z.unknown().optional(),
  result: z
    .object({
      ok: z.boolean().optional(),
      error: z.string().nullable().optional(),
      output: z.unknown().optional(),
      rationale: z.string().optional(),
    })
    .passthrough()
    .nullable()
    .optional(),
  rollback_json: z.unknown().optional(),
  ts: z.string().optional().default(""),
  status: z.string().optional().default("queued"),
});
export type PlanAction = z.infer<typeof PlanActionSchema>;

export const PlaybookNodeSchema = z
  .object({
    id: z.string(),
    kind: z.string().optional().default("action"),
    label: z.string().optional(),
    tool: z.string().optional(),
    args: z.record(z.unknown()).optional(),
  })
  .passthrough();
export type PlaybookNode = z.infer<typeof PlaybookNodeSchema>;

export const PlaybookEdgeSchema = z
  .object({ from: z.string(), to: z.string() })
  .passthrough();
export type PlaybookEdge = z.infer<typeof PlaybookEdgeSchema>;

export const PlaybookSchema = z.object({
  name: z.string(),
  description: z.string().optional().default(""),
  trigger: z.string().optional().default(""),
  nodes: z.array(PlaybookNodeSchema).optional().default([]),
  edges: z.array(PlaybookEdgeSchema).optional().default([]),
});
export type Playbook = z.infer<typeof PlaybookSchema>;

export const MetricsSchema = z.object({
  alerts_total: z.number().optional(),
  alerts_open: z.number().optional(),
  cases_open: z.number().optional(),
  mttr_minutes: z.number().optional(),
  events_per_sec: z.number().optional(),
  agents_active: z.number().optional(),
  alerts_by_severity: z.record(z.number()).optional(),
  series: z.record(z.array(z.number())).optional(),
});
export type Metrics = z.infer<typeof MetricsSchema>;

export const CoverageSchema = z.object({
  tactics: z
    .array(
      z.object({
        id: z.string().optional(),
        name: z.string(),
        techniques: z
          .array(
            z.object({
              id: z.string(),
              name: z.string().optional().default(""),
              covered: z.boolean().optional().default(false),
            }),
          )
          .optional()
          .default([]),
      }),
    )
    .optional()
    .default([]),
  techniques: z
    .array(
      z.object({
        id: z.string(),
        name: z.string().optional().default(""),
        tactic: z.string().optional().default(""),
        covered: z.boolean().optional().default(false),
      }),
    )
    .optional()
    .default([]),
});
export type Coverage = z.infer<typeof CoverageSchema>;

export const ActivityEventSchema = z.object({
  workflow_run_id: z.string().optional().default(""),
  agent: z.string().optional().default(""),
  step: z.string().optional().default(""),
  detail: z.string().optional().default(""),
  ts: z.string().optional().default(""),
});
export type ActivityEvent = z.infer<typeof ActivityEventSchema>;

export const KillSwitchSchema = z.object({
  engaged: z.boolean(),
  reason: z.string().optional().default(""),
  updated_at: z.string().optional().default(""),
});
export type KillSwitch = z.infer<typeof KillSwitchSchema>;

export const HuntResponseSchema = z.object({
  answer: z.string().optional(),
  text: z.string().optional(),
  response: z.string().optional(),
});
export type HuntResponse = z.infer<typeof HuntResponseSchema>;

/* ── Vulnerability intel (NVD API 2.0 + CVSS engine) ──────────────────── */

export const CvssMetricSchema = z.object({
  version: z.string(),
  source: z.string().optional().default(""),
  kind: z.string().optional().default(""),
  vector: z.string(),
  base_score: z.number(),
  severity: z.string().optional().nullable(),
});
export type CvssMetric = z.infer<typeof CvssMetricSchema>;

export const CveReferenceSchema = z.object({
  url: z.string(),
  tags: z.array(z.string()).optional().default([]),
});

export const CveRecordSchema = z.object({
  id: z.string(),
  source_identifier: z.string().optional().nullable(),
  published: z.string().optional().nullable(),
  last_modified: z.string().optional().nullable(),
  vuln_status: z.string().optional().nullable(),
  description: z.string().optional().nullable(),
  metrics: z.array(CvssMetricSchema).optional().default([]),
  weaknesses: z.array(z.string()).optional().default([]),
  configurations: z.unknown().optional(),
  references: z.array(CveReferenceSchema).optional().default([]),
  has_kev: z.boolean().optional().default(false),
});
export type CveRecord = z.infer<typeof CveRecordSchema>;

export const CvePageSchema = z.object({
  total_results: z.number().optional().default(0),
  start_index: z.number().optional().default(0),
  results_per_page: z.number().optional().default(0),
  cves: z.array(CveRecordSchema).optional().default([]),
});
export type CvePage = z.infer<typeof CvePageSchema>;

export const VulnAdvisorySchema = z.object({
  cve_id: z.string(),
  first_seen: z.string().optional().nullable(),
  last_checked: z.string().optional().nullable(),
  severity: z.string().optional().nullable(),
  score: z.number().optional().nullable(),
  kev: z.boolean().optional().nullable(),
  summary: z.string().optional().nullable(),
  watchlist_match: z.string().optional().nullable(),
  cna: z.string().optional().nullable(),
  case_id: z.string().optional().nullable(),
  status: z.string().optional().nullable(),
});
export type VulnAdvisory = z.infer<typeof VulnAdvisorySchema>;

export const CvssScoreSchema = z.object({
  version: z.string(),
  vector: z.string(),
  score: z.number(),
  severity: z.string(),
  nomenclature: z.string().optional().default(""),
  metrics: z.record(z.string()).optional().default({}),
});
export type CvssScore = z.infer<typeof CvssScoreSchema>;

/* ── Fetch helpers ───────────────────────────────────────────────────── */

export class ApiError extends Error {
  status: number;
  /** Machine-readable class from the service envelope (`UPSTREAM_ERROR`, ...). */
  code?: string;
  constructor(status: number, message: string, code?: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

/** Reads `{error:{code,message}}` from a failed response; never throws. */
async function errorEnvelope(
  res: Response,
): Promise<{ code?: string; message?: string }> {
  const body = await res.json().catch(() => null);
  const err = body?.error;
  return {
    code: typeof err?.code === "string" ? err.code : undefined,
    message: typeof err?.message === "string" ? err.message : undefined,
  };
}

async function request<S extends z.ZodTypeAny>(
  path: string,
  schema: S,
  init?: RequestInit,
): Promise<z.output<S>> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: { "content-type": "application/json", ...init?.headers },
  });
  if (!res.ok) {
    const { code, message } = await errorEnvelope(res);
    throw new ApiError(
      res.status,
      message ? `${res.status} ${message}` : `${res.status} ${path}`,
      code,
    );
  }
  const json = await res.json().catch(() => null);
  const parsed = schema.safeParse(json);
  return parsed.success ? parsed.data : (json as z.output<S>);
}

/**
 * Accept either `[T, …]` or `{ "items": [T, …] }` list responses;
 * malformed entries are dropped rather than failing the whole list.
 */
const list = <S extends z.ZodTypeAny>(item: S) =>
  z.any().transform((v): z.output<S>[] => {
    const raw = Array.isArray(v)
      ? v
      : (v as { items?: unknown[] } | null)?.items;
    if (!Array.isArray(raw)) return [];
    return raw.flatMap((x) => {
      const p = item.safeParse(x);
      return p.success ? [p.data] : [];
    });
  });

const argsSummary = (v: unknown): string => {
  if (v == null) return "";
  if (typeof v !== "object") return String(v);
  const parts = Object.entries(v as Record<string, unknown>)
    .filter(([, x]) => ["string", "number", "boolean"].includes(typeof x))
    .map(([k, x]) => `${k}=${x}`);
  const s = parts.length ? parts.join(" ") : JSON.stringify(v);
  return s.length > 160 ? `${s.slice(0, 157)}…` : s;
};

const actionToPlanStep = (a: PlanAction): PlanStep => {
  const res = a.result as { ok?: boolean; error?: unknown } | null | undefined;
  const failed = res?.ok === false || (res?.ok == null && res?.error != null);
  const rb = a.rollback_json as { tool?: string } | null | undefined;
  return {
    id: a.id,
    title: a.tool || "response action",
    detail: argsSummary(a.args),
    status:
      failed || a.status === "failed"
        ? "failed"
        : a.status === "queued"
          ? "queued"
          : "done",
    tool: a.tool || null,
    estimate: a.ts.slice(0, 19).replace("T", " "),
    rollback: rb?.tool ?? null,
  };
};

export const api = {
  alerts: (opts?: { limit?: number; status?: string }) => {
    const q = new URLSearchParams();
    if (opts?.limit) q.set("limit", String(opts.limit));
    if (opts?.status) q.set("status", opts.status);
    const qs = q.toString();
    return request(`/alerts${qs ? `?${qs}` : ""}`, list(AlertSchema));
  },
  alert: (id: string) => request(`/alerts/${id}`, AlertSchema),
  cases: () => request("/cases", list(CaseSchema)),
  case: (id: string) => request(`/cases/${id}`, CaseSchema),
  audit: () => request("/audit", list(AuditEventSchema)),
  metrics: () => request("/metrics", MetricsSchema),
  coverage: () => request("/coverage", CoverageSchema),
  plan: (caseId: string) =>
    request(
      `/plans/${caseId}`,
      z.any().transform((v): PlanStep[] => {
        const raw = Array.isArray(v)
          ? v
          : (v as { actions?: unknown[] } | null)?.actions;
        if (!Array.isArray(raw)) return [];
        return raw.flatMap((x) => {
          const p = PlanActionSchema.safeParse(x);
          return p.success ? [actionToPlanStep(p.data)] : [];
        });
      }),
    ),
  planActions: (caseId: string) =>
    request(
      `/plans/${caseId}`,
      z.any().transform((v): PlanAction[] => {
        const raw = (v as { actions?: unknown[] } | null)?.actions;
        if (!Array.isArray(raw)) return [];
        return raw.flatMap((x) => {
          const p = PlanActionSchema.safeParse(x);
          return p.success ? [p.data] : [];
        });
      }),
    ),
  playbooks: () => request("/playbooks", list(PlaybookSchema)),
  playbook: (name: string) => request(`/playbooks/${name}`, PlaybookSchema),
  killSwitch: () => request("/controls/kill-switch", KillSwitchSchema),
  setKillSwitch: (engaged: boolean, reason?: string) =>
    request("/controls/kill-switch", KillSwitchSchema, {
      method: "POST",
      body: JSON.stringify({ engaged, reason: reason ?? "" }),
    }),
  hunt: async (prompt: string): Promise<string> => {
    const res = await fetch(`${API_URL}/hunt`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ prompt }),
    });
    if (!res.ok) {
      if (res.status === 404)
        return "Hunt endpoint is not available on this api-server build. Ask again once /hunt is wired.";
      throw new ApiError(res.status, `${res.status} /hunt`);
    }
    const json = await res.json().catch(() => null);
    const parsed = HuntResponseSchema.safeParse(json);
    if (parsed.success)
      return (
        parsed.data.answer ??
        parsed.data.text ??
        parsed.data.response ??
        JSON.stringify(json)
      );
    return typeof json === "string" ? json : JSON.stringify(json);
  },
  vulns: (opts?: {
    days?: number;
    keyword?: string;
    cvss_v4_severity?: string;
    has_kev?: boolean;
  }) => {
    const q = new URLSearchParams();
    if (opts?.days) q.set("days", String(opts.days));
    if (opts?.keyword) q.set("keyword", opts.keyword);
    if (opts?.cvss_v4_severity) q.set("cvss_v4_severity", opts.cvss_v4_severity);
    if (opts?.has_kev) q.set("has_kev", "true");
    const qs = q.toString();
    return request(`/vulns${qs ? `?${qs}` : ""}`, CvePageSchema);
  },
  vuln: (cveId: string) => request(`/vulns/${cveId}`, CveRecordSchema),
  vulnHistory: (cveId: string) =>
    request(`/vulns/${cveId}/history`, z.any()),
  vulnAdvisories: () =>
    request(
      "/vulns/advisories",
      z.any().transform((v): VulnAdvisory[] => {
        const raw = (v as { advisories?: unknown[] } | null)?.advisories;
        if (!Array.isArray(raw)) return [];
        return raw.flatMap((x) => {
          const p = VulnAdvisorySchema.safeParse(x);
          return p.success ? [p.data] : [];
        });
      }),
    ),
  cvssScore: (vector: string) =>
    request("/cvss/score", CvssScoreSchema, {
      method: "POST",
      body: JSON.stringify({ vector }),
    }),
};

/* ── SSE ─────────────────────────────────────────────────────────────── */

/**
 * Subscribe to an SSE endpoint. Returns a cleanup function.
 * Events are parsed through the supplied zod schema; malformed
 * frames are skipped.
 */
export function sse<S extends z.ZodTypeAny>(
  path: string,
  schema: S,
  onEvent: (event: z.output<S>) => void,
  onError?: (error: Event) => void,
): () => void {
  const source = new EventSource(`${API_URL}${path}`);
  source.onmessage = (frame) => {
    try {
      const parsed = schema.safeParse(JSON.parse(frame.data));
      if (parsed.success) onEvent(parsed.data);
    } catch {
      // skip malformed frames
    }
  };
  if (onError) source.onerror = onError;
  return () => source.close();
}
