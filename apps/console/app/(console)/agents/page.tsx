"use client";

import { useEffect, useRef, useState } from "react";
import AgentActivity1, {
  type Step,
} from "@/components/blocks/agent-activity-1";
import { api, ActivityEventSchema, sse } from "@/lib/api";
import { useApi } from "@/lib/use-api";

export default function AgentsPage() {
  const [events, setEvents] = useState<Step[]>([]);
  const closeRef = useRef<(() => void) | null>(null);
  const { data: backlog } = useApi(() => api.audit());

  useEffect(() => {
    closeRef.current = sse("/stream/activity", ActivityEventSchema, (e) => {
      setEvents((prev) => [
        ...prev.map((s) =>
          s.state === "running" ? { ...s, state: "done" as const } : s,
        ),
        {
          label: e.detail
            ? `${e.agent}: ${e.step} — ${e.detail}`
            : `${e.agent}: ${e.step}`,
          state: "running",
        },
      ]);
    });
    return () => closeRef.current?.();
  }, []);

  const steps: Step[] =
    events.length > 0
      ? events.slice(-30)
      : (backlog ?? []).slice(-12).map((e) => ({
          label: `${e.agent || e.actor}: ${e.tool || e.action}`,
          state: "done" as const,
        }));

  return (
    <div className="h-full p-2">
      <AgentActivity1
        steps={steps.length ? steps : [{ label: "Waiting for agent activity…", state: "queued" }]}
        title="Live agent activity"
        onStop={() => closeRef.current?.()}
      />
    </div>
  );
}
