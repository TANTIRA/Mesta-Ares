"use client";

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Attachment01Icon,
  Cancel01Icon,
  DragDropVerticalIcon,
  Message01Icon,
} from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const hoverFill = "hover:bg-neutral-900/5 dark:hover:bg-neutral-100/5";
const pressedFill = "active:bg-neutral-900/8 dark:active:bg-neutral-100/8";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const ruleBottom = "border-b border-neutral-900/8 dark:border-neutral-100/8";
const rowsRuled = "divide-y divide-neutral-900/8 dark:divide-neutral-100/8";
const warnText = "text-[oklch(49%_0.1_68)] dark:text-[oklch(80%_0.13_68)]";
const typeHeadingXs = "text-[16px]/[20px] font-[450] tracking-[-0.015em]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeLabelSm = "text-[12px]/[16px] font-[450] tabular-nums";
const typeCodeSm = "font-mono text-[12px]/[16px] tabular-nums";
const typeStateWord = "text-[14px]/[16px] font-[450] tabular-nums";
const overlayShadow =
  "shadow-[0_12px_24px_-12px_rgb(0_0_0/0.2)] dark:shadow-[0_12px_24px_-12px_rgb(0_0_0/0.6)]";
const focusRingInset =
  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))]";
const primaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_94%,black))] text-[var(--rb-accent-fg,oklch(100%_0_0))] shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.22),0_2px_5px_-2px_rgb(0_0_0/0.3)] hover:not-disabled:brightness-110 active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] disabled:opacity-40 disabled:shadow-none dark:bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_94%,black))] dark:text-[var(--rb-accent-fg,oklch(20.5%_0_0))] dark:shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.66),0_2px_5px_-2px_rgb(0_0_0/0.9)] dark:active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] dark:disabled:shadow-none";
const ghostButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-transparent text-neutral-900 hover:not-disabled:bg-neutral-900/5 active:bg-neutral-900/8 aria-pressed:bg-neutral-900/9 disabled:text-neutral-900/40 dark:text-neutral-100 dark:hover:not-disabled:bg-neutral-100/5 dark:active:bg-neutral-100/8 dark:aria-pressed:bg-neutral-100/9 dark:disabled:text-neutral-100/40";
const buttonSm =
  "h-7 gap-1.5 rounded-[var(--rb-r-sm,6px)] px-3 text-[14px]/[16px] font-[450]";
const iconButtonXs = "size-6 rounded-[var(--rb-r-sm,6px)]";
const iconButtonSm = "size-7 rounded-[var(--rb-r-sm,6px)]";
const inputBox =
  "relative flex w-full min-w-0 cursor-text items-center rounded-[var(--rb-r-sm,6px)] data-concentric:rounded-[var(--rb-r-lg,10px)] border border-neutral-900/12 bg-white text-[14px]/[16px] font-[450] text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] transition-[border-color,box-shadow,background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-focus-within:not-has-[[aria-invalid=true]]:not-has-[input:disabled]:border-neutral-900/32 focus-within:border-[var(--rb-accent,oklch(20.5%_0_0))] focus-within:ring-3 focus-within:ring-neutral-900/8 has-[[aria-invalid=true]]:border-[oklch(60%_0.236_26)] has-[input:disabled]:cursor-not-allowed has-[input:disabled]:border-neutral-900/8 has-[input:disabled]:text-neutral-900/40 has-[input:disabled]:shadow-none has-[input[readonly]]:cursor-default has-[input[readonly]]:border-transparent has-[input[readonly]]:bg-neutral-900/5 has-[input[readonly]]:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-focus-within:not-has-[[aria-invalid=true]]:not-has-[input:disabled]:border-neutral-100/32 dark:focus-within:border-[var(--rb-accent,oklch(100%_0_0))] dark:focus-within:ring-neutral-100/8 dark:has-[[aria-invalid=true]]:border-[oklch(65.2%_0.219_26)] dark:has-[input:disabled]:border-neutral-100/8 dark:has-[input:disabled]:text-neutral-100/40 dark:has-[input[readonly]]:bg-neutral-100/5";
