"use client";

import {
  Activity01Icon,
  AlertDiamondIcon,
  KanbanIcon,
  Timer01Icon,
} from "@hugeicons/core-free-icons";
import Dashboard1, {
  type Range,
} from "@/components/blocks/dashboard-1";
import Monitoring1, {
  type Incident,
  type Kpi,
} from "@/components/blocks/monitoring-1";
import Analytics1, {
  type Metric,
} from "@/components/blocks/analytics-1";
import { EmptyState } from "@/components/empty-state";
import { api, type Metrics } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const flat = [1, 1, 1, 1, 1, 1];

const SPARKS: Record<string, number[]> = {};

function dashboardRanges(m: Metrics): Range[] {
  const alertsSeries =
    m.series?.alerts ?? SPARKS.alerts ?? flat;
  const casesSeries = m.series?.cases ?? flat;
  const eventsSeries = m.series?.events ?? flat;
  return [
    {
      key: "Current",
      compare: "",
      months: alertsSeries.map((_, i) => `T-${alertsSeries.length - i}`),
      revenue: alertsSeries,
      metrics: [
        {
          label: "Open alerts",
          value: String(m.alerts_open ?? m.alerts_total ?? 0),
          delta: `${m.alerts_total ?? 0} total`,
          positive: false,
          spark: alertsSeries,
        },
        {
          label: "Open cases",
          value: String(m.cases_open ?? 0),
          delta: "active investigations",
          positive: true,
          spark: casesSeries,
        },
        {
          label: "Events / sec",
          value: String(m.events_per_sec ?? 0),
          delta: "ingestion rate",
          positive: true,
          spark: eventsSeries,
        },
        {
          label: "MTTR",
          value:
            m.mttr_minutes != null ? `${m.mttr_minutes}m` : "—",
          delta: "mean time to respond",
          positive: true,
          spark: flat,
        },
      ],
    },
  ];
}

export default function DashboardPage() {
  const { data: metrics, loading } = useApi(() => api.metrics());
  const { data: alerts } = useApi(() => api.alerts({ limit: 20 }));

  if (!loading && !metrics) {
    return <EmptyState title="Metrics unavailable" />;
  }

  const kpis: Kpi[] = [
    { id: "req", label: "Events", base: metrics?.events_per_sec ?? 0, drift: 0.05, unit: "" },
    { id: "err", label: "Open alerts", base: metrics?.alerts_open ?? 0, drift: 0.02, unit: "" },
    { id: "p95", label: "Open cases", base: metrics?.cases_open ?? 0, drift: 0.01, unit: "" },
    { id: "sat", label: "Active agents", base: metrics?.agents_active ?? 0, drift: 0.03, unit: "" },
  ];

  const incidents: Incident[] = (alerts ?? []).slice(0, 5).map((a) => ({
    id: a.id,
    title: a.title,
    severity: a.severity,
    tone:
      a.severity === "critical"
        ? "danger"
        : a.severity === "high" || a.severity === "medium"
          ? "warning"
          : "resolved",
    service: a.source || "unknown",
    at: a.created_at ? new Date(a.created_at).toLocaleTimeString() : "",
    body: a.description || "No description provided.",
  }));

  const analytics: Metric[] = [
    {
      id: "alerts",
      label: "Alerts",
      icon: AlertDiamondIcon,
      base: metrics?.alerts_total ?? 0,
      drift: 0.02,
      seed: 1.2,
    },
    {
      id: "cases",
      label: "Cases",
      icon: KanbanIcon,
      base: metrics?.cases_open ?? 0,
      drift: 0.01,
      seed: 3.4,
    },
    {
      id: "sessions",
      label: "Events / sec",
      icon: Activity01Icon,
      base: metrics?.events_per_sec ?? 0,
      drift: 0.05,
      seed: 5.1,
    },
    {
      id: "duration",
      label: "MTTR (s)",
      icon: Timer01Icon,
      base: (metrics?.mttr_minutes ?? 0) * 60,
      drift: -0.01,
      seed: 7.9,
    },
  ];

  const ranges = dashboardRanges(metrics ?? {});
  const yMax = Math.max(
    ...ranges[0].revenue.map((v) => v),
    10,
  );

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className="h-[640px] shrink-0">
          <Dashboard1
            ranges={ranges}
            title="Operations"
            chartTitle="Alerts per period"
            chartUnit="count"
            yMax={yMax}
          />
        </div>
        <div className="grid gap-2 xl:grid-cols-2">
          <div className="h-[560px]">
            <Monitoring1
              incidents={incidents}
              kpis={kpis}
              title="Signal health"
            />
          </div>
          <div className="h-[560px]">
            <Analytics1 metrics={analytics} />
          </div>
        </div>
      </div>
    </div>
  );
}
