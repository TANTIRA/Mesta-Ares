"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlayIcon, SquareIcon, Tick02Icon } from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const typeHeadingXs = "text-[16px]/[20px] font-[450] tracking-[-0.015em]";
const typeLabel = "text-[14px]/[16px] font-[450]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const typeCodeSm = "font-mono text-[12px]/[16px] tabular-nums";
const rowsRuled = "divide-y divide-neutral-900/8 dark:divide-neutral-100/8";
const cornerSm = "rounded-[var(--rb-r-sm,6px)]";
const focusRingInset =
  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))]";
const secondaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] border border-neutral-900/12 bg-white text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] hover:not-disabled:border-neutral-900/32 hover:not-disabled:bg-linear-to-b hover:not-disabled:from-neutral-900/3 hover:not-disabled:to-neutral-900/3 active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:border-neutral-100/32 dark:hover:not-disabled:from-neutral-100/3 dark:hover:not-disabled:to-neutral-100/3 dark:active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.4)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const buttonMd =
  "h-8 gap-2 rounded-[var(--rb-r-sm,6px)] px-4 text-[14px]/[16px] font-[450]";
const emptyValue = "text-neutral-900/66 dark:text-neutral-100/66";
const statusWord =
  "inline-flex items-start gap-1.5 text-[14px]/[16px] font-[450] tabular-nums";
const statusIdle = "text-neutral-900/66 dark:text-neutral-100/66";
const statusQueued = "text-neutral-900 dark:text-neutral-100";
const statusRunning = "text-neutral-900 dark:text-neutral-100";
const statusOk = "text-neutral-900/80 dark:text-neutral-100/80";
const arcLine = "flex h-lh shrink-0 items-center";
const runningArc =
  "inline-block size-2.5 shrink-0 rounded-full border border-current border-t-transparent animate-spin motion-reduce:animate-none";
const panel =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-neutral-50 text-neutral-900 dark:text-neutral-100 dark:bg-neutral-900";
const panelBody =
  "mx-1 mb-1 flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-white [&:has(+*)]:mb-0 dark:border-neutral-100/8 dark:bg-neutral-950";
const itemRowSm =
  "relative flex min-h-8 w-full min-w-0 flex-wrap items-center gap-x-3 gap-y-1 py-1.5 text-left text-neutral-900 dark:text-neutral-100 transition-[background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)]";
const itemRowInteractive =
  "cursor-pointer hover:bg-neutral-900/5 dark:hover:bg-neutral-100/5 active:bg-neutral-900/8 dark:active:bg-neutral-100/8";
const progressTrack =
  "relative block h-1 w-full overflow-hidden bg-neutral-900/7 dark:bg-neutral-100/7";
const progressFill =
  "block h-full bg-neutral-900 transition-[width] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none dark:bg-neutral-100";
const stepIndicator =
  "grid size-6 shrink-0 place-items-center rounded-[var(--rb-r-sm,6px)] border text-[12px]/[16px] font-[450] tabular-nums";
const stepDone =
  "border-neutral-900/12 bg-white dark:bg-neutral-900 text-neutral-900/80 dark:text-neutral-100/80 dark:border-neutral-100/14";
const stepCurrent =
  "border-neutral-900 bg-white dark:bg-neutral-900 text-neutral-900 dark:text-neutral-100 dark:border-neutral-100";
const stepUpcoming =
  "border-neutral-900/12 bg-white dark:bg-neutral-900 text-neutral-900/66 dark:text-neutral-100/66 dark:border-neutral-100/14";
const toolWell =
  "flex min-w-0 flex-col gap-3 rounded-[var(--rb-r-sm,6px)] bg-neutral-900/5 px-3 py-2 dark:bg-neutral-100/5";
const fadeTop =
  "bg-linear-to-b from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeBottom =
  "bg-linear-to-t from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";

const metaText = cx(typeBodySm, text3);

type StepState = "done" | "running" | "paused" | "queued" | "skipped";

export type Step = {
  title: string;
  detail: string;
  target: string | null;
  estimate: string;
  skipped?: boolean;
};

