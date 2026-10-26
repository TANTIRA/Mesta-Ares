"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  Heading01Icon,
  Heading02Icon,
  LeftToRightListBulletIcon,
  LeftToRightListNumberIcon,
  Link02Icon,
  QuoteDownIcon,
  Redo03Icon,
  SourceCodeIcon,
  TextBoldIcon,
  TextItalicIcon,
  TextStrikethroughIcon,
  Tick02Icon,
  Undo03Icon,
} from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const ruleTop = "border-t border-neutral-900/8 dark:border-neutral-100/8";
const ruleEnd = "border-r border-neutral-900/8 dark:border-neutral-100/8";
const cornerSm = "rounded-[var(--rb-r-sm,6px)]";
const typeHeading = "text-[24px]/[28px] font-[450] tracking-[-0.015em]";
const typeBodyLg = "text-[16px]/[22px] font-normal tracking-[-0.01em]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const focusRing =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))]";
const secondaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] border border-neutral-900/12 bg-white text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] hover:not-disabled:border-neutral-900/32 hover:not-disabled:bg-linear-to-b hover:not-disabled:from-neutral-900/3 hover:not-disabled:to-neutral-900/3 active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:border-neutral-100/32 dark:hover:not-disabled:from-neutral-100/3 dark:hover:not-disabled:to-neutral-100/3 dark:active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.4)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const ghostButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-transparent text-neutral-900 hover:not-disabled:bg-neutral-900/5 active:bg-neutral-900/8 aria-pressed:bg-neutral-900/9 disabled:text-neutral-900/40 dark:text-neutral-100 dark:hover:not-disabled:bg-neutral-100/5 dark:active:bg-neutral-100/8 dark:aria-pressed:bg-neutral-100/9 dark:disabled:text-neutral-100/40";
const buttonSm =
  "h-7 gap-1.5 rounded-[var(--rb-r-sm,6px)] px-3 text-[14px]/[16px] font-[450]";
const iconButtonSm = "size-7 rounded-[var(--rb-r-sm,6px)]";
const menuPopup =
  "min-w-48 max-w-[min(480px,calc(100vw-2rem))] overflow-x-hidden overflow-y-auto overscroll-contain rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 p-1 text-neutral-900 dark:text-neutral-100 outline-none shadow-[0_12px_24px_-12px_rgb(0_0_0/0.2)] dark:shadow-[0_12px_24px_-12px_rgb(0_0_0/0.6)]";
const menuItem =
  "relative flex min-h-8 w-full cursor-default items-center gap-2 rounded-[var(--rb-r-sm,6px)] px-2 py-1.5 text-left text-[14px]/[16px] font-[450] text-neutral-900 outline-none select-none [&_svg]:shrink-0 [&_svg]:text-neutral-900/80 hover:bg-neutral-900/5 focus-visible:bg-neutral-900/5 data-[highlighted]:bg-neutral-900/5 aria-selected:bg-neutral-900/9 aria-disabled:cursor-not-allowed aria-disabled:text-neutral-900/40 dark:text-neutral-100 dark:[&_svg]:text-neutral-100/80 dark:hover:bg-neutral-100/5 dark:focus-visible:bg-neutral-100/5 dark:data-[highlighted]:bg-neutral-100/5 dark:aria-selected:bg-neutral-100/9 dark:aria-disabled:text-neutral-100/40";
const topBar =
  "border-b border-neutral-900/8 bg-white dark:border-neutral-100/8 dark:bg-neutral-900";
const fadeStartOnTray =
  "bg-linear-to-r from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-900";
const fadeEndOnTray =
  "bg-linear-to-l from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-900";
const fadeTop =
  "bg-linear-to-b from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeBottom =
  "bg-linear-to-t from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";

const docTitle = cx(typeHeading, text1);
const docMeta = cx(typeBodySm, text3);
const docBody = cx(typeBodyLg, text2);
const barText = cx(typeCaption, text3);

