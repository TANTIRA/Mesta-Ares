"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Search01Icon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const avatarSquare =
  "relative inline-flex size-8 shrink-0 overflow-hidden rounded-[var(--rb-r-sm,6px)] select-none";
const avatarFallback =
  "flex size-full items-center justify-center rounded-[inherit] border border-neutral-900/12 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 text-[12px]/[16px] font-[450] tabular-nums text-neutral-900 dark:text-neutral-100";
const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const typeHeadingSm = "text-[20px]/[24px] font-[450] tracking-[-0.015em]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeLabel = "text-[14px]/[16px] font-[450]";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const secondaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] border border-neutral-900/12 bg-white text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] hover:not-disabled:border-neutral-900/32 hover:not-disabled:bg-linear-to-b hover:not-disabled:from-neutral-900/3 hover:not-disabled:to-neutral-900/3 active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:border-neutral-100/32 dark:hover:not-disabled:from-neutral-100/3 dark:hover:not-disabled:to-neutral-100/3 dark:active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.4)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const buttonMd =
  "h-8 gap-2 rounded-[var(--rb-r-sm,6px)] px-4 text-[14px]/[16px] font-[450]";
const inputBox =
  "relative flex w-full min-w-0 cursor-text items-center rounded-[var(--rb-r-sm,6px)] data-concentric:rounded-[var(--rb-r-lg,10px)] border border-neutral-900/12 bg-white text-[14px]/[16px] font-[450] text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] transition-[border-color,box-shadow,background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-focus-within:not-has-[[aria-invalid=true]]:not-has-[input:disabled]:border-neutral-900/32 focus-within:border-[var(--rb-accent,oklch(20.5%_0_0))] focus-within:ring-3 focus-within:ring-neutral-900/8 has-[[aria-invalid=true]]:border-[oklch(60%_0.236_26)] has-[input:disabled]:cursor-not-allowed has-[input:disabled]:border-neutral-900/8 has-[input:disabled]:text-neutral-900/40 has-[input:disabled]:shadow-none has-[input[readonly]]:cursor-default has-[input[readonly]]:border-transparent has-[input[readonly]]:bg-neutral-900/5 has-[input[readonly]]:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-focus-within:not-has-[[aria-invalid=true]]:not-has-[input:disabled]:border-neutral-100/32 dark:focus-within:border-[var(--rb-accent,oklch(100%_0_0))] dark:focus-within:ring-neutral-100/8 dark:has-[[aria-invalid=true]]:border-[oklch(65.2%_0.219_26)] dark:has-[input:disabled]:border-neutral-100/8 dark:has-[input:disabled]:text-neutral-100/40 dark:has-[input[readonly]]:bg-neutral-100/5";
const fieldMd = "h-8";
const inputControl =
  "h-full w-full min-w-0 flex-1 bg-transparent px-3 outline-none [[data-affix]+&]:ps-2 [&:has(+[data-affix])]:pe-2 placeholder:text-neutral-900/66 disabled:cursor-not-allowed disabled:placeholder:text-neutral-900/40 dark:placeholder:text-neutral-100/66 dark:disabled:placeholder:text-neutral-100/40 dark:[color-scheme:dark]";
const inputAffix =
  "flex shrink-0 items-center gap-2 text-[14px]/[16px] text-neutral-900/66 dark:text-neutral-100/66 select-none first:ps-3 last:pe-3 has-[button]:first:ps-1 has-[button]:last:pe-1";
const lineTabsList =
  "flex max-w-full min-w-0 items-center gap-1 overflow-x-auto shadow-[inset_0_-1px_0_color-mix(in_oklab,var(--color-neutral-900)_8%,transparent)] [scrollbar-width:none] dark:shadow-[inset_0_-1px_0_color-mix(in_oklab,var(--color-neutral-100)_8%,transparent)]";
const lineTab =
  "relative inline-flex min-h-8 shrink-0 cursor-pointer items-center justify-center gap-1.5 rounded-[var(--rb-r-sm,6px)] px-2 text-[14px]/[16px] font-[450] whitespace-nowrap text-neutral-900/80 select-none transition-[color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:text-neutral-900 after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-transparent aria-selected:text-neutral-900 aria-selected:after:bg-neutral-900 disabled:cursor-not-allowed disabled:text-neutral-900/40 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:text-neutral-100/80 dark:hover:text-neutral-100 dark:aria-selected:text-neutral-100 dark:aria-selected:after:bg-neutral-100 dark:disabled:text-neutral-100/40";
const frame =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 text-neutral-900 dark:text-neutral-100";
const emptyTitle =
  "text-[16px]/[20px] font-[450] tracking-[-0.015em] text-neutral-900 dark:text-neutral-100";
