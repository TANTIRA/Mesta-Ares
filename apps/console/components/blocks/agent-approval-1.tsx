"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useReducedMotion } from "motion/react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowRight01Icon,
  MinusSignIcon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const typeCodeSm = "font-mono text-[12px]/[16px] tabular-nums";
const focusRingInset =
  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))]";
const primaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_94%,black))] text-[var(--rb-accent-fg,oklch(100%_0_0))] shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.22),0_2px_5px_-2px_rgb(0_0_0/0.3)] hover:not-disabled:brightness-110 active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] disabled:opacity-40 disabled:shadow-none dark:bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_94%,black))] dark:text-[var(--rb-accent-fg,oklch(20.5%_0_0))] dark:shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.66),0_2px_5px_-2px_rgb(0_0_0/0.9)] dark:active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] dark:disabled:shadow-none";
const secondaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] border border-neutral-900/12 bg-white text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] hover:not-disabled:border-neutral-900/32 hover:not-disabled:bg-linear-to-b hover:not-disabled:from-neutral-900/3 hover:not-disabled:to-neutral-900/3 active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:border-neutral-100/32 dark:hover:not-disabled:from-neutral-100/3 dark:hover:not-disabled:to-neutral-100/3 dark:active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.4)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const buttonSm =
  "h-7 gap-1.5 rounded-[var(--rb-r-sm,6px)] px-3 text-[14px]/[16px] font-[450]";
const inkSlot =
  "inline-flex h-6 shrink-0 items-center whitespace-nowrap rounded-[var(--rb-r-xs,4px)] border border-neutral-900/12 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 px-2 text-[14px]/[16px] font-[450] tabular-nums text-neutral-900 dark:text-neutral-100";
const panel =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-neutral-50 text-neutral-900 dark:text-neutral-100 dark:bg-neutral-900";
const panelBody =
  "mx-1 mb-1 flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-white [&:has(+*)]:mb-0 dark:border-neutral-100/8 dark:bg-neutral-950";
const panelFoot =
  "flex min-h-11 min-w-0 items-center gap-2 px-4 py-2 text-[13px]/[18px] text-neutral-900/66 dark:text-neutral-100/66 has-[button]:justify-end has-[button]:pe-2";
const disclosureRow =
  "group -mx-2 flex min-h-8 max-w-[calc(100%+16px)] min-w-0 cursor-pointer items-center gap-2 self-start rounded-[var(--rb-r-sm,6px)] px-2 text-left text-[13px]/[18px] text-neutral-900/80 dark:text-neutral-100/80 transition-[background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:bg-neutral-900/5 active:bg-neutral-900/8 dark:hover:bg-neutral-100/5 dark:active:bg-neutral-100/8 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))]";
const disclosureCaret =
  "shrink-0 text-neutral-900/66 dark:text-neutral-100/66 transition-transform duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none group-aria-expanded:rotate-90";
const userBubble =
  "w-fit max-w-full rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 px-3.5 py-2 text-[14px]/[20px] whitespace-pre-wrap text-neutral-900 dark:text-neutral-100 [overflow-wrap:anywhere]";
const prose =
  "flex min-w-0 flex-col gap-3 text-[14px]/[20px] text-neutral-900 dark:text-neutral-100 [overflow-wrap:anywhere]";
const stepGlyph = "flex size-4 shrink-0 items-center justify-center";
const toolDetail =
  "min-w-[6ch] truncate text-neutral-900 dark:text-neutral-100";
const toolEnd =
  "ms-auto flex shrink-0 items-center gap-2 whitespace-nowrap tabular-nums text-neutral-900/66 dark:text-neutral-100/66";
const toolWell =
  "flex min-w-0 flex-col gap-3 rounded-[var(--rb-r-sm,6px)] bg-neutral-900/5 px-3 py-2 dark:bg-neutral-100/5";
const toolOutput =
  "min-w-0 overflow-x-auto font-mono text-[12px]/[16px] whitespace-pre-wrap text-neutral-900/80 dark:text-neutral-100/80 [overflow-wrap:anywhere]";
const approvalQuestion =
  "min-w-0 py-2 text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100";
const fadeTop =
  "bg-linear-to-b from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeBottom =
  "bg-linear-to-t from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";

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

const REQUEST_ARGS = `delete_projects({
  workspace: "storage-cleanup",
  ids: ["prj_9f2a", "prj_7c10", "prj_5b83", … +9],
  purgeBuildCache: true
})`;

type Decision = "pending" | "approved" | "rejected";