type Mark = "bold" | "italic" | "strike" | "code" | "link";

type BlockId = "p" | "h1" | "h2" | "quote" | "ul" | "ol";

const BLOCKS: { id: BlockId; label: string; icon: IconSvgElement | null }[] = [
  { id: "p", label: "Paragraph", icon: null },
  { id: "h1", label: "Heading 1", icon: Heading01Icon },
  { id: "h2", label: "Heading 2", icon: Heading02Icon },
  { id: "quote", label: "Quote", icon: QuoteDownIcon },
  { id: "ul", label: "Bulleted list", icon: LeftToRightListBulletIcon },
  { id: "ol", label: "Numbered list", icon: LeftToRightListNumberIcon },
];

const MARKS: { id: Mark; label: string; icon: IconSvgElement; keys: string }[] =
  [
    { id: "bold", label: "Bold", icon: TextBoldIcon, keys: "Ctrl B" },
    { id: "italic", label: "Italic", icon: TextItalicIcon, keys: "Ctrl I" },
    {
      id: "strike",
      label: "Strikethrough",
      icon: TextStrikethroughIcon,
      keys: "Ctrl S",
    },
    { id: "code", label: "Inline code", icon: SourceCodeIcon, keys: "Ctrl E" },
    { id: "link", label: "Link", icon: Link02Icon, keys: "Ctrl K" },
  ];

const HISTORY: { label: string; icon: IconSvgElement }[] = [
  { label: "Undo", icon: Undo03Icon },
  { label: "Redo", icon: Redo03Icon },
];

const SEED = `Halcyon writes every permission change to an append-only ledger. Each entry records the actor, the resource, the previous value and the reason supplied at the time of the change.

Entries are immutable. A correction is a new entry that supersedes the old one, which means the history of a resource can always be replayed exactly as it happened.

Retention follows the workspace plan: ninety days on Team, one year on Scale, and seven years on Enterprise.`;

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