const fieldMd = "h-8";
const inputControl =
  "h-full w-full min-w-0 flex-1 bg-transparent px-3 outline-none [[data-affix]+&]:ps-2 [&:has(+[data-affix])]:pe-2 placeholder:text-neutral-900/66 disabled:cursor-not-allowed disabled:placeholder:text-neutral-900/40 dark:placeholder:text-neutral-100/66 dark:disabled:placeholder:text-neutral-100/40 dark:[color-scheme:dark]";
const badge =
  "inline-flex h-5 min-w-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden whitespace-nowrap rounded-[var(--rb-r-xs,4px)] px-1.5 text-[12px]/[16px] font-[450] tabular-nums";
const badgeNeutral =
  "bg-neutral-900/9 dark:bg-neutral-100/9 text-neutral-900 dark:text-neutral-100";
const avatar =
  "relative inline-flex size-8 shrink-0 overflow-hidden rounded-full select-none";
const avatarFallback =
  "flex size-full items-center justify-center rounded-[inherit] border border-neutral-900/12 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 text-[12px]/[16px] font-[450] tabular-nums text-neutral-900 dark:text-neutral-100";
const avatarStack =
  "flex items-center *:not-first:-ms-1.5 *:not-last:[mask-image:radial-gradient(50%_50%_at_calc(150%-6px)_50%,transparent_calc(100%+1.5px),#000_calc(100%+2px))]";
const frame =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 text-neutral-900 dark:text-neutral-100";
const frameTitle =
  "min-w-0 truncate text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100";
const frameMeta =
  "shrink-0 text-[14px]/[16px] font-[450] tabular-nums text-neutral-900/66 dark:text-neutral-100/66";
const panel =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-neutral-50 text-neutral-900 dark:text-neutral-100 dark:bg-neutral-900";
const panelHead =
  "flex min-h-10 min-w-0 shrink-0 items-center gap-2 px-4 has-[button]:pe-2";
const panelBody =
  "mx-1 mb-1 flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-white [&:has(+*)]:mb-0 dark:border-neutral-100/8 dark:bg-neutral-950";
const itemRow =
  "relative flex min-h-10 w-full min-w-0 flex-wrap items-center gap-x-3 gap-y-1 py-2 text-left text-neutral-900 dark:text-neutral-100 transition-[background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] has-[[data-description]]:min-h-14";
const itemRowSelected = "bg-neutral-900/9 dark:bg-neutral-100/9";
const cornerSm = "rounded-[var(--rb-r-sm,6px)]";
const itemTitle =
  "flex max-w-full min-w-0 items-baseline gap-2 text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100";
const skeleton =
  "rounded-[var(--rb-r-sm,6px)] bg-neutral-900/8 dark:bg-neutral-100/8";

const countOver = cx(typeStateWord, warnText);

const EASE_OUT: [number, number, number, number] = [0.2, 0, 0, 1];
const SPRING = { type: "spring" as const, bounce: 0, duration: 0.35 };

export type Card = {
  id: string;
  ref: string;
  title: string;
  tag: string;
  assignee: string;
  comments: number;
  files: number;
  priority: "High" | "Medium" | "Low";
};

/** A card plus the column it belongs to. Column ids: backlog, progress, review, done. */
export type BoardCard = Card & { column?: string };

