"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import AgentPlan1 from "@/components/blocks/agent-plan-1";
import ToolCalls1 from "@/components/blocks/tool-calls-1";
import AgentApproval1 from "@/components/blocks/agent-approval-1";
import PlanGraph from "@/components/plan-graph";
import { EmptyState } from "@/components/empty-state";
import {
  api,
  sse,
  ActivityEventSchema,
  type PlanStep,
} from "@/lib/api";
import { useApi } from "@/lib/use-api";
import { auditToCall, planCurrentIndex, planToStep } from "@/lib/map";

export default function CaseDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { data: caseData, loading } = useApi(() => api.case(id), [id]);
  const [plan, setPlan] = useState<PlanStep[] | null>(null);
  const { data: audit } = useApi(() => api.audit(), []);
  const runIds = useRef(new Set<string>());

  // Load the plan once, then refetch whenever the SSE activity stream
  // reports progress on this case's workflow run. The run id is learned
  // from the response agent's `received: case <id>` event; later events
  // for the run carry the same `workflow_run_id`.
  useEffect(() => {
    let alive = true;
    const refresh = () =>
      api
        .plan(id)
        .then((d) => alive && setPlan(d))
        .catch(() => {});
    refresh();
    const close = sse("/stream/activity", ActivityEventSchema, (e) => {
      const known = runIds.current;
      if (e.workflow_run_id && known.has(e.workflow_run_id)) {
        refresh();
      } else if (e.agent === "response" && e.detail.includes(id)) {
        if (e.workflow_run_id) known.add(e.workflow_run_id);
        refresh();
      }
    });
    return () => {
      alive = false;
      close();
    };
  }, [id]);

  if (!loading && !caseData) {
    return (
      <EmptyState
        title="Case not found"
        hint={`GET /cases/${id} returned nothing.`}
      />
    );
  }

  const calls = (audit ?? [])
    .filter((e) => !e.case_id || e.case_id === id)
    .map(auditToCall);

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className="rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-5 dark:border-neutral-100/14 dark:bg-neutral-900">
          <Link
            href="/cases"
            className="text-[12px] text-neutral-900/66 hover:text-neutral-900 dark:text-neutral-100/66 dark:hover:text-neutral-100"
          >
            ← Case board
          </Link>
          <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <h1 className="text-[18px] font-medium text-neutral-900 dark:text-neutral-100">
              {caseData?.title ?? "…"}
            </h1>
            <span className="text-[13px] capitalize text-neutral-900/66 dark:text-neutral-100/66">
              {caseData?.status}
              {caseData?.assignee ? ` · ${caseData.assignee}` : ""}
            </span>
          </div>
        </div>

        <div className="grid gap-2 xl:grid-cols-2">
          <div className="h-[560px]">
            <AgentPlan1
              steps={(plan ?? []).map(planToStep)}
              current={planCurrentIndex(plan ?? [])}
              title={plan ? "Response plan" : undefined}
            />
          </div>
          <div className="h-[560px]">
            <ToolCalls1
              settled={calls}
              showLive={false}
              title="Tool calls in this case"
            />
          </div>
        </div>

        <div className="h-[420px] shrink-0">
          <PlanGraph steps={plan ?? []} title="Response plan graph" />
        </div>

        <div className="h-[480px] shrink-0">
          <AgentApproval1
            prompt={`Case ${id}: approve the containment actions proposed by the response agent.`}
            rationale="The response plan above includes irreversible containment steps. Review the plan and the audited tool calls, then confirm or reject the actions before the agent proceeds."
            question={`Run the proposed containment for case ${id}?`}
            summary="Executes the queued plan steps against the affected assets."
            requestArgs={JSON.stringify({ case_id: id, steps: (plan ?? []).map((s) => s.title) }, null, 2)}
            approveLabel="Approve containment"
            approvedText="Containment approved and queued"
            approvedNote="The response agent will execute the queued steps and log each call to /audit."
          />
        </div>
      </div>
    </div>
  );
}
