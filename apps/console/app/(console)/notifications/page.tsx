"use client";

import Notifications1, {
  type Item,
} from "@/components/blocks/notifications-1";
import { EmptyState } from "@/components/empty-state";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const KIND: Record<string, Item["kind"]> = {
  critical: "security",
  high: "security",
};

export default function NotificationsPage() {
  const { data: alerts, loading } = useApi(() => api.alerts({ limit: 30 }));

  if (!loading && !alerts) {
    return (
      <EmptyState title="Notifications unavailable" hint="GET /alerts returned nothing." />
    );
  }

  const seed: Item[] = (alerts ?? []).map((a, i) => ({
    id: a.id || `n${i}`,
    kind: KIND[a.severity] ?? "review",
    actor: a.source || "detection",
    action: "raised",
    target: a.title,
    detail: a.description || undefined,
    time: a.created_at
      ? new Date(a.created_at).toLocaleTimeString()
      : "",
    group: "Today",
    unread: a.status === "open" || a.status === "new",
  }));

  if (loading) return null;

  return (
    <div className="h-full p-2">
      <Notifications1 seed={seed} />
    </div>
  );
}