const CARDS: Record<string, Card> = {
  h1: {
    id: "h1",
    ref: "HAL-241",
    title: "Retry billing webhooks that fail on timeout",
    tag: "Billing",
    assignee: "Ana Reyes",
    comments: 4,
    files: 1,
    priority: "High",
  },
  h2: {
    id: "h2",
    ref: "HAL-238",
    title: "Seat count drifts after a downgrade",
    tag: "Billing",
    assignee: "Priya Nandakumar",
    comments: 2,
    files: 0,
    priority: "High",
  },
  h3: {
    id: "h3",
    ref: "HAL-236",
    title: "Audit log export times out past 90 days",
    tag: "Platform",
    assignee: "Marco Silva",
    comments: 0,
    files: 3,
    priority: "Medium",
  },
  h4: {
    id: "h4",
    ref: "HAL-233",
    title: "Add SCIM group mapping to the admin console",
    tag: "Identity",
    assignee: "Wei Chen",
    comments: 6,
    files: 2,
    priority: "Medium",
  },
  h5: {
    id: "h5",
    ref: "HAL-229",
    title: "Deprecate the v1 usage endpoint",
    tag: "API",
    assignee: "Sofia Alvarez",
    comments: 1,
    files: 0,
    priority: "Low",
  },
  h6: {
    id: "h6",
    ref: "HAL-226",
    title: "Rework the empty state on first workspace",
    tag: "Growth",
    assignee: "Ana Reyes",
    comments: 3,
    files: 1,
    priority: "Medium",
  },
  h7: {
    id: "h7",
    ref: "HAL-221",
    title: "Cache invalidation on role change is slow",
    tag: "Platform",
    assignee: "Marco Silva",
    comments: 8,
    files: 0,
    priority: "High",
  },
  h8: {
    id: "h8",
    ref: "HAL-217",
    title: "Ship the invoice PDF redesign",
    tag: "Billing",
    assignee: "Priya Nandakumar",
    comments: 2,
    files: 4,
    priority: "Low",
  },
  h9: {
    id: "h9",
    ref: "HAL-214",
    title: "Backfill workspace regions for EU tenants",
    tag: "Platform",
    assignee: "Wei Chen",
    comments: 0,
    files: 0,
    priority: "Medium",
  },
  h10: {
    id: "h10",
    ref: "HAL-208",
    title: "Rotate signing keys without downtime",
    tag: "Identity",
    assignee: "Sofia Alvarez",
    comments: 5,
    files: 1,
    priority: "High",
  },
};

const COLUMNS = [
  { id: "backlog", name: "Triage", limit: 0 },
  { id: "progress", name: "Investigating", limit: 4 },
  { id: "review", name: "Containment", limit: 3 },
  { id: "done", name: "Closed", limit: 0 },
] as const;

type ColumnId = (typeof COLUMNS)[number]["id"];

const INITIAL: Record<ColumnId, string[]> = {
  backlog: ["h1", "h5", "h9"],
  progress: ["h2", "h7", "h4"],
  review: ["h3", "h10"],
  done: ["h6", "h8"],
};

const initials = (name: string) =>
  name
    .split(/[\s-]+/)
    .filter(Boolean)
    .slice(0, 1)
    .map((w) => w[0]?.toUpperCase())
    .join("");

const PRIORITY_TONE: Record<Card["priority"], string> = {
  High: text1,
  Medium: text2,
  Low: text3,
};