const emptyDescription =
  "max-w-[48ch] text-[13px]/[18px] text-neutral-900/80 dark:text-neutral-100/80";

const fadeTop =
  "bg-linear-to-b from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeBottom =
  "bg-linear-to-t from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeStart =
  "bg-linear-to-r from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeEnd =
  "bg-linear-to-l from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";

const metaText = cx(typeBodySm, text3);
const detailText = cx(typeBodySm, text2);
const captionText = cx(typeCaption, text3);

function useScrollFade<T extends HTMLElement>(content?: unknown) {
  const ref = useRef<T>(null);
  const [edges, setEdges] = useState({ start: false, end: false });
  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const { scrollTop, scrollHeight, clientHeight } = el;
    setEdges({
      start: scrollTop > 1,
      end: Math.ceil(scrollTop + clientHeight) < scrollHeight - 1,
    });
  }, []);
  useEffect(() => {
    update();
    const el = ref.current;
    const view = el?.ownerDocument.defaultView;
    if (!el || !view?.ResizeObserver) return;
    const observer = new view.ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [content, update]);
  return { ref, edges, onScroll: update };
}

function useScrollFadeX<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [edges, setEdges] = useState({ start: false, end: false });
  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const { scrollLeft, scrollWidth, clientWidth } = el;
    setEdges({
      start: scrollLeft > 1,
      end: Math.ceil(scrollLeft + clientWidth) < scrollWidth - 1,
    });
  }, []);
  useEffect(() => {
    update();
    const el = ref.current;
    const view = el?.ownerDocument.defaultView;
    if (!el || !view?.ResizeObserver) return;
    const observer = new view.ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [update]);
  return { ref, edges, onScroll: update };
}

function moveChoice(
  e: React.KeyboardEvent<HTMLElement>,
  count: number,
  index: number,
  select: (next: number) => void,
) {
  const next =
    e.key === "ArrowRight" || e.key === "ArrowDown"
      ? (index + 1) % count
      : e.key === "ArrowLeft" || e.key === "ArrowUp"
        ? (index - 1 + count) % count
        : e.key === "Home"
          ? 0
          : e.key === "End"
            ? count - 1
            : -1;
  if (next < 0) return;
  e.preventDefault();
  select(next);
  e.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
}

export type App = {
  name: string;
  mark: string;
  group: string;
  blurb: string;
  connected?: boolean;
};

const APPS: App[] = [
  {
    name: "Northwind",
    mark: "NW",
    group: "Analytics",
    blurb: "Stream product events into Northwind dashboards.",
    connected: true,
  },
  {
    name: "Cedar Labs",
    mark: "CL",
    group: "Developer",
    blurb: "Open an issue whenever a deploy check fails.",
    connected: true,
  },
  {
    name: "Harbor",
    mark: "HB",
    group: "Messaging",
    blurb: "Post alerts and approvals into a shared channel.",
  },
  {
    name: "Meridian",
    mark: "MD",
    group: "Storage",
    blurb: "Archive exports to a bucket you control.",
  },
  {
    name: "Alder",
    mark: "AL",
    group: "Analytics",
    blurb: "Warehouse sync for usage and billing tables.",
  },
  {
    name: "Fernrock",
    mark: "FR",
    group: "Developer",
    blurb: "Run a workflow when an environment is promoted.",
  },
  {
    name: "Tidepool",
    mark: "TP",
    group: "Messaging",
    blurb: "Route incidents to the on-call rotation.",
  },
  {
    name: "Quarry",
    mark: "QR",
    group: "Storage",
    blurb: "Mirror audit logs to cold storage nightly.",
  },
  {
    name: "Lantern",
    mark: "LN",
    group: "Security",
    blurb: "Send sign-in anomalies to your SIEM.",
  },
];



