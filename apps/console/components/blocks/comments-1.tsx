"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  CornerUpLeftIcon,
  MoreHorizontalIcon,
  SmileIcon,
} from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const rowsRuled = "divide-y divide-neutral-900/8 dark:divide-neutral-100/8";
const ruleEnd = "border-r border-neutral-900/8 dark:border-neutral-100/8";
const typeHeadingXs = "text-[16px]/[20px] font-[450] tracking-[-0.015em]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeLabel = "text-[14px]/[16px] font-[450]";
const primaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_94%,black))] text-[var(--rb-accent-fg,oklch(100%_0_0))] shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.22),0_2px_5px_-2px_rgb(0_0_0/0.3)] hover:not-disabled:brightness-110 active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] disabled:opacity-40 disabled:shadow-none dark:bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_94%,black))] dark:text-[var(--rb-accent-fg,oklch(20.5%_0_0))] dark:shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.66),0_2px_5px_-2px_rgb(0_0_0/0.9)] dark:active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] dark:disabled:shadow-none";
const secondaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] border border-neutral-900/12 bg-white text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] hover:not-disabled:border-neutral-900/32 hover:not-disabled:bg-linear-to-b hover:not-disabled:from-neutral-900/3 hover:not-disabled:to-neutral-900/3 active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:border-neutral-100/32 dark:hover:not-disabled:from-neutral-100/3 dark:hover:not-disabled:to-neutral-100/3 dark:active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.4)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const ghostButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-transparent text-neutral-900 hover:not-disabled:bg-neutral-900/5 active:bg-neutral-900/8 aria-pressed:bg-neutral-900/9 disabled:text-neutral-900/40 dark:text-neutral-100 dark:hover:not-disabled:bg-neutral-100/5 dark:active:bg-neutral-100/8 dark:aria-pressed:bg-neutral-100/9 dark:disabled:text-neutral-100/40";
const buttonSm =
  "h-7 gap-1.5 rounded-[var(--rb-r-sm,6px)] px-3 text-[14px]/[16px] font-[450]";
const iconButtonSm = "size-7 rounded-[var(--rb-r-sm,6px)]";
const segmentedTrack =
  "inline-flex w-fit max-w-full items-stretch gap-0.5 rounded-[var(--rb-r-sm,6px)] bg-neutral-900/5 p-0.5 dark:bg-neutral-100/5";
const segmentedMd = "h-8";
const segment =
  "relative inline-flex min-w-0 cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap [&_svg]:shrink-0 rounded-[var(--rb-r-xs,4px)] px-3 text-[14px]/[16px] font-[450] text-neutral-900/80 select-none transition-[background-color,color,outline-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-aria-checked:bg-neutral-900/5 hover:not-aria-checked:text-neutral-900 active:not-aria-checked:bg-neutral-900/8 aria-checked:bg-white aria-checked:text-neutral-900 aria-checked:shadow-[0_1px_2px_rgb(0_0_0/0.1)] disabled:cursor-not-allowed disabled:text-neutral-900/40 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:text-neutral-100/80 dark:hover:not-aria-checked:bg-neutral-100/5 dark:hover:not-aria-checked:text-neutral-100 dark:active:not-aria-checked:bg-neutral-100/8 dark:aria-checked:bg-neutral-700 dark:aria-checked:text-neutral-100 dark:aria-checked:shadow-[0_0_0_1px_rgb(255_255_255/0.16),0_1px_2px_rgb(0_0_0/0.3)] dark:disabled:text-neutral-100/40";
const chip =
  "relative inline-flex h-7 max-w-full min-w-0 shrink-0 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-[var(--rb-r-sm,6px)] border border-neutral-900/12 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 px-2.5 text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100 select-none shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] transition-[background-color,border-color,color,scale,outline-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 hover:not-disabled:border-neutral-900/32 aria-pressed:border-neutral-900/32 aria-pressed:bg-neutral-900/9 aria-expanded:border-neutral-900/32 disabled:cursor-not-allowed disabled:text-neutral-900/40 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:hover:not-disabled:border-neutral-100/32 dark:aria-pressed:border-neutral-100/32 dark:aria-pressed:bg-neutral-100/16 dark:aria-expanded:border-neutral-100/32 dark:disabled:text-neutral-100/40";
const avatar =
  "relative inline-flex size-8 shrink-0 overflow-hidden rounded-full select-none";
const avatarFallback =
  "flex size-full items-center justify-center rounded-[inherit] border border-neutral-900/12 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 text-[12px]/[16px] font-[450] tabular-nums text-neutral-900 dark:text-neutral-100";
const panel =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-neutral-50 text-neutral-900 dark:text-neutral-100 dark:bg-neutral-900";
const panelBody =
  "mx-1 mb-1 flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-white [&:has(+*)]:mb-0 dark:border-neutral-100/8 dark:bg-neutral-950";
