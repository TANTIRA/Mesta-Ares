"use client";

import { useMemo } from "react";
import { useTheme } from "next-themes";
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import type { PlanStep } from "@/lib/api";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const fail = "text-[oklch(51%_0.18_26)] dark:text-[oklch(76%_0.14_26)]";

type StepState =
  | "done"
  | "failed"
  | "running"
  | "paused"
  | "queued"
  | "skipped";

const STATE: Record<string, StepState> = {
  done: "done",
  complete: "done",
  ok: "done",
  success: "done",
  failed: "failed",
  error: "failed",
  running: "running",
  in_progress: "running",
  paused: "paused",
  approval_required: "paused",
  skipped: "skipped",
};

const stateOf = (s: string | undefined): StepState => STATE[s ?? ""] ?? "queued";

const STATE_WORD: Record<StepState, { label: string; cls: string }> = {
  done: { label: "Done", cls: "text-neutral-900/80 dark:text-neutral-100/80" },
  failed: { label: "Failed", cls: fail },
  running: { label: "Running", cls: "text-neutral-900 dark:text-neutral-100" },
  paused: { label: "Paused", cls: "text-neutral-900 dark:text-neutral-100" },
  queued: { label: "Queued", cls: "text-neutral-900/55 dark:text-neutral-100/55" },
  skipped: { label: "Skipped", cls: "text-neutral-900/55 dark:text-neutral-100/55" },
};

type PlanNodeData = {
  index: number;
  title: string;
  detail: string;
  state: StepState;
};
type PlanNode = Node<PlanNodeData, "planStep">;

type RollbackNodeData = { tool: string };
type RollbackNode = Node<RollbackNodeData, "rollback">;

function PlanStepNode({ data, selected }: NodeProps<PlanNode>) {
  const w = STATE_WORD[data.state];
  return (
    <div
      className={cx(
        "w-[230px] rounded-[var(--rb-r-sm,6px)] border bg-white px-3 py-2 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] dark:bg-neutral-900 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.5)]",
        data.state === "failed"
          ? "border-[oklch(51%_0.18_26)]/50 dark:border-[oklch(76%_0.14_26)]/40"
          : data.state === "done"
            ? "border-emerald-600/50 dark:border-emerald-400/40"
            : "border-neutral-900/12 dark:border-neutral-100/14",
        selected &&
          "outline-2 outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:outline-[var(--rb-accent,oklch(100%_0_0))]",
      )}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!size-1.5 !border-0 !bg-neutral-900/30 dark:!bg-neutral-100/30"
      />
      <div className="flex items-baseline gap-2">
        <span className="font-mono text-[11px]/[16px] tabular-nums text-neutral-900/55 dark:text-neutral-100/55">
          {String(data.index + 1).padStart(2, "0")}
        </span>
        <span
          className={cx(
            "ml-auto inline-flex items-center gap-1.5 text-[11px]/[16px] font-medium",
            w.cls,
          )}
        >
          {data.state === "running" && (
            <span
              aria-hidden="true"
              className="inline-block size-2 rounded-full border border-current border-t-transparent animate-spin motion-reduce:animate-none"
            />
          )}
          {w.label}
        </span>
      </div>
      <div
        className="mt-1 truncate text-[13px]/[18px] font-[450] text-neutral-900 dark:text-neutral-100"
        title={data.title}
      >
        {data.title}
      </div>
      {data.detail && (
        <div
          className="mt-0.5 truncate font-mono text-[11px]/[16px] text-neutral-900/55 dark:text-neutral-100/55"
          title={data.detail}
        >
          {data.detail}
        </div>
      )}
      <Handle
        id="next"
        type="source"
        position={Position.Right}
        className="!size-1.5 !border-0 !bg-neutral-900/30 dark:!bg-neutral-100/30"
      />
      <Handle
        id="rb"
        type="source"
        position={Position.Bottom}
        className="!size-1.5 !border-0 !bg-neutral-900/30 dark:!bg-neutral-100/30"
      />
    </div>
  );
}

function RollbackChip({ data }: NodeProps<RollbackNode>) {
  return (
    <div className="w-[190px] rounded-[var(--rb-r-sm,6px)] border border-dashed border-neutral-900/25 bg-white/60 px-2.5 py-1.5 dark:border-neutral-100/25 dark:bg-neutral-900/60">
      <Handle
        type="target"
        position={Position.Top}
        className="!size-1.5 !border-0 !bg-neutral-900/30 dark:!bg-neutral-100/30"
      />
      <div
        className="truncate font-mono text-[11px]/[16px] text-neutral-900/66 dark:text-neutral-100/66"
        title={`rollback: ${data.tool}`}
      >
        {"\u21A9"} {data.tool}
      </div>
    </div>
  );
}