export default function AgentApproval1({
  prompt = "Clean up the archived projects: we’re over the storage quota again.",
  rationale = "Twelve archived projects haven’t been opened in over a year. Removing them and their build history frees about 4.2 GB. Because this is permanent, I need you to confirm before I run it.",
  question = "Delete 12 archived projects and their build history",
  summary = "Frees 4.2 GB in the storage-cleanup workspace. Active projects are untouched and this can’t be undone.",
  requestArgs = REQUEST_ARGS,
  approveLabel = "Delete 12 projects",
  approvedText = "Approved: deleted 12 projects, freed 4.2 GB",
  approvedNote = "Done: your archive is cleaned up. I’ll flag projects for review the next time you pass 90% of the quota.",
  onDecide,
}: {
  prompt?: string;
  rationale?: string;
  question?: string;
  summary?: string;
  requestArgs?: string;
  approveLabel?: string;
  approvedText?: string;
  approvedNote?: string;
  onDecide?: (decision: "approved" | "rejected") => void;
}) {
  const reduceMotion = useReducedMotion();
  const [decision, setDecision] = useState<Decision>("pending");
  const [showRequest, setShowRequest] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const {
    ref: bodyRef,
    edges: bodyEdges,
    onScroll: onBodyScroll,
  } = useScrollFade<HTMLDivElement>();

  useEffect(() => {
    if (!announcement) return;
    const id = setTimeout(() => setAnnouncement(""), 2400);
    return () => clearTimeout(id);
  }, [announcement]);

  useEffect(() => {
    if (decision === "pending") return;
    const el = bodyRef.current;
    if (!el) return;
    el.scrollTo({
      top: el.scrollHeight,
      behavior: reduceMotion ? "auto" : "smooth",
    });
  }, [decision, reduceMotion, bodyRef]);

  return (
    <div
      className={cx(
        canvas,
        text1,
        "flex h-full min-h-[480px] w-full flex-col overflow-hidden",
      )}
    >
      <div className="relative min-h-0 flex-1">
        <div
          ref={bodyRef}
          onScroll={onBodyScroll}
          className="h-full overflow-y-auto overscroll-contain [overflow-anchor:none] px-4 py-6 sm:px-6"
        >
          <div className="mx-auto flex max-w-3xl flex-col gap-6">
            <p className={cx(typeCaption, text3, "text-center")}>Today</p>

            <div className="flex justify-end">
              <div className="max-w-[85%]">
                <p className={userBubble}>{prompt}</p>
              </div>
            </div>

            <p className={prose}>{rationale}</p>

            <div
              className={cx(typeBodySm, text2, "flex h-8 items-center gap-2")}
            >
              <span className={stepGlyph}>
                <HugeiconsIcon
                  icon={Tick02Icon}
                  size={12}
                  strokeWidth={1.8}
                  aria-hidden="true"
                  className={text3}
                />
              </span>
              <span className={cx(toolDetail, typeCodeSm)}>list_projects</span>
              <span className={toolEnd}>0.8s</span>
            </div>

            {decision === "pending" ? (
              <section aria-labelledby="aa1-title" className={panel}>
                <div className="flex min-w-0 flex-col gap-1 px-4 pt-3 pb-1">
                  <div className="flex h-6 min-w-0 items-center">
                    <span className={inkSlot}>Needs approval</span>
                  </div>
                  <p
                    id="aa1-title"
                    className={cx(approvalQuestion, "text-balance")}
                  >
                    {question}
                  </p>
                </div>
                <div
                  className={cx(
                    panelBody,
                    "gap-1 px-3 pt-3",
                    showRequest ? "pb-3" : "pb-1",
                  )}
                >
                  <p className={cx(typeBodySm, text2)}>{summary}</p>
                  <button
                    type="button"
                    onClick={() => setShowRequest((v) => !v)}
                    aria-expanded={showRequest}
                    className={disclosureRow}
                  >
                    <span className={stepGlyph}>
                      <HugeiconsIcon
                        icon={ArrowRight01Icon}
                        size={12}
                        strokeWidth={1.8}
                        aria-hidden="true"
                        className={disclosureCaret}
                      />
                    </span>
                    {showRequest ? "Hide request" : "Show request"}
                  </button>
                  {showRequest && (
                    <pre
                      tabIndex={0}
                      className={cx(
                        toolWell,
                        toolOutput,
                        focusRingInset,
                        "max-h-40 overflow-auto",
                      )}
                    >
                      {requestArgs}
                    </pre>
                  )}
                </div>
                <div className={panelFoot}>
                  <button
                    type="button"
                    onClick={() => {
                      setDecision("rejected");
                      setAnnouncement("Skipped, not approved.");
                      onDecide?.("rejected");
                    }}
                    className={cx(secondaryButton, buttonSm)}
                  >
                    Reject
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setDecision("approved");
                      setAnnouncement(`Approved. ${approvedText}`);
                      onDecide?.("approved");
                    }}
                    className={cx(primaryButton, buttonSm)}
                  >
                    {approveLabel}
                  </button>
                </div>
              </section>
            ) : (
              <div
                className={cx(
                  typeBodySm,
                  text2,
                  "flex min-h-8 items-center gap-2",
                )}
              >
                <span className={stepGlyph}>
                  <HugeiconsIcon
                    icon={decision === "approved" ? Tick02Icon : MinusSignIcon}
                    size={12}
                    strokeWidth={1.8}
                    aria-hidden="true"
                    className={text3}
                  />
                </span>
                <p className="min-w-0 flex-1 truncate">
                  {decision === "approved" ? approvedText : "Skipped, not approved"}
                </p>
                <span className={toolEnd}>just now</span>
              </div>
            )}

            {decision === "approved" && (
              <p className={prose}>{approvedNote}</p>
            )}
          </div>
        </div>

        <div
          aria-hidden="true"
          className={cx(
            fadeTop,
            "pointer-events-none absolute inset-x-0 top-0 h-8",
            bodyEdges.start ? "opacity-100" : "opacity-0",
          )}
        />
        <div
          aria-hidden="true"
          className={cx(
            fadeBottom,
            "pointer-events-none absolute inset-x-0 bottom-0 h-8",
            bodyEdges.end ? "opacity-100" : "opacity-0",
          )}
        />
      </div>

      <span className="sr-only" aria-live="polite">
        {announcement}
      </span>
    </div>
  );
}