const composer =
  "relative flex w-full min-w-0 flex-col rounded-[var(--rb-r-lg,10px)] border border-neutral-900/12 bg-white text-neutral-900 dark:text-neutral-100 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] transition-[border-color,box-shadow] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-focus-within:border-neutral-900/32 focus-within:border-[var(--rb-accent,oklch(20.5%_0_0))] focus-within:ring-3 focus-within:ring-neutral-900/8 dark:border-neutral-100/14 dark:bg-neutral-900 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-focus-within:border-neutral-100/32 dark:focus-within:border-[var(--rb-accent,oklch(100%_0_0))] dark:focus-within:ring-neutral-100/8";
const composerInput =
  "max-h-40 min-h-5 min-w-0 flex-auto resize-none bg-transparent px-1.5 py-1 field-sizing-content text-[14px]/[20px] text-neutral-900 dark:text-neutral-100 outline-none placeholder:text-neutral-900/66 dark:placeholder:text-neutral-100/66";
const fadeTop =
  "bg-linear-to-b from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeBottom =
  "bg-linear-to-t from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";

const initials = (name: string, count = 2) =>
  name
    .split(/[\s-]+/)
    .filter(Boolean)
    .slice(0, count)
    .map((w) => w[0]?.toUpperCase())
    .join("");

type Reply = {
  id: string;
  author: string;
  role: string;
  time: string;
  body: string;
};

type Thread = Reply & { reactions: number; replies: Reply[] };

const THREADS: Thread[] = [
  {
    id: "t1",
    author: "Priya Nandakumar",
    role: "Platform",
    time: "2h ago",
    body: "The cutover window is tight. If we hold the read replica until the backfill drains we lose the rollback path, so I would rather split this into two deploys.",
    reactions: 4,
    replies: [
      {
        id: "t1r1",
        author: "Marcus Bell",
        role: "Infrastructure",
        time: "1h ago",
        body: "Agreed. Two deploys also means the schema change lands on a quiet day instead of during the migration itself.",
      },
      {
        id: "t1r2",
        author: "Sofia Alvarez",
        role: "Platform",
        time: "48m ago",
        body: "I can prepare the second deploy behind a flag so we can flip it without another release.",
      },
    ],
  },
  {
    id: "t2",
    author: "Wei Chen",
    role: "Data",
    time: "36m ago",
    body: "One caveat: the backfill job reads from the same connection pool as the API. We should cap it at eight workers or the request latency will spike during business hours.",
    reactions: 2,
    replies: [],
  },
  {
    id: "t3",
    author: "Tomás Guerrero",
    role: "Reliability",
    time: "12m ago",
    body: "Added a dashboard for pool saturation so we can watch it live. Alert fires at 80 percent for five minutes.",
    reactions: 1,
    replies: [],
  },
];

const SORTS = ["Newest", "Oldest"] as const;
type Sort = (typeof SORTS)[number];

const onRadioKeys = (e: React.KeyboardEvent<HTMLDivElement>) => {
  const step =
    e.key === "ArrowRight" || e.key === "ArrowDown"
      ? 1
      : e.key === "ArrowLeft" || e.key === "ArrowUp"
        ? -1
        : 0;
  if (!step && e.key !== "Home" && e.key !== "End") return;
  const radios = Array.from(
    e.currentTarget.querySelectorAll<HTMLButtonElement>(
      '[role=\"radio\"]:not(:disabled)',
    ),
  );
  const from = radios.indexOf(e.target as HTMLButtonElement);
  if (from < 0) return;
  e.preventDefault();
  const to =
    e.key === "Home"
      ? 0
      : e.key === "End"
        ? radios.length - 1
        : (from + step + radios.length) % radios.length;
  radios[to].focus();
  radios[to].click();
};

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

