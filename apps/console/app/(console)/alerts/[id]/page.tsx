"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import {
  Activity01Icon,
  AlertDiamondIcon,
  SecurityCheckIcon,
} from "@hugeicons/core-free-icons";
import Card1, { type Member } from "@/components/blocks/card-1";
import List1, { type Metric } from "@/components/blocks/list-1";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const initials = (s: string) =>
  s
    .split(/[\s:.\-_/]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase())
    .join("");

export default function AlertDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { data: alert, loading } = useApi(() => api.alert(id), [id]);

  if (!loading && !alert) {
    return (
      <EmptyState
        title="Alert not found"
        hint={`GET /alerts/${id} returned nothing.`}
      />
    );
  }

  const enrichment = alert?.enrichment ?? {};
  const entries = Object.entries(enrichment);

  const members: Member[] = entries.slice(0, 6).map(([key, value], i) => {
    const text = typeof value === "string" ? value : JSON.stringify(value);
    return {
      id: `e${i}`,
      name: text.length > 40 ? key : text,
      initials: initials(key) || "EN",
      role: key,
      location: "enrichment",
      online: i % 2 === 0,
      bio: typeof value === "string" ? value : JSON.stringify(value, null, 2),
    };
  });

  const metrics: Metric[] = [
    {
      id: "sev",
      label: "Risk score",
      icon: AlertDiamondIcon,
      caption: "Derived from alert severity",
      by: {
        "7d": {
          value: String(
            { critical: 95, high: 75, medium: 50, low: 25, info: 5 }[
              (alert?.severity as never) ?? "medium"
            ],
          ),
          delta: 0,
        },
        "28d": {
          value: String(
            { critical: 95, high: 75, medium: 50, low: 25, info: 5 }[
              (alert?.severity as never) ?? "medium"
            ],
          ),
          delta: 0,
        },
        qtd: {
          value: String(
            { critical: 95, high: 75, medium: 50, low: 25, info: 5 }[
              (alert?.severity as never) ?? "medium"
            ],
          ),
          delta: 0,
        },
      },
    },
    {
      id: "enrich",
      label: "Enrichment fields",
      icon: Activity01Icon,
      caption: "Keys attached by enrichment-lib",
      by: {
        "7d": { value: String(entries.length), delta: 0 },
        "28d": { value: String(entries.length), delta: 0 },
        qtd: { value: String(entries.length), delta: 0 },
      },
    },
    {
      id: "verdict",
      label: "Verdict",
      icon: SecurityCheckIcon,
      caption: "Triage verdict for this alert",
      by: {
        "7d": { value: alert?.verdict ?? "Pending", delta: 0 },
        "28d": { value: alert?.verdict ?? "Pending", delta: 0 },
        qtd: { value: alert?.verdict ?? "Pending", delta: 0 },
      },
    },
  ];

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className="rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-5 dark:border-neutral-100/14 dark:bg-neutral-900">
          <Link
            href="/alerts"
            className="text-[12px] text-neutral-900/66 hover:text-neutral-900 dark:text-neutral-100/66 dark:hover:text-neutral-100"
          >
            ← Alert queue
          </Link>
          <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <h1 className="text-[18px] font-medium text-neutral-900 dark:text-neutral-100">
              {alert?.title ?? "…"}
            </h1>
            <span className="text-[13px] capitalize text-neutral-900/66 dark:text-neutral-100/66">
              {alert?.severity} · {alert?.status} · {alert?.source}
            </span>
          </div>
          {alert?.description && (
            <p className="mt-2 max-w-prose text-[13px] text-neutral-900/80 dark:text-neutral-100/80">
              {alert.description}
            </p>
          )}
          {alert?.verdict && (
            <div className="mt-3 inline-flex items-center gap-2 rounded-[var(--rb-r-sm,6px)] border border-neutral-900/10 px-3 py-1.5 text-[13px] dark:border-neutral-100/14">
              <span className="font-medium">Verdict:</span>
              <span>{alert.verdict}</span>
            </div>
          )}
        </div>

        <div className="grid gap-2 xl:grid-cols-2">
          <div className="h-[560px]">
            <Card1 members={members.length ? members : undefined} />
          </div>
          <div className="h-[560px]">
            <List1
              metrics={metrics}
              title="Alert context"
              caption="Values repeat across ranges"
            />
          </div>
        </div>
      </div>
    </div>
  );
}