const STEPS: Step[] = [
  {
    title: "Read the billing contract notes",
    detail: "Collected the pricing rules, renewal terms and license caps.",
    target: "docs/billing.md",
    estimate: "2 min",
  },
  {
    title: "Map the entitlement resolver",
    detail: "Traced how a key resolves to a tier before touching the branch.",
    target: "lib/registry-auth.ts",
    estimate: "3 min",
  },
  {
    title: "Patch the tier lookup",
    detail:
      "Adding the renewal fallback so expired keys keep access through the grace window, without changing the response shape.",
    target: "lib/registry-auth.ts",
    estimate: "6 min",
  },
  {
    title: "Add a renewal-grace test",
    detail: "Covers the expired-but-in-grace path from key to response.",
    target: "tests/entitlements.test.ts",
    estimate: "4 min",
  },
  {
    title: "Run the license checks",
    detail: "Verifies expired, starter, pro and ultimate keys each branch.",
    target: "npm test",
    estimate: "5 min",
  },
  {
    title: "Regenerate the entitlement fixtures",
    detail: "Skipped in review: the current fixtures already cover the change.",
    target: "scripts/fixtures.ts",
    estimate: "4 min",
    skipped: true,
  },
  {
    title: "Update the installation docs",
    detail: "Describes the renewal fallback in plain language.",
    target: "docs/installation.md",
    estimate: "3 min",
  },
];

const CURRENT = 2;

const STATE_LABEL: Record<StepState, string> = {
  done: "Done",
  running: "Running",
  paused: "Paused",
  queued: "Queued",
  skipped: "Skipped",
};

const STATUS_TONE: Record<StepState, string> = {
  done: statusOk,
  running: statusRunning,
  paused: statusIdle,
  queued: statusQueued,
  skipped: statusIdle,
};

const STEP_TONE: Record<StepState, string> = {
  done: stepDone,
  running: stepCurrent,
  paused: stepCurrent,
  queued: stepUpcoming,
  skipped: stepUpcoming,
};

const TITLE_TONE: Record<StepState, string> = {
  done: text2,
  running: text1,
  paused: text1,
  queued: text1,
  skipped: cx(text3, "line-through"),
};

function stateFor(steps: Step[], index: number, current: number, paused: boolean): StepState {
  if (steps[index]?.skipped) return "skipped";
  if (index < CURRENT) return "done";
  if (index === current) return paused ? "paused" : "running";
  return "queued";
}

function useScrollFade<T extends HTMLElement>() {
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
  }, [update]);

  return { ref, edges, onScroll: update };
}