export default function Integrations1({
  apps = APPS,
}: {
  apps?: App[];
}) {
  const [group, setGroup] = useState("All");
  const [q, setQ] = useState("");
  const groups = useMemo(
    () => ["All", ...new Set(apps.map((a) => a.group))],
    [apps],
  );
  const [connected, setConnected] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(
      apps.filter((a) => a.connected).map((a) => [a.name, true]),
    ),
  );

  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return apps.filter(
      (a) =>
        (group === "All" || a.group === group) &&
        (!needle ||
          a.name.toLowerCase().includes(needle) ||
          a.blurb.toLowerCase().includes(needle)),
    );
  }, [group, q]);

  const count = Object.values(connected).filter(Boolean).length;
  const {
    ref: gridRef,
    edges: gridEdges,
    onScroll: onGridScroll,
  } = useScrollFade<HTMLDivElement>(`${group}|${q}`);
  const {
    ref: tabsRef,
    edges: tabsEdges,
    onScroll: onTabsScroll,
  } = useScrollFadeX<HTMLDivElement>();

  return (
    <div
      className={cx(
        canvas,
        text1,
        "relative flex h-full min-h-[680px] w-full flex-col overflow-hidden",
      )}
    >
      <header className="shrink-0 pt-4">
        <div className="flex flex-wrap items-end justify-between gap-3 px-5">
          <div>
            <h2 className={typeHeadingSm}>Integrations</h2>
            <p className={cx(metaText, "mt-1 flex flex-wrap gap-x-3")}>
              <span>
                <span className="tabular-nums">{count}</span> connected
              </span>
              <span>
                <span className="tabular-nums">{apps.length}</span> available
              </span>
            </p>
          </div>

          <div className={cx(inputBox, fieldMd, "sm:w-[248px]")}>
            <span data-affix className={inputAffix}>
              <HugeiconsIcon
                icon={Search01Icon}
                size={16}
                strokeWidth={1.8}
                aria-hidden="true"
              />
            </span>
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search integrations"
              aria-label="Search integrations"
              className={inputControl}
            />
          </div>
        </div>

        <div className="relative mt-3.5">
          <div
            ref={tabsRef}
            onScroll={onTabsScroll}
            role="tablist"
            aria-label="Category"
            onKeyDown={(e) =>
              moveChoice(e, groups.length, groups.indexOf(group), (i) =>
                setGroup(groups[i]),
              )
            }
            className={cx(lineTabsList, "px-5")}
          >
            {groups.map((g) => {
              const on = g === group;
              return (
                <button
                  key={g}
                  role="tab"
                  type="button"
                  aria-selected={on}
                  tabIndex={on ? 0 : -1}
                  onClick={() => setGroup(g)}
                  className={lineTab}
                >
                  {g}
                </button>
              );
            })}
          </div>
          <div
            aria-hidden="true"
            className={cx(
              fadeStart,
              "pointer-events-none absolute top-0 bottom-px left-0 w-8",
              tabsEdges.start ? "opacity-100" : "opacity-0",
            )}
          />
          <div
            aria-hidden="true"
            className={cx(
              fadeEnd,
              "pointer-events-none absolute top-0 right-0 bottom-px w-8",
              tabsEdges.end ? "opacity-100" : "opacity-0",
            )}
          />
        </div>
      </header>

      <div className="relative min-h-0 flex-1">
        <div
          ref={gridRef}
          onScroll={onGridScroll}
          className="h-full overflow-y-auto px-5 py-4"
        >
          {list.length > 0 ? (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {list.map((a) => {
                const on = !!connected[a.name];
                return (
                  <article key={a.name} className={cx(frame, "p-4")}>
                    <div className="flex items-center gap-3">
                      <span
                        aria-hidden="true"
                        className={cx(avatarSquare, "size-9")}
                      >
                        <span className={avatarFallback}>{a.mark}</span>
                      </span>
                      <div className="min-w-0 flex-1">
                        <h3 className={cx(typeLabel, "truncate")}>{a.name}</h3>
                        <p className={cx(captionText, "mt-0.5")}>{a.group}</p>
                      </div>
                    </div>

                    <p className={cx(detailText, "mt-3 flex-1")}>{a.blurb}</p>

                    <button
                      type="button"
                      onClick={() =>
                        setConnected((p) => ({ ...p, [a.name]: !p[a.name] }))
                      }
                      className={cx(secondaryButton, buttonMd, "mt-4")}
                    >
                      <HugeiconsIcon
                        icon={on ? Tick02Icon : Add01Icon}
                        size={16}
                        strokeWidth={1.8}
                        aria-hidden="true"
                      />
                      {on ? "Connected" : "Connect"}
                    </button>
                  </article>
                );
              })}
            </div>
          ) : (
            <div className="flex flex-col items-center px-6 py-20 text-center">
              <p className={emptyTitle}>No integrations match</p>
              <p className={cx(emptyDescription, "mt-1")}>
                Try a different category, or clear the search to see everything
                available.
              </p>
              <button
                type="button"
                onClick={() => {
                  setQ("");
                  setGroup("All");
                }}
                className={cx(secondaryButton, buttonMd, "mt-4")}
              >
                Reset filters
              </button>
            </div>
          )}
        </div>
        <div
          aria-hidden="true"
          className={cx(
            fadeTop,
            "pointer-events-none absolute inset-x-0 top-0 h-8",
            gridEdges.start ? "opacity-100" : "opacity-0",
          )}
        />
        <div
          aria-hidden="true"
          className={cx(
            fadeBottom,
            "pointer-events-none absolute inset-x-0 bottom-0 h-8",
            gridEdges.end ? "opacity-100" : "opacity-0",
          )}
        />
      </div>
    </div>
  );
}
