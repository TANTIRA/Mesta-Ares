"use client";

import { useMemo, useState } from "react";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const selectedFill = "bg-neutral-900/9 dark:bg-neutral-100/9";
const cornerSm = "rounded-[var(--rb-r-sm,6px)]";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const chartTone1 = "text-neutral-900 dark:text-neutral-100";
const chartTone3 = "text-neutral-900/66 dark:text-neutral-100/66";
const rowsRuled = "divide-y divide-neutral-900/8 dark:divide-neutral-100/8";
const typeHeadingXs = "text-[16px]/[20px] font-[450] tracking-[-0.015em]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const focusRingInset =
  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))]";
const transition =
  "transition-[color,background-color,border-color,box-shadow,outline-color,opacity,scale] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:duration-[60ms]";
const segmentedTrack =
  "inline-flex w-fit max-w-full items-stretch gap-0.5 rounded-[var(--rb-r-sm,6px)] bg-neutral-900/5 p-0.5 dark:bg-neutral-100/5";
const segmentedMd = "h-8";
const segment =
  "relative inline-flex min-w-0 cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap [&_svg]:shrink-0 rounded-[var(--rb-r-xs,4px)] px-3 text-[14px]/[16px] font-[450] text-neutral-900/80 select-none transition-[background-color,color,outline-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-aria-checked:bg-neutral-900/5 hover:not-aria-checked:text-neutral-900 active:not-aria-checked:bg-neutral-900/8 aria-checked:bg-white aria-checked:text-neutral-900 aria-checked:shadow-[0_1px_2px_rgb(0_0_0/0.1)] disabled:cursor-not-allowed disabled:text-neutral-900/40 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:text-neutral-100/80 dark:hover:not-aria-checked:bg-neutral-100/5 dark:hover:not-aria-checked:text-neutral-100 dark:active:not-aria-checked:bg-neutral-100/8 dark:aria-checked:bg-neutral-700 dark:aria-checked:text-neutral-100 dark:aria-checked:shadow-[0_0_0_1px_rgb(255_255_255/0.16),0_1px_2px_rgb(0_0_0/0.3)] dark:disabled:text-neutral-100/40";
const panel =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-neutral-50 text-neutral-900 dark:text-neutral-100 dark:bg-neutral-900";
const panelBody =
  "mx-1 mb-1 flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-white [&:has(+*)]:mb-0 dark:border-neutral-100/8 dark:bg-neutral-950";
const statLabel =
  "text-[14px]/[16px] font-[450] text-neutral-900/80 dark:text-neutral-100/80";
const statValueSm =
  "flex min-w-0 flex-wrap items-baseline gap-x-1 text-[20px]/[24px] font-[450] tabular-nums text-neutral-900 dark:text-neutral-100";
const chartSwatch = "inline-block size-2 shrink-0";

const metaText = cx(typeBodySm, text3);
const inputMark = cx(chartTone1, "bg-current");
const outputMark = cx(chartTone3, "bg-current");

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
  e.currentTarget
    .querySelectorAll<HTMLElement>('[role="radio"]')
    [next]?.focus();
}

const compact = (n: number) =>
  Math.abs(n) >= 1_000_000
    ? `${(n / 1_000_000).toFixed(Math.abs(n) >= 10_000_000 ? 0 : 1)}M`
    : Math.abs(n) >= 1_000
      ? `${(n / 1_000).toFixed(Math.abs(n) >= 10_000 ? 0 : 1)}k`
      : `${n}`;

const money = (n: number) =>
  n >= 100
    ? `$${n.toLocaleString("en-US", { maximumFractionDigits: 0 })}`
    : `$${n.toFixed(2)}`;

const RANGES = ["7d", "30d", "90d"] as const;
type Range = (typeof RANGES)[number];

type Day = { label: string; input: number; output: number };

const SERIES: Record<Range, Day[]> = {
  "7d": [
    { label: "Mon", input: 4.1, output: 1.6 },
    { label: "Tue", input: 5.3, output: 2.2 },
    { label: "Wed", input: 6.8, output: 2.6 },
    { label: "Thu", input: 5.9, output: 2.4 },
    { label: "Fri", input: 8.2, output: 3.4 },
    { label: "Sat", input: 2.4, output: 0.9 },
    { label: "Sun", input: 1.9, output: 0.7 },
  ],
  "30d": [
    { label: "W1", input: 22.4, output: 8.8 },
    { label: "W2", input: 26.1, output: 10.4 },
    { label: "W3", input: 31.7, output: 12.9 },
    { label: "W4", input: 34.6, output: 13.8 },
  ],
  "90d": [
    { label: "Apr", input: 71.2, output: 27.9 },
    { label: "May", input: 88.4, output: 35.1 },
    { label: "Jun", input: 96.8, output: 39.4 },
  ],
};