export default function AgentPlan1({
  steps = STEPS,
  current = CURRENT,
  title,
}: {
  steps?: Step[];
  current?: number;
  title?: string;
}) {
  const [paused, setPaused] = useState(false);
  const [openIndex, setOpenIndex] = useState<number | null>(null);
  const {
    ref: stepsRef,
    edges: stepsEdges,
    onScroll: onStepsScroll,
  } = useScrollFade<HTMLOListElement>();

  const states = useMemo(
    () => steps.map((_, i) => stateFor(steps, i, current, paused)),
    [steps, current, paused],
  );

  const doneCount = states.filter((s) => s === "done").length;
  const progress = (doneCount / Math.max(steps.length, 1)) * 100;
  const expanded = openIndex === null ? current : openIndex;

  return (
    <div
      className={cx(
        canvas,
        text1,
        "relative flex h-full min-h-[560px] w-full flex-col overflow-hidden p-4 sm:p-6",
      )}
    >
      <header className="mb-3 flex shrink-0 items-center gap-3">
        <h2 className={cx(typeHeadingXs, "min-w-0 flex-1 truncate")}>
          {title ?? "Response plan"}
        </h2>
        <div className="flex shrink-0 items-center gap-2">
          <div aria-hidden="true" className="w-20 max-sm:hidden">
            <div className={progressTrack}>
              <div className={progressFill} style={{ width: `${progress}%` }} />
            </div>
          </div>
          <span className={cx(metaText, "tabular-nums")}>
            {doneCount} of {steps.length} done
          </span>
        </div>
        <button
          type="button"
          onClick={() => setPaused((p) => !p)}
          className={cx(secondaryButton, buttonMd)}
        >
          <HugeiconsIcon
            icon={paused ? PlayIcon : SquareIcon}
            size={16}
            strokeWidth={1.8}
            aria-hidden="true"
          />
          {paused ? "Resume" : "Stop"}
        </button>
      </header>

      <p className="sr-only" aria-live="polite">
        {steps[current]
          ? paused
            ? `Paused on ${steps[current].title}.`
            : `Working on ${steps[current].title}.`
          : "No active step."}
      </p>

      <div className={cx(panel, "min-h-0 flex-1")}>
        <div className={cx(panelBody, "relative mt-1 min-h-0 flex-1")}>
          <ol
            ref={stepsRef}
            onScroll={onStepsScroll}
            className={cx(
              rowsRuled,
              "h-full overflow-y-auto overscroll-contain",
            )}
          >
            {steps.map((step, i) => {
              const state = states[i];
              const isOpen = expanded === i;
              const current = state === "running" || state === "paused";

              return (
                <li
                  key={step.title}
                  aria-current={current ? "step" : undefined}
                  className="p-1"
                >
                  <button
                    type="button"
                    aria-expanded={isOpen}
                    onClick={() => setOpenIndex(isOpen ? -1 : i)}
                    className={cx(
                      itemRowSm,
                      itemRowInteractive,
                      cornerSm,
                      focusRingInset,
                      "h-9 content-center px-2",
                    )}
                  >
                    <span
                      className={cx(stepIndicator, STEP_TONE[state], "h-7 w-7")}
                    >
                      {state === "done" ? (
                        <HugeiconsIcon
                          icon={Tick02Icon}
                          size={12}
                          strokeWidth={1.8}
                          aria-hidden="true"
                        />
                      ) : (
                        i + 1
                      )}
                    </span>
                    <span
                      className={cx(
                        typeLabel,
                        TITLE_TONE[state],
                        "min-w-0 flex-1 truncate",
                      )}
                    >
                      {step.title}
                    </span>
                    <span className="flex w-20 shrink-0 items-center justify-end gap-1.5">
                      <span className={cx(statusWord, STATUS_TONE[state])}>
                        {state === "running" && (
                          <span className={arcLine}>
                            <span aria-hidden="true" className={runningArc} />
                          </span>
                        )}
                        {STATE_LABEL[state]}
                      </span>
                    </span>
                  </button>

                  {isOpen && (
                    <div className={cx(toolWell, "ms-12 me-2 mt-2 mb-2")}>
                      <p className={cx(typeBodySm, text2)}>{step.detail}</p>
                      <dl className="flex flex-wrap gap-x-8 gap-y-1">
                        <div className="flex min-w-0 items-center gap-2">
                          <dt className={cx(typeCaption, text3, "shrink-0")}>
                            Target
                          </dt>
                          <dd
                            className={cx(
                              step.target
                                ? cx(typeCodeSm, text2)
                                : cx(typeCaption, emptyValue),
                              "min-w-0 truncate",
                            )}
                          >
                            {step.target ?? "None"}
                          </dd>
                        </div>
                        <div className="flex items-center gap-2">
                          <dt className={cx(typeCaption, text3, "shrink-0")}>
                            Estimate
                          </dt>
                          <dd className={cx(typeCaption, text2)}>
                            {step.estimate}
                          </dd>
                        </div>
                      </dl>
                    </div>
                  )}
                </li>
              );
            })}
          </ol>

          <div
            aria-hidden="true"
            className={cx(
              fadeTop,
              "pointer-events-none absolute inset-x-0 top-0 h-8",
              stepsEdges.start ? "opacity-100" : "opacity-0",
            )}
          />
          <div
            aria-hidden="true"
            className={cx(
              fadeBottom,
              "pointer-events-none absolute inset-x-0 bottom-0 h-8",
              stepsEdges.end ? "opacity-100" : "opacity-0",
            )}
          />
        </div>
      </div>
    </div>
  );
}
