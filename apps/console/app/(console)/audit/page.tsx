"use client";

import ToolCalls1 from "@/components/blocks/tool-calls-1";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";
import { auditToCall } from "@/lib/map";

export default function AuditPage() {
  const { data: audit, loading } = useApi(() => api.audit());

  if (!loading && !audit) {
    return <EmptyState title="Audit log unavailable" hint="GET /audit returned nothing." />;
  }

  return (
    <div className="h-full p-2">
      <ToolCalls1
        settled={(audit ?? []).map(auditToCall)}
        showLive={false}
        title="Audit log"
      />
    </div>
  );
}