export default function Editor1() {
  const reduce = useReducedMotion();
  const [text, setText] = useState(SEED);
  const [block, setBlock] = useState<BlockId>("p");
  const [marks, setMarks] = useState<Mark[]>(["bold"]);
  const [menu, setMenu] = useState(false);
  const [menuInstant, setMenuInstant] = useState(false);
  const [menuActive, setMenuActive] = useState(0);
  const [saved, setSaved] = useState<"saved" | "saving">("saved");
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const areaRef = useRef<HTMLTextAreaElement>(null);
  const {
    ref: toolsRef,
    edges: toolsEdges,
    onScroll: onToolsScroll,
  } = useScrollFadeX<HTMLDivElement>();
  const {
    ref: docRef,
    edges: docEdges,
    onScroll: onDocScroll,
  } = useScrollFade<HTMLDivElement>(text);

  const stats = useMemo(() => {
    const words = text.trim() ? text.trim().split(/\s+/).length : 0;
    return {
      words,
      chars: text.length,
      read: Math.max(1, Math.round(words / 220)),
    };
  }, [text]);

  useEffect(() => {
    if (!menu) return;
    const node = menuRef.current;
    const doc = node?.ownerDocument ?? document;
    const onDown = (e: MouseEvent) => {
      if (node && !node.contains(e.target as Node)) setMenu(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setMenu(false);
        triggerRef.current?.focus({ preventScroll: true });
      }
    };
    doc.addEventListener("mousedown", onDown);
    doc.addEventListener("keydown", onKey);
    return () => {
      doc.removeEventListener("mousedown", onDown);
      doc.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  useEffect(() => {
    if (menu) itemRefs.current[menuActive]?.focus({ preventScroll: true });
  }, [menu, menuActive]);

  useEffect(() => {
    if (saved !== "saving") return;
    const t = window.setTimeout(() => setSaved("saved"), 900);
    return () => window.clearTimeout(t);
  }, [saved, text]);

  const toggleMark = (m: Mark) =>
    setMarks((prev) =>
      prev.includes(m) ? prev.filter((x) => x !== m) : [...prev, m],
    );

  const openMenu = (viaKeys: boolean) => {
    setMenuInstant(viaKeys);
    setMenuActive(
      Math.max(
        0,
        BLOCKS.findIndex((b) => b.id === block),
      ),
    );
    setMenu(true);
  };

  const current = BLOCKS.find((b) => b.id === block)!;

  return (
    <div
      className={cx(
        canvas,
        text1,
        "relative flex h-full min-h-[680px] w-full flex-col overflow-hidden",
      )}
    >
      <div className={cx(topBar, "flex shrink-0 items-center gap-1 px-2 py-1")}>
        <div className="relative shrink-0" ref={menuRef}>
          <button
            ref={triggerRef}
            type="button"
            aria-haspopup="menu"
            aria-expanded={menu}
            onClick={(e) => (menu ? setMenu(false) : openMenu(e.detail === 0))}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                openMenu(true);
              }
            }}
            className={cx(secondaryButton, buttonSm, "min-w-[132px]")}
          >
            <span className="flex-1 text-left">{current.label}</span>
            <HugeiconsIcon
              icon={ArrowDown01Icon}
              size={12}
              strokeWidth={1.8}
              aria-hidden="true"
              className={cx(text3, "shrink-0", menu && "rotate-180")}
            />
          </button>
          <AnimatePresence>
            {menu && (
              <motion.div
                role="menu"
                aria-label="Block type"
                initial={reduce || menuInstant ? false : { opacity: 0 }}
                animate={{
                  opacity: 1,
                  transition: { duration: 0.16, ease: [0.2, 0, 0, 1] },
                }}
                exit={
                  reduce
                    ? undefined
                    : {
                        opacity: 0,
                        transition: { duration: 0.12, ease: [0.2, 0, 0, 1] },
                      }
                }
                onKeyDown={(e) => {
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    setMenuActive((i) => (i + 1) % BLOCKS.length);
                  } else if (e.key === "ArrowUp") {
                    e.preventDefault();
                    setMenuActive(
                      (i) => (i - 1 + BLOCKS.length) % BLOCKS.length,
                    );
                  } else if (e.key === "Home") {
                    e.preventDefault();
                    setMenuActive(0);
                  } else if (e.key === "End") {
                    e.preventDefault();
                    setMenuActive(BLOCKS.length - 1);
                  } else if (e.key === "Tab") {
                    setMenu(false);
                  }
                }}
                className={cx(
                  menuPopup,
                  "absolute top-[calc(100%+6px)] left-0 z-30 w-[180px] min-w-[180px]",
                )}
              >
                {BLOCKS.map((b, i) => {
                  const on = b.id === block;
                  return (
                    <button
                      key={b.id}
                      ref={(el) => {
                        itemRefs.current[i] = el;
                      }}
                      role="menuitemradio"
                      aria-checked={on}
                      type="button"
                      tabIndex={i === menuActive ? 0 : -1}
                      onFocus={() => setMenuActive(i)}
                      onClick={() => {
                        setBlock(b.id);
                        setMenu(false);
                        triggerRef.current?.focus({ preventScroll: true });
                      }}
                      className={menuItem}
                    >
                      {b.icon ? (
                        <HugeiconsIcon
                          icon={b.icon}
                          size={16}
                          strokeWidth={1.8}
                          aria-hidden="true"
                        />
                      ) : (
                        <span className="size-4 shrink-0" />
                      )}
                      <span className="flex-1">{b.label}</span>
                      {on && (
                        <HugeiconsIcon
                          icon={Tick02Icon}
                          size={16}
                          strokeWidth={1.8}
                          aria-hidden="true"
                        />
                      )}
                    </button>
                  );
                })}
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        <span
          aria-hidden="true"
          className={cx(ruleEnd, "ml-1 h-5 w-px shrink-0")}
        />

        <div className="relative min-w-0 flex-1">
          <div
            ref={toolsRef}
            onScroll={onToolsScroll}
            className="flex items-center gap-1 overflow-x-auto p-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
          >
            {MARKS.map((m) => {
              const on = marks.includes(m.id);
              return (
                <button
                  key={m.id}
                  type="button"
                  aria-pressed={on}
                  title={`${m.label} (${m.keys})`}
                  onClick={() => toggleMark(m.id)}
                  className={cx(ghostButton, iconButtonSm)}
                >
                  <HugeiconsIcon
                    icon={m.icon}
                    size={16}
                    strokeWidth={1.8}
                    aria-hidden="true"
                  />
                  <span className="sr-only">{m.label}</span>
                </button>
              );
            })}

            <span
              aria-hidden="true"
              className={cx(ruleEnd, "mx-1 h-5 w-px shrink-0")}
            />

            {HISTORY.map((a) => (
              <button
                key={a.label}
                type="button"
                aria-label={a.label}
                className={cx(ghostButton, iconButtonSm)}
              >
                <HugeiconsIcon
                  icon={a.icon}
                  size={16}
                  strokeWidth={1.8}
                  aria-hidden="true"
                />
              </button>
            ))}
          </div>
          <div
            aria-hidden="true"
            className={cx(
              fadeStartOnTray,
              "pointer-events-none absolute inset-y-0 left-0 w-8",
              toolsEdges.start ? "opacity-100" : "opacity-0",
            )}
          />
          <div
            aria-hidden="true"
            className={cx(
              fadeEndOnTray,
              "pointer-events-none absolute inset-y-0 right-0 w-8",
              toolsEdges.end ? "opacity-100" : "opacity-0",
            )}
          />
        </div>

        <span className={cx(barText, "shrink-0 pr-3")}>
          {saved === "saved" ? "Saved" : "Saving…"}
        </span>
      </div>

      <div className="relative min-h-0 flex-1">
        <div
          ref={docRef}
          onScroll={onDocScroll}
          className="h-full overflow-y-auto"
        >
          <div className="mx-auto w-full max-w-[720px] px-6 py-8 sm:px-8">
            <input
              aria-label="Document title"
              defaultValue="Audit ledger design notes"
              onChange={() => setSaved("saving")}
              className={cx(docTitle, focusRing, cornerSm, "w-full")}
              placeholder="Untitled"
            />
            <p className={cx(docMeta, "mt-1.5 flex flex-wrap gap-x-3 gap-y-1")}>
              <span>Draft by Ana Reyes</span>
              <span>Last edited 4 minutes ago</span>
            </p>

            <textarea
              ref={areaRef}
              aria-label="Document body"
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setSaved("saving");
              }}
              spellCheck={false}
              className={cx(
                docBody,
                focusRing,
                cornerSm,
                "mt-6 min-h-[320px] w-full resize-none field-sizing-content",
              )}
              placeholder="Start writing…"
            />
          </div>
        </div>
        <div
          aria-hidden="true"
          className={cx(
            fadeTop,
            "pointer-events-none absolute inset-x-0 top-0 h-8",
            docEdges.start ? "opacity-100" : "opacity-0",
          )}
        />
        <div
          aria-hidden="true"
          className={cx(
            fadeBottom,
            "pointer-events-none absolute inset-x-0 bottom-0 h-8",
            docEdges.end ? "opacity-100" : "opacity-0",
          )}
        />
      </div>

      <footer
        className={cx(
          ruleTop,
          barText,
          "flex shrink-0 items-center gap-4 px-5 py-2.5",
        )}
      >
        <span className="tabular-nums">{stats.words} words</span>
        <span className="tabular-nums">{stats.chars} characters</span>
        <span className="tabular-nums">{stats.read} min read</span>
        <span className="ml-auto hidden sm:inline">
          Markdown shortcuts are on
        </span>
      </footer>
    </div>
  );
}