export default function Comments1() {
  const [sort, setSort] = useState<Sort>("Oldest");
  const [liked, setLiked] = useState<string[]>(["t1"]);
  const [draft, setDraft] = useState("");
  const {
    ref: listRef,
    edges: listEdges,
    onScroll: onListScroll,
  } = useScrollFade<HTMLDivElement>();

  const threads = useMemo(
    () => (sort === "Oldest" ? THREADS : [...THREADS].reverse()),
    [sort],
  );

  const total = THREADS.reduce((sum, t) => sum + 1 + t.replies.length, 0);

  const toggleLike = (id: string) =>
    setLiked((current) =>
      current.includes(id) ? current.filter((v) => v !== id) : [...current, id],
    );

  const Actions = ({ id, reactions }: { id: string; reactions: number }) => {
    const on = liked.includes(id);
    return (
      <div className="mt-2 flex items-center gap-1">
        <button
          type="button"
          aria-pressed={on}
          onClick={() => toggleLike(id)}
          className={cx(chip, "tabular-nums")}
        >
          <HugeiconsIcon
            icon={SmileIcon}
            size={14}
            strokeWidth={1.8}
            aria-hidden="true"
          />
          {reactions + (on ? 1 : 0)}
        </button>
        <button type="button" className={cx(secondaryButton, buttonSm)}>
          <HugeiconsIcon
            icon={CornerUpLeftIcon}
            size={16}
            strokeWidth={1.8}
            aria-hidden="true"
          />
          Reply
        </button>
      </div>
    );
  };

  return (
    <div
      className={cx(
        canvas,
        text1,
        "flex h-full min-h-[640px] w-full flex-col justify-center-safe overflow-y-auto p-6 sm:p-8",
      )}
    >
      <div className="mx-auto w-full max-w-[720px]">
        <div className="mb-3 flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h2 className={cx(typeHeadingXs, text1)}>
              Migration plan feedback
            </h2>
            <p className={cx(typeBodySm, text3, "mt-0.5 flex gap-3 truncate")}>
              <span>{total} comments</span>
              <span>4 participants</span>
            </p>
          </div>

          <div
            role="radiogroup"
            aria-label="Sort comments"
            onKeyDown={onRadioKeys}
            className={cx(segmentedTrack, segmentedMd, "shrink-0")}
          >
            {SORTS.map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={option === sort}
                tabIndex={option === sort ? 0 : -1}
                onClick={() => setSort(option)}
                className={segment}
              >
                {option}
              </button>
            ))}
          </div>
        </div>

        <div className={panel}>
          <div className={cx(panelBody, "relative mt-1")}>
            <div
              ref={listRef}
              onScroll={onListScroll}
              className={cx(
                rowsRuled,
                "max-h-[280px] overflow-y-auto sm:max-h-[368px]",
              )}
            >
              {threads.map((thread) => (
                <article key={thread.id} className="flex gap-3 p-3">
                  <div className="flex shrink-0 flex-col items-center">
                    <span aria-hidden className={avatar}>
                      <span className={avatarFallback}>
                        {initials(thread.author)}
                      </span>
                    </span>
                    {thread.replies.length > 0 && (
                      <span
                        aria-hidden
                        className={cx(ruleEnd, "mt-2 w-px flex-1")}
                      />
                    )}
                  </div>

                  <div className="min-w-0 flex-1">
                    <div className="flex items-center justify-between gap-3">
                      <div className="flex min-h-7 min-w-0 flex-wrap content-center items-baseline gap-x-2">
                        <p className={cx(typeLabel, text1, "min-w-0 truncate")}>
                          {thread.author}
                        </p>
                        <p className={cx(typeBodySm, text3, "flex gap-x-2")}>
                          <span>{thread.role}</span>
                          <span>{thread.time}</span>
                        </p>
                      </div>
                      <button
                        type="button"
                        aria-label={`More actions on ${thread.author}'s comment`}
                        className={cx(ghostButton, iconButtonSm)}
                      >
                        <HugeiconsIcon
                          icon={MoreHorizontalIcon}
                          size={16}
                          strokeWidth={1.8}
                          aria-hidden="true"
                        />
                      </button>
                    </div>

                    <p className={cx(typeBodySm, text2, "mt-1")}>
                      {thread.body}
                    </p>

                    <Actions id={thread.id} reactions={thread.reactions} />

                    {thread.replies.length > 0 && (
                      <div className="mt-3 space-y-3">
                        {thread.replies.map((reply) => (
                          <div key={reply.id} className="flex gap-2">
                            <span
                              aria-hidden
                              className={cx(avatar, "size-[24px]")}
                            >
                              <span className={avatarFallback}>
                                {initials(reply.author, 1)}
                              </span>
                            </span>
                            <div className="min-w-0 flex-1">
                              <div className="flex min-h-6 min-w-0 flex-wrap content-center items-baseline gap-x-2">
                                <p
                                  className={cx(
                                    typeLabel,
                                    text1,
                                    "min-w-0 truncate",
                                  )}
                                >
                                  {reply.author}
                                </p>
                                <span className={cx(typeBodySm, text3)}>
                                  {reply.time}
                                </span>
                              </div>
                              <p className={cx(typeBodySm, text2, "mt-0.5")}>
                                {reply.body}
                              </p>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </article>
              ))}
            </div>
            <div
              aria-hidden="true"
              className={cx(
                fadeTop,
                "pointer-events-none absolute inset-x-0 top-0 h-8",
                listEdges.start ? "opacity-100" : "opacity-0",
              )}
            />
            <div
              aria-hidden="true"
              className={cx(
                fadeBottom,
                "pointer-events-none absolute inset-x-0 bottom-0 h-8",
                listEdges.end ? "opacity-100" : "opacity-0",
              )}
            />
          </div>
        </div>

        <div className={cx(composer, "mt-2 p-3")}>
          <label htmlFor="comments-1-draft" className="sr-only">
            Write a comment
          </label>
          <textarea
            id="comments-1-draft"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            rows={2}
            placeholder="Write a comment"
            className={cx(
              composerInput,
              "field-sizing-content min-h-12 w-full",
            )}
          />
          <div className="mt-2 flex items-center justify-between gap-3 ps-1.5">
            <p className={cx(typeBodySm, text3, "truncate")}>
              Mention a teammate with @ to notify them.
            </p>
            <button
              type="button"
              disabled={draft.trim().length === 0}
              className={cx(primaryButton, buttonSm)}
            >
              Comment
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
