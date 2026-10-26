"use client";

import { useMemo, useState } from "react";
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  Panel,
  Handle,
  Position,
  MarkerType,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import dagre from "dagre";
import { Zap, Wrench, GitFork, Flag } from "lucide-react";

import { EmptyState } from "@/components/empty-state";
import { api, type Playbook, type PlaybookNode } from "@/lib/api";
import { useApi } from "@/lib/use-api";

/* ── Custom node ─────────────────────────────────────────────────────── */

type PlaybookFlowNode = Node<PlaybookNode, "playbook">;

const KIND_ICON = {
  trigger: Zap,
  action: Wrench,
  condition: GitFork,
  end: Flag,
} as const;

/** One playbook step: kind icon, label/tool and a single-line args preview. */
function PlaybookNodeView({ data }: NodeProps<PlaybookFlowNode>) {
  const Icon = KIND_ICON[data.kind as keyof typeof KIND_ICON] ?? Wrench;
  const argsText = data.args ? JSON.stringify(data.args) : null;
  return (
    <div className="w-[220px] rounded-[var(--rb-r-sm,6px)] border border-neutral-900/12 bg-white px-3 py-2 shadow-sm dark:border-neutral-100/14 dark:bg-neutral-900">
      <Handle
        type="target"
        position={Position.Left}
        className="!size-1.5 !border-0 !bg-neutral-400"
      />
      <div className="flex items-center gap-1.5 text-[12px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
        <Icon className="size-3.5 shrink-0" />
        <span className="truncate">{data.label ?? data.tool ?? data.id}</span>
      </div>
      {data.tool ? (
        <div className="mt-0.5 truncate font-mono text-[11px]/[14px] text-neutral-900/55 dark:text-neutral-100/55">
          {data.tool}
        </div>
      ) : null}
      {argsText ? (
        <div
          title={argsText}
          className="mt-0.5 truncate text-[10px]/[13px] text-neutral-900/40 dark:text-neutral-100/40"
        >
          {argsText}
        </div>
      ) : null}
      <Handle
        type="source"
        position={Position.Right}
        className="!size-1.5 !border-0 !bg-neutral-400"
      />
    </div>
  );
}

const nodeTypes = { playbook: PlaybookNodeView };

/* ── Dagre layout (left → right) ─────────────────────────────────────── */

const NODE_W = 220;
const NODE_H = 76;

function layoutLR(
  nodes: PlaybookFlowNode[],
  edges: Edge[],
): PlaybookFlowNode[] {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", nodesep: 36, ranksep: 90, marginx: 20, marginy: 20 });
  for (const n of nodes) g.setNode(n.id, { width: NODE_W, height: NODE_H });
  for (const e of edges) g.setEdge(e.source, e.target);
  dagre.layout(g);
  return nodes.map((n) => {
    const p = g.node(n.id);
    return { ...n, position: { x: p.x - NODE_W / 2, y: p.y - NODE_H / 2 } };
  });
}

function toFlow(pb: Playbook): { nodes: PlaybookFlowNode[]; edges: Edge[] } {
  const nodes: PlaybookFlowNode[] = pb.nodes.map((n) => ({
    id: n.id,
    type: "playbook" as const,
    position: { x: 0, y: 0 },
    data: n,
  }));
  const edges: Edge[] = pb.edges.map((e, i) => ({
    id: `e${i}-${e.from}-${e.to}`,
    source: e.from,
    target: e.to,
    markerEnd: { type: MarkerType.ArrowClosed },
  }));
  return { nodes: layoutLR(nodes, edges), edges };
}

/* ── Page ────────────────────────────────────────────────────────────── */

export default function PlaybooksPage() {
  const { data: playbooks, loading } = useApi(() => api.playbooks());
  const [selected, setSelected] = useState<string | null>(null);

  const pb =
    playbooks?.find((p) => p.name === selected) ?? playbooks?.[0] ?? null;
  const { nodes, edges } = useMemo(
    () => (pb ? toFlow(pb) : { nodes: [], edges: [] }),
    [pb],
  );

  if (!loading && (!playbooks || playbooks.length === 0)) {
    return (
      <EmptyState
        title="No playbooks found"
        hint="GET /playbooks returned nothing. Check that api-server is running and PLAYBOOKS_DIR contains .yml files."
      />
    );
  }

  return (
    <div className="flex h-full min-h-0">
      {/* Playbook list */}
      <div className="w-64 shrink-0 overflow-y-auto border-r border-neutral-900/10 dark:border-neutral-100/14">
        <div className="flex items-baseline justify-between px-3 pb-1 pt-3">
          <h1 className="text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
            Playbooks
          </h1>
          <span className="text-[12px] tabular-nums text-neutral-900/66 dark:text-neutral-100/66">
            {playbooks?.length ?? 0}
          </span>
        </div>
        {(playbooks ?? []).map((p) => (
          <button
            key={p.name}
            onClick={() => setSelected(p.name)}
            aria-current={pb?.name === p.name ? "page" : undefined}
            className="block w-full px-3 py-2 text-left hover:bg-neutral-900/5 aria-[current=page]:bg-neutral-900/9 dark:hover:bg-neutral-100/5 dark:aria-[current=page]:bg-neutral-100/9"
          >
            <div className="truncate font-mono text-[13px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100">
              {p.name}
            </div>
            {p.description ? (
              <div className="mt-0.5 line-clamp-2 text-[12px]/[15px] text-neutral-900/66 dark:text-neutral-100/66">
                {p.description}
              </div>
            ) : null}
          </button>
        ))}
      </div>

      {/* Graph canvas */}
      <div className="min-h-0 min-w-0 flex-1">
        {pb ? (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            colorMode="dark"
            fitView
            nodesConnectable={false}
            nodesDraggable={false}
          >
            <Background variant={BackgroundVariant.Dots} gap={16} />
            <Controls />
            <MiniMap pannable />
            <Panel
              position="top-right"
              className="max-w-[320px] rounded-[var(--rb-r-sm,6px)] border border-neutral-900/10 bg-white px-3 py-2 dark:border-neutral-100/14 dark:bg-neutral-900"
            >
              <div className="font-mono text-[11px]/[14px] text-neutral-900/66 dark:text-neutral-100/66">
                {pb.trigger || "manual"}
              </div>
              {pb.description ? (
                <div className="mt-0.5 text-[12px]/[15px] text-neutral-900 dark:text-neutral-100">
                  {pb.description}
                </div>
              ) : null}
            </Panel>
          </ReactFlow>
        ) : null}
      </div>
    </div>
  );
}