const RATE_IN = 3 / 1_000_000;
const RATE_OUT = 15 / 1_000_000;

export default function AiUsage1() {
  const [range, setRange] = useState<Range>("7d");
  const [active, setActive] = useState<number | null>(null);

  const days = SERIES[range];

  const totals = useMemo(() => {
    const input = days.reduce((sum, d) => sum + d.input, 0) * 1_000_000;
    const output = days.reduce((sum, d) => sum + d.output, 0) * 1_000_000;
    return {
      input,
      output,
      tokens: input + output,
      cost: input * RATE_IN + output * RATE_OUT,
    };
  }, [days]);

  const max = Math.max(...days.map((d) => d.input + d.output));

  const shown = active === null ? null : days[active];
  const shownCost = shown
    ? shown.input * 1_000_000 * RATE_IN + shown.output * 1_000_000 * RATE_OUT
    : null;

  const pick = (option: Range) => {
    setRange(option);
    setActive(null);
  };

  return (
    <div
      className={cx(
        canvas,
        text1,
        "flex h-full min-h-[480px] w-full flex-col justify-center-safe overflow-y-auto p-6 sm:p-8",
      )}
    >
      <div className="mx-auto w-full max-w-[840px]">
        <div className="mb-3 flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h2 className={typeHeadingXs}>Token usage</h2>
            <p className={cx(metaText, "mt-0.5 truncate")}>
              Input and output tokens billed to this workspace.
            </p>
          </div>

          <div
            role="radiogroup"
            aria-label="Range"
            onKeyDown={(e) =>
              moveChoice(e, RANGES.length, RANGES.indexOf(range), (i) =>
                pick(RANGES[i]),
              )
            }
            className={cx(segmentedTrack, segmentedMd, "shrink-0")}
          >
            {RANGES.map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={option === range}
                tabIndex={option === range ? 0 : -1}
                onClick={() => pick(option)}
                className={segment}
              >
                {option}
              </button>
            ))}
          </div>
        </div>

        <div className={panel}>
          <div className={cx(panelBody, rowsRuled, "mt-1")}>
            <div className="grid grid-cols-2 sm:grid-cols-4">
              {(
                [
                  ["Total tokens", compact(totals.tokens)],
                  ["Input", compact(totals.input)],
                  ["Output", compact(totals.output)],
                  ["Spend", money(totals.cost)],
                ] as const
              ).map(([label, value]) => (
                <div key={label} className="flex min-w-0 flex-col gap-1 p-3">
                  <p className={statLabel}>{label}</p>
                  <p className={statValueSm}>{value}</p>
                </div>
              ))}
            </div>

            <div className="p-3">
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-4">
                  <span
                    className={cx(metaText, "inline-flex items-center gap-1.5")}
                  >
                    <span
                      aria-hidden="true"
                      className={cx(chartSwatch, inputMark)}
                    />
                    Input
                  </span>
                  <span
                    className={cx(metaText, "inline-flex items-center gap-1.5")}
                  >
                    <span
                      aria-hidden="true"
                      className={cx(chartSwatch, outputMark)}
                    />
                    Output
                  </span>
                </div>
                <p className={cx(metaText, "flex min-w-0 gap-x-3 truncate")}>
                  {shown ? (
                    <>
                      <span>{shown.label}</span>
                      <span className="tabular-nums">
                        {compact((shown.input + shown.output) * 1_000_000)}
                      </span>
                      <span className="tabular-nums">
                        {money(shownCost ?? 0)}
                      </span>
                    </>
                  ) : (
                    "Hover a bar for detail"
                  )}
                </p>
              </div>

              <div
                className="mt-4 flex h-44 items-end gap-2"
                onMouseLeave={() => setActive(null)}
              >
                {days.map((day, i) => {
                  const total = day.input + day.output;
                  const isActive = active === i;
                  return (
                    <button
                      key={day.label}
                      type="button"
                      aria-label={`${day.label}: ${compact(
                        total * 1_000_000,
                      )} tokens`}
                      onMouseEnter={() => setActive(i)}
                      onFocus={() => setActive(i)}
                      onBlur={() => setActive(null)}
                      className={cx(
                        focusRingInset,
                        transition,
                        cornerSm,
                        isActive && selectedFill,
                        "group flex h-full flex-1 cursor-pointer flex-col justify-end gap-1",
                      )}
                    >
                      <div
                        className="mx-auto flex w-full max-w-[36px] flex-col justify-end"
                        style={{ height: `${(total / max) * 100}%` }}
                      >
                        <div
                          className={cx(outputMark, "w-full")}
                          style={{ height: `${(day.output / total) * 100}%` }}
                        />
                        <div
                          className={cx(inputMark, "w-full")}
                          style={{ height: `${(day.input / total) * 100}%` }}
                        />
                      </div>
                      <span
                        className={cx(typeCaption, isActive ? text1 : text3)}
                      >
                        {day.label}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
