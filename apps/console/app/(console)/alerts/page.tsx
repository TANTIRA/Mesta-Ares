"use client";

import { useRouter } from "next/navigation";
import DataTable1 from "@/components/blocks/data-table-1";
import Filtering1 from "@/components/blocks/filtering-1";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";
import { alertToFacet, alertToRow } from "@/lib/map";

export default function AlertsPage() {
  const router = useRouter();
  const { data: alerts, loading } = useApi(() => api.alerts({ limit: 200 }));
  const open = (id: string) => router.push(`/alerts/${id}`);

  if (!loading && !alerts) {
    return <EmptyState title="Alerts unavailable" hint="GET /alerts returned nothing. Check the api-server." />;
  }

  const rows = (alerts ?? []).map(alertToRow);
  const facets = (alerts ?? []).map(alertToFacet);

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div className="h-[640px] shrink-0">
          <DataTable1 alerts={rows} onOpen={open} />
        </div>
        <div className="shrink-0">
          <Filtering1 alerts={facets} onOpen={open} />
        </div>
      </div>
    </div>
  );
}