const nodeTypes = { planStep: PlanStepNode, rollback: RollbackChip };

const X_GAP = 290;
const WRAP = 8;
const ROW_H = 210;
const RB_DY = 120;
const EDGE_STYLE = { stroke: "#a3a3a3", strokeWidth: 1.5 };
const RB_STYLE = { ...EDGE_STYLE, strokeDasharray: "5 4" };
const ARROW = {
  type: MarkerType.ArrowClosed,
  width: 16,
  height: 16,
  color: "#a3a3a3",
} as const;

const toFlow = (
  steps: PlanStep[],
): { nodes: (PlanNode | RollbackNode)[]; edges: Edge[] } => {
  const nodes: (PlanNode | RollbackNode)[] = [];
  const edges: Edge[] = [];
  let prev: PlanNode | null = null;

  steps.forEach((s, i) => {
    const row = Math.floor(i / WRAP);
    const col = i % WRAP;
    const node: PlanNode = {
      id: s.id ?? `step-${i}`,
      type: "planStep",
      position: { x: col * X_GAP, y: row * ROW_H },
      data: {
        index: i,
        title: s.title,
        detail: s.detail,
        state: stateOf(s.status),
      },
    };
    nodes.push(node);

    if (prev) {
      edges.push({
        id: `e-${prev.id}-${node.id}`,
        source: prev.id,
        sourceHandle: "next",
        target: node.id,
        type: "smoothstep",
        animated: node.data.state === "running",
        markerEnd: ARROW,
        style: EDGE_STYLE,
      });
    }
    prev = node;

    if (s.rollback) {
      const rb: RollbackNode = {
        id: `${node.id}-rb`,
        type: "rollback",
        position: { x: col * X_GAP + 20, y: row * ROW_H + RB_DY },
        data: { tool: s.rollback },
      };
      nodes.push(rb);
      edges.push({
        id: `e-${node.id}-${rb.id}`,
        source: node.id,
        sourceHandle: "rb",
        target: rb.id,
        type: "smoothstep",
        markerEnd: ARROW,
        style: RB_STYLE,
      });
    }
  });

  return { nodes, edges };
};

/**
 * Response plan for a case rendered as a left-to-right node graph.
 * One node per recorded `response_actions` row, chained in execution order.
 */
export default function PlanGraph({
  steps,
  title,
}: {
  steps: PlanStep[];
  title?: string;
}) {
  const { resolvedTheme } = useTheme();
  const { nodes, edges } = useMemo(() => toFlow(steps), [steps]);

  return (
    <div className="relative flex h-full min-h-[420px] w-full flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-4 dark:border-neutral-100/14 dark:bg-neutral-900 sm:p-5">
      <header className="mb-3 flex shrink-0 items-baseline gap-3">
        <h2 className="min-w-0 flex-1 truncate text-[16px]/[20px] font-[450] tracking-[-0.015em] text-neutral-900 dark:text-neutral-100">
          {title ?? "Plan graph"}
        </h2>
        <span className="shrink-0 tabular-nums text-[13px]/[18px] text-neutral-900/66 dark:text-neutral-100/66">
          {steps.length} {steps.length === 1 ? "action" : "actions"}
        </span>
      </header>
      <div className="min-h-0 flex-1 overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-neutral-50 dark:border-neutral-100/8 dark:bg-neutral-950">
        {nodes.length === 0 ? (
          <div className="grid h-full place-items-center text-[13px]/[18px] text-neutral-900/66 dark:text-neutral-100/66">
            No response actions recorded for this case.
          </div>
        ) : (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            fitView
            fitViewOptions={{ padding: 0.3, maxZoom: 1 }}
            minZoom={0.2}
            nodesConnectable={false}
            panOnDrag
            zoomOnScroll
            colorMode={resolvedTheme === "dark" ? "dark" : "light"}
          >
            <Background variant={BackgroundVariant.Dots} gap={20} size={1} />
            <Controls showInteractive={false} position="bottom-right" />
          </ReactFlow>
        )}
      </div>
    </div>
  );
}
