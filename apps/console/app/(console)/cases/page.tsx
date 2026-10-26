"use client";

import { useRouter } from "next/navigation";
import Kanban1 from "@/components/blocks/kanban-1";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";
import { caseToCard } from "@/lib/map";

export default function CasesPage() {
  const router = useRouter();
  const { data: cases, loading } = useApi(() => api.cases());

  if (!loading && !cases) {
    return <EmptyState title="Cases unavailable" hint="GET /cases returned nothing." />;
  }

  return (
    <div className="h-full p-2">
      <Kanban1
        cases={(cases ?? []).map(caseToCard)}
        onOpen={(id) => router.push(`/cases/${id}`)}
        title="Case board"
      />
    </div>
  );
}