export default function Kanban1({
  cases,
  onOpen,
  title = "Case board",
}: {
  cases?: BoardCard[];
  onOpen?: (id: string) => void;
  title?: string;
}) {
  const uid = useId();
  const reduce = useReducedMotion();

  const seed = useMemo(() => {
    if (!cases)
      return {
        map: CARDS,
        cols: INITIAL,
        people: ["Ana Reyes", "Priya Nandakumar", "Marco Silva", "Wei Chen"],
      };
    const map: Record<string, Card> = {};
    const cols: Record<ColumnId, string[]> = {
      backlog: [],
      progress: [],
      review: [],
      done: [],
    };
    for (const c of cases) {
      map[c.id] = c;
      const col =
        c.column && c.column in cols ? (c.column as ColumnId) : "backlog";
      cols[col].push(c.id);
    }
    const people = Array.from(
      new Set(cases.map((c) => c.assignee).filter(Boolean)),
    ).slice(0, 4);
    return { map, cols, people: people.length ? people : ["Unassigned"] };
  }, [cases]);

  const [items, setItems] = useState<Record<ColumnId, string[]>>(seed.cols);
  const [cards, setCards] = useState<Record<string, Card>>(seed.map);

  useEffect(() => {
    setItems(seed.cols);
    setCards(seed.map);
  }, [seed]);
  const [dragId, setDragId] = useState<string | null>(null);
  const [grabbed, setGrabbed] = useState<string | null>(null);
  const [ghost, setGhost] = useState({ x: 0, y: 0, w: 0 });
  const [composing, setComposing] = useState<ColumnId | null>(null);
  const [draft, setDraft] = useState("");
  const [announce, setAnnounce] = useState("");

  const rootRef = useRef<HTMLDivElement>(null);
  const colRefs = useRef(new Map<ColumnId, HTMLElement>());
  const seq = useRef(0);
  const dragRef = useRef<{
    id: string;
    dx: number;
    dy: number;
    w: number;
    active: boolean;
    startX: number;
    startY: number;
    pointerId: number;
  } | null>(null);

  const columnOf = useCallback(
    (id: string) =>
      (Object.keys(items) as ColumnId[]).find((c) => items[c].includes(id)) ??
      "backlog",
    [items],
  );

  const move = useCallback((id: string, toCol: ColumnId, toIndex: number) => {
    setItems((prev) => {
      const next = { ...prev } as Record<ColumnId, string[]>;
      let from: ColumnId | null = null;
      for (const c of Object.keys(next) as ColumnId[]) {
        if (next[c].includes(id)) from = c;
      }
      if (!from) return prev;
      const fromIndex = next[from].indexOf(id);
      if (from === toCol && fromIndex === toIndex) return prev;
      next[from] = next[from].filter((x) => x !== id);
      const list = from === toCol ? next[from] : [...next[toCol]];
      list.splice(Math.max(0, Math.min(toIndex, list.length)), 0, id);
      next[toCol] = list;
      return next;
    });
  }, []);

  const targetFromPoint = useCallback(
    (clientX: number, clientY: number, id: string) => {
      for (const [colId, el] of colRefs.current) {
        const r = el.getBoundingClientRect();
        if (clientX < r.left || clientX > r.right) continue;
        const others = Array.from(
          el.querySelectorAll<HTMLElement>("[data-card]"),
        ).filter((c) => c.dataset.card !== id);
        let index = others.length;
        for (let i = 0; i < others.length; i++) {
          const cr = others[i].getBoundingClientRect();
          if (clientY < cr.top + cr.height / 2) {
            index = i;
            break;
          }
        }
        return { col: colId, index };
      }
      return null;
    },
    [],
  );

  const onPointerDown = (e: React.PointerEvent<HTMLElement>, id: string) => {
    if (e.button !== 0) return;
    const el = e.currentTarget.closest<HTMLElement>("[data-card]");
    const root = rootRef.current;
    if (!el || !root) return;
    e.preventDefault();
    const r = el.getBoundingClientRect();
    dragRef.current = {
      id,
      dx: e.clientX - r.left,
      dy: e.clientY - r.top,
      w: r.width,
      active: false,
      startX: e.clientX,
      startY: e.clientY,
      pointerId: e.pointerId,
    };
    root.setPointerCapture(e.pointerId);
    el.querySelector<HTMLElement>(
      '[aria-roledescription="Draggable card"]',
    )?.focus();
  };

  const onPointerMove = (e: React.PointerEvent<HTMLElement>) => {
    const d = dragRef.current;
    const root = rootRef.current;
    if (!d || !root) return;
    if (!d.active) {
      if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < 4) return;
      d.active = true;
      setGrabbed(null);
      setDragId(d.id);
    }
    const rr = root.getBoundingClientRect();
    setGhost({
      x: e.clientX - rr.left - d.dx,
      y: e.clientY - rr.top - d.dy,
      w: d.w,
    });
    const t = targetFromPoint(e.clientX, e.clientY, d.id);
    if (t) move(d.id, t.col as ColumnId, t.index);
  };

  const endDrag = () => {
    const d = dragRef.current;
    if (!d) return;
    dragRef.current = null;
    const root = rootRef.current;
    if (root?.hasPointerCapture?.(d.pointerId))
      root.releasePointerCapture(d.pointerId);
    if (d.active) {
      const col = COLUMNS.find((c) => c.id === columnOf(d.id));
      setAnnounce(`${cards[d.id]?.ref} moved to ${col?.name}.`);
    }
    setDragId(null);
  };

  const onCardKeyDown = (e: React.KeyboardEvent<HTMLElement>, id: string) => {
    const col = columnOf(id);
    const index = items[col].indexOf(id);
    const colIndex = COLUMNS.findIndex((c) => c.id === col);

    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      const next = grabbed === id ? null : id;
      setGrabbed(next);
      setAnnounce(
        next
          ? `${cards[id].ref} grabbed. Use the arrow keys to move it, then press space to drop.`
          : `${cards[id].ref} dropped in ${COLUMNS[colIndex].name}.`,
      );
      return;
    }
    if (e.key === "Escape" && grabbed === id) {
      e.preventDefault();
      setGrabbed(null);
      setAnnounce("Move cancelled.");
      return;
    }
    if (grabbed !== id) return;

    let handled = true;
    if (e.key === "ArrowUp") move(id, col, Math.max(0, index - 1));
    else if (e.key === "ArrowDown") move(id, col, index + 1);
    else if (e.key === "ArrowLeft" && colIndex > 0)
      move(
        id,
        COLUMNS[colIndex - 1].id,
        items[COLUMNS[colIndex - 1].id].length,
      );
    else if (e.key === "ArrowRight" && colIndex < COLUMNS.length - 1)
      move(
        id,
        COLUMNS[colIndex + 1].id,
        items[COLUMNS[colIndex + 1].id].length,
      );
    else handled = false;

    if (handled) {
      e.preventDefault();
      e.currentTarget.focus();
    }
  };

  const addCard = (col: ColumnId) => {
    const title = draft.trim();
    if (!title) {
      setComposing(null);
      return;
    }
    const n = 242 + seq.current;
    const id = `n${n}`;
    seq.current += 1;
    setCards((p) => ({
      ...p,
      [id]: {
        id,
        ref: `HAL-${n}`,
        title,
        tag: "Triage",
        assignee: "Ana Reyes",
        comments: 0,
        files: 0,
        priority: "Medium",
      },
    }));
    setItems((p) => ({ ...p, [col]: [id, ...p[col]] }));
    setDraft("");
    setComposing(null);
    setAnnounce(`Added ${title}.`);
  };

  const total = useMemo(
    () => Object.values(items).reduce((s, l) => s + l.length, 0),
    [items],
  );

  return (
    <div
      ref={rootRef}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={endDrag}
      className={cx(
        canvas,
        text1,
        "relative flex h-full min-h-[680px] w-full flex-col overflow-hidden",
        dragId && "touch-none select-none",
      )}
    >
      <header className="flex shrink-0 flex-wrap items-end justify-between gap-3 px-6 pt-6 pb-4 sm:px-8">
        <div className="min-w-0">
          <h2 className={cx(typeHeadingXs, text1)}>{title}</h2>
          <p className={cx(typeBodySm, text3, "mt-0.5 truncate")}>
            {total} cases. Drag a card to move stage, or press space on a
            focused card.
          </p>
        </div>
        <div className={cx(avatarStack, "shrink-0")}>
          {seed.people.map((p) => (
            <span key={p} title={p} className={cx(avatar, "size-[28px]")}>
              <span className={avatarFallback}>{initials(p)}</span>
            </span>
          ))}
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-x-auto overflow-y-hidden px-6 pb-6 sm:px-8">
        <div className="flex h-full min-w-max gap-2.5">
          {COLUMNS.map((col) => {
            const list = items[col.id];
            const over = col.limit > 0 && list.length > col.limit;
            return (
              <section
                key={col.id}
                aria-label={col.name}
                className={cx(panel, "h-full w-[264px] shrink-0")}
              >
                <div className={panelHead}>
                  <span className={frameTitle}>{col.name}</span>
                  <span
                    className={cx(over ? countOver : frameMeta, "shrink-0")}
                  >
                    {list.length}
                    {col.limit > 0 ? `/${col.limit}` : ""}
                  </span>
                  <button
                    type="button"
                    onClick={() => {
                      setComposing(col.id);
                      setDraft("");
                    }}
                    aria-label={`Add a card to ${col.name}`}
                    className={cx(ghostButton, iconButtonXs, "ml-auto")}
                  >
                    <HugeiconsIcon
                      icon={Add01Icon}
                      size={16}
                      strokeWidth={1.8}
                      aria-hidden="true"
                    />
                  </button>
                </div>

                <div
                  ref={(el) => {
                    if (el) colRefs.current.set(col.id, el);
                    else colRefs.current.delete(col.id);
                  }}
                  className={cx(panelBody, "min-h-0 flex-1 overflow-y-auto")}
                >
                  {composing === col.id && (
                    <div className={cx(ruleBottom, "px-3 py-2")}>
                      <label
                        htmlFor={`${uid}-${col.id}-draft`}
                        className="sr-only"
                      >
                        New card in {col.name}
                      </label>
                      <div className={cx(inputBox, fieldMd)}>
                        <input
                          id={`${uid}-${col.id}-draft`}
                          autoFocus
                          value={draft}
                          onChange={(e) => setDraft(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") addCard(col.id);
                            if (e.key === "Escape") setComposing(null);
                          }}
                          placeholder="What needs doing?"
                          className={inputControl}
                        />
                      </div>
                      <div className="mt-2 flex items-center gap-1">
                        <button
                          type="button"
                          onClick={() => addCard(col.id)}
                          className={cx(primaryButton, buttonSm)}
                        >
                          Add
                        </button>
                        <button
                          type="button"
                          onClick={() => setComposing(null)}
                          aria-label="Cancel"
                          className={cx(ghostButton, iconButtonSm)}
                        >
                          <HugeiconsIcon
                            icon={Cancel01Icon}
                            size={16}
                            strokeWidth={1.8}
                            aria-hidden="true"
                          />
                        </button>
                      </div>
                    </div>
                  )}

                  <ul className={cx(rowsRuled, "p-1")}>
                    {list.map((id) => {
                      const card = cards[id];
                      if (!card) return null;
                      const isDragging = dragId === id;
                      const isGrabbed = grabbed === id;
                      return (
                        <motion.li
                          key={id}
                          layout={reduce ? false : "position"}
                          transition={SPRING}
                          data-card={id}
                          className={cx(isDragging && cx(skeleton, cornerSm))}
                        >
                          <div
                            role="button"
                            tabIndex={0}
                            aria-roledescription="Draggable card"
                            aria-label={`${card.ref}: ${card.title}. ${card.priority} priority, assigned to ${card.assignee}.`}
                            onPointerDown={(e) => {
                              if (e.pointerType === "mouse")
                                onPointerDown(e, id);
                            }}
                            onKeyDown={(e) => onCardKeyDown(e, id)}
                            className={cx(
                              itemRow,
                              cornerSm,
                              isGrabbed
                                ? itemRowSelected
                                : cx(hoverFill, pressedFill),
                              focusRingInset,
                              "cursor-grab px-2",
                              isDragging && "invisible",
                            )}
                          >
                            <div className="flex w-full items-center gap-2">
                              <span
                                className={cx(
                                  typeLabelSm,
                                  PRIORITY_TONE[card.priority],
                                )}
                              >
                                {card.priority}
                              </span>
                              <span className={cx(typeCodeSm, text3)}>
                                {card.ref}
                              </span>
                              <span
                                onPointerDown={(e) => onPointerDown(e, id)}
                                style={{ touchAction: "none" }}
                                aria-hidden
                                className={cx(
                                  text3,
                                  "ml-auto -mr-0.5 cursor-grab",
                                )}
                              >
                                <HugeiconsIcon
                                  icon={DragDropVerticalIcon}
                                  size={14}
                                  strokeWidth={1.8}
                                  aria-hidden="true"
                                />
                              </span>
                            </div>

                            <p className={cx(itemTitle, "w-full")}>
                              <button
                                type="button"
                                onPointerDown={(e) => e.stopPropagation()}
                                onClick={() => onOpen?.(card.id)}
                                className={cx(
                                  focusRingInset,
                                  "cursor-pointer rounded-[var(--rb-r-xs,4px)] text-left hover:underline",
                                )}
                              >
                                {card.title}
                              </button>
                            </p>

                            <div className="flex w-full items-center gap-2">
                              <span className={cx(badge, badgeNeutral)}>
                                {card.tag}
                              </span>
                              {card.comments > 0 && (
                                <span
                                  className={cx(
                                    typeLabelSm,
                                    text3,
                                    "inline-flex items-center gap-1",
                                  )}
                                >
                                  <HugeiconsIcon
                                    icon={Message01Icon}
                                    size={12}
                                    strokeWidth={1.8}
                                    aria-hidden="true"
                                  />
                                  {card.comments}
                                </span>
                              )}
                              {card.files > 0 && (
                                <span
                                  className={cx(
                                    typeLabelSm,
                                    text3,
                                    "inline-flex items-center gap-1",
                                  )}
                                >
                                  <HugeiconsIcon
                                    icon={Attachment01Icon}
                                    size={12}
                                    strokeWidth={1.8}
                                    aria-hidden="true"
                                  />
                                  {card.files}
                                </span>
                              )}
                              <span
                                title={card.assignee}
                                className={cx(avatar, "ml-auto size-[24px]")}
                              >
                                <span className={avatarFallback}>
                                  {initials(card.assignee)}
                                </span>
                              </span>
                            </div>
                          </div>
                        </motion.li>
                      );
                    })}
                  </ul>

                  {list.length === 0 && composing !== col.id && (
                    <p
                      className={cx(typeBodySm, text3, "px-3 py-6 text-center")}
                    >
                      Nothing here yet
                    </p>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      </div>

      <AnimatePresence>
        {dragId && cards[dragId] && (
          <motion.div
            key="ghost"
            initial={reduce ? { opacity: 0.9 } : { opacity: 0.9, scale: 1 }}
            animate={
              reduce
                ? { opacity: 1 }
                : { opacity: 1, scale: 1.02, rotate: -1.5 }
            }
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
            transition={{ duration: 0.16, ease: EASE_OUT }}
            style={{
              left: ghost.x,
              top: ghost.y,
              width: ghost.w,
            }}
            className="pointer-events-none absolute z-50"
          >
            <div className={cx(frame, overlayShadow, "gap-1 px-3 py-2")}>
              <div className="flex items-center gap-2">
                <span
                  className={cx(
                    typeLabelSm,
                    PRIORITY_TONE[cards[dragId].priority],
                  )}
                >
                  {cards[dragId].priority}
                </span>
                <span className={cx(typeCodeSm, text3)}>
                  {cards[dragId].ref}
                </span>
              </div>
              <p className={itemTitle}>{cards[dragId].title}</p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <p aria-live="polite" className="sr-only">
        {announce}
      </p>
    </div>
  );
}
