"use client";

import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useReducedMotion } from "motion/react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Loading03Icon,
  Tick02Icon,
  UnfoldMoreIcon,
} from "@hugeicons/core-free-icons";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const rowsRuled = "divide-y divide-neutral-900/8 dark:divide-neutral-100/8";
const typeHeadingXs = "text-[16px]/[20px] font-[450] tracking-[-0.015em]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const primaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_94%,black))] text-[var(--rb-accent-fg,oklch(100%_0_0))] shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.22),0_2px_5px_-2px_rgb(0_0_0/0.3)] hover:not-disabled:brightness-110 active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] disabled:opacity-40 disabled:shadow-none dark:bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_94%,black))] dark:text-[var(--rb-accent-fg,oklch(20.5%_0_0))] dark:shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.66),0_2px_5px_-2px_rgb(0_0_0/0.9)] dark:active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] dark:disabled:shadow-none";
const secondaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] border border-neutral-900/12 bg-white text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] hover:not-disabled:border-neutral-900/32 hover:not-disabled:bg-linear-to-b hover:not-disabled:from-neutral-900/3 hover:not-disabled:to-neutral-900/3 active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.12)] disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:border-neutral-100/32 dark:hover:not-disabled:from-neutral-100/3 dark:hover:not-disabled:to-neutral-100/3 dark:active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.4)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const buttonSm =
  "h-7 gap-1.5 rounded-[var(--rb-r-sm,6px)] px-3 text-[14px]/[16px] font-[450]";
const pendingRing =
  "animate-spin [animation-duration:800ms] motion-reduce:animate-none";
const pendingStack = "grid place-items-center *:[grid-area:1/1]";
const inputBox =
  "relative flex w-full min-w-0 cursor-text items-center rounded-[var(--rb-r-sm,6px)] data-concentric:rounded-[var(--rb-r-lg,10px)] border border-neutral-900/12 bg-white text-[14px]/[16px] font-[450] text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] transition-[border-color,box-shadow,background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-focus-within:not-has-[[aria-invalid=true]]:not-has-[input:disabled]:border-neutral-900/32 focus-within:border-[var(--rb-accent,oklch(20.5%_0_0))] focus-within:ring-3 focus-within:ring-neutral-900/8 has-[[aria-invalid=true]]:border-[oklch(60%_0.236_26)] has-[input:disabled]:cursor-not-allowed has-[input:disabled]:border-neutral-900/8 has-[input:disabled]:text-neutral-900/40 has-[input:disabled]:shadow-none has-[input[readonly]]:cursor-default has-[input[readonly]]:border-transparent has-[input[readonly]]:bg-neutral-900/5 has-[input[readonly]]:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-focus-within:not-has-[[aria-invalid=true]]:not-has-[input:disabled]:border-neutral-100/32 dark:focus-within:border-[var(--rb-accent,oklch(100%_0_0))] dark:focus-within:ring-neutral-100/8 dark:has-[[aria-invalid=true]]:border-[oklch(65.2%_0.219_26)] dark:has-[input:disabled]:border-neutral-100/8 dark:has-[input:disabled]:text-neutral-100/40 dark:has-[input[readonly]]:bg-neutral-100/5";
const fieldMd = "h-8";
const inputControl =
  "h-full w-full min-w-0 flex-1 bg-transparent px-3 outline-none [[data-affix]+&]:ps-2 [&:has(+[data-affix])]:pe-2 placeholder:text-neutral-900/66 disabled:cursor-not-allowed disabled:placeholder:text-neutral-900/40 dark:placeholder:text-neutral-100/66 dark:disabled:placeholder:text-neutral-100/40 dark:[color-scheme:dark]";
const inputAffix =
  "flex shrink-0 items-center gap-2 text-[14px]/[16px] text-neutral-900/66 dark:text-neutral-100/66 select-none first:ps-3 last:pe-3 has-[button]:first:ps-1 has-[button]:last:pe-1";
const textareaClass =
  "block max-h-[256px] min-h-[76px] w-full min-w-48 resize-none rounded-[var(--rb-r-sm,6px)] border border-neutral-900/12 bg-white px-3 py-2 text-[14px]/[20px] font-[450] text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] outline-none field-sizing-content transition-[border-color,box-shadow,background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] placeholder:text-neutral-900/66 hover:not-focus-visible:not-aria-invalid:not-disabled:not-read-only:border-neutral-900/32 focus-visible:border-[var(--rb-accent,oklch(20.5%_0_0))] focus-visible:ring-3 focus-visible:ring-neutral-900/8 aria-invalid:border-[oklch(60%_0.236_26)] disabled:cursor-not-allowed disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none read-only:border-transparent read-only:bg-neutral-900/5 read-only:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:placeholder:text-neutral-100/66 dark:hover:not-focus-visible:not-aria-invalid:not-disabled:not-read-only:border-neutral-100/32 dark:focus-visible:border-[var(--rb-accent,oklch(100%_0_0))] dark:focus-visible:ring-neutral-100/8 dark:aria-invalid:border-[oklch(65.2%_0.219_26)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40 dark:read-only:bg-neutral-100/5";
const fieldLabel =
  "flex w-fit items-center gap-2 text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100";
const fieldDescription =
  "max-w-[60ch] text-[13px]/[18px] text-neutral-900/80 dark:text-neutral-100/80";
const selectTrigger =
  "inline-flex h-8 w-fit min-w-40 cursor-pointer items-center justify-between gap-2 rounded-[var(--rb-r-sm,6px)] border border-neutral-900/12 bg-white px-3 text-left text-[14px]/[16px] font-[450] whitespace-nowrap text-neutral-900 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] outline-none select-none transition-[border-color,box-shadow] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-disabled:not-aria-invalid:border-neutral-900/32 focus-visible:border-[var(--rb-accent,oklch(20.5%_0_0))] focus-visible:ring-3 focus-visible:ring-neutral-900/8 aria-expanded:border-[var(--rb-accent,oklch(20.5%_0_0))] aria-expanded:ring-3 aria-expanded:ring-neutral-900/8 aria-invalid:border-[oklch(60%_0.236_26)] disabled:cursor-not-allowed disabled:border-neutral-900/8 disabled:text-neutral-900/40 disabled:shadow-none dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-disabled:not-aria-invalid:border-neutral-100/32 dark:focus-visible:border-[var(--rb-accent,oklch(100%_0_0))] dark:focus-visible:ring-neutral-100/8 dark:aria-expanded:border-[var(--rb-accent,oklch(100%_0_0))] dark:aria-expanded:ring-neutral-100/8 dark:aria-invalid:border-[oklch(65.2%_0.219_26)] dark:disabled:border-neutral-100/8 dark:disabled:text-neutral-100/40";
const selectCaret = "shrink-0 text-neutral-900/80 dark:text-neutral-100/80";
const checkboxBox =
  "relative inline-flex size-4 shrink-0 items-center justify-center";
const checkbox =
  "peer size-4 shrink-0 cursor-pointer appearance-none rounded-[var(--rb-r-xs,4px)] border border-neutral-900/50 bg-white shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] transition-[background-color,border-color,box-shadow,outline-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:not-checked:not-disabled:border-neutral-900 active:not-checked:bg-neutral-900/8 checked:border-transparent checked:bg-[var(--rb-accent,oklch(20.5%_0_0))] indeterminate:border-transparent indeterminate:bg-[var(--rb-accent,oklch(20.5%_0_0))] aria-invalid:border-[oklch(60%_0.236_26)] disabled:cursor-not-allowed disabled:border-neutral-900/40 disabled:shadow-none disabled:checked:bg-neutral-900/40 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:border-neutral-100/50 dark:bg-neutral-900 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:not-checked:not-disabled:border-neutral-100 dark:active:not-checked:bg-neutral-100/8 dark:checked:bg-[var(--rb-accent,oklch(100%_0_0))] dark:indeterminate:bg-[var(--rb-accent,oklch(100%_0_0))] dark:aria-invalid:border-[oklch(65.2%_0.219_26)] dark:disabled:border-neutral-100/40 dark:disabled:checked:bg-neutral-100/40";
const checkboxMark =
  "pointer-events-none absolute text-[var(--rb-accent-fg,oklch(100%_0_0))] opacity-0 peer-checked:opacity-100 peer-indeterminate:opacity-100 dark:text-[var(--rb-accent-fg,oklch(20.5%_0_0))]";
const switchTrack =
  "group relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full bg-white p-0.5 inset-ring inset-ring-neutral-900/50 shadow-[0_1px_2px_-1px_rgb(0_0_0/0.12)] transition-[background-color,box-shadow,outline-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] hover:aria-[checked=false]:not-disabled:inset-ring-neutral-900 active:aria-[checked=false]:bg-neutral-900/8 aria-checked:bg-[var(--rb-accent,oklch(20.5%_0_0))] aria-checked:inset-ring-transparent disabled:cursor-not-allowed disabled:inset-ring-neutral-900/40 disabled:shadow-none disabled:aria-checked:bg-neutral-900/40 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:bg-neutral-900 dark:inset-ring-neutral-100/50 dark:shadow-[0_1px_2px_-1px_rgb(0_0_0/0.36)] dark:hover:aria-[checked=false]:not-disabled:inset-ring-neutral-100 dark:active:aria-[checked=false]:bg-neutral-100/8 dark:aria-checked:bg-[var(--rb-accent,oklch(100%_0_0))] dark:disabled:inset-ring-neutral-100/40 dark:disabled:aria-checked:bg-neutral-100/40";
const switchThumb =
  "pointer-events-none block size-4 rounded-full bg-neutral-900/80 transition-[translate,background-color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none group-aria-checked:translate-x-4 group-aria-checked:bg-[var(--rb-accent-fg,oklch(100%_0_0))] group-disabled:bg-neutral-900/40 dark:bg-neutral-100/80 dark:group-aria-checked:bg-[var(--rb-accent-fg,oklch(20.5%_0_0))] dark:group-disabled:bg-neutral-100/40";
const badge =
  "inline-flex h-5 min-w-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden whitespace-nowrap rounded-[var(--rb-r-xs,4px)] px-1.5 text-[12px]/[16px] font-[450] tabular-nums";
const badgeNeutral =
  "bg-neutral-900/9 dark:bg-neutral-100/9 text-neutral-900 dark:text-neutral-100";
const frameTitle =
  "min-w-0 truncate text-[14px]/[16px] font-[450] text-neutral-900 dark:text-neutral-100";
const frameDescription =
  "text-[13px]/[18px] text-neutral-900/80 dark:text-neutral-100/80";
const panel =
  "relative flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-neutral-50 text-neutral-900 dark:text-neutral-100 dark:bg-neutral-900";
const panelHead =
  "flex min-h-10 min-w-0 shrink-0 items-center gap-2 px-4 has-[button]:pe-2";
const panelBody =
  "mx-1 mb-1 flex min-w-0 flex-col overflow-hidden rounded-[var(--rb-r-sm,6px)] border border-neutral-900/8 bg-white [&:has(+*)]:mb-0 dark:border-neutral-100/8 dark:bg-neutral-950";
const panelFoot =
  "flex min-h-11 min-w-0 items-center gap-2 px-4 py-2 text-[13px]/[18px] text-neutral-900/66 dark:text-neutral-100/66 has-[button]:justify-end has-[button]:pe-2";
const menuPopup =
  "min-w-48 max-w-[min(480px,calc(100vw-2rem))] overflow-x-hidden overflow-y-auto overscroll-contain rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14 bg-white dark:bg-neutral-900 p-1 text-neutral-900 dark:text-neutral-100 outline-none shadow-[0_12px_24px_-12px_rgb(0_0_0/0.2)] dark:shadow-[0_12px_24px_-12px_rgb(0_0_0/0.6)]";
const menuItem =
  "relative flex min-h-8 w-full cursor-default items-center gap-2 rounded-[var(--rb-r-sm,6px)] px-2 py-1.5 text-left text-[14px]/[16px] font-[450] text-neutral-900 outline-none select-none [&_svg]:shrink-0 [&_svg]:text-neutral-900/80 hover:bg-neutral-900/5 focus-visible:bg-neutral-900/5 data-[highlighted]:bg-neutral-900/5 aria-selected:bg-neutral-900/9 aria-disabled:cursor-not-allowed aria-disabled:text-neutral-900/40 dark:text-neutral-100 dark:[&_svg]:text-neutral-100/80 dark:hover:bg-neutral-100/5 dark:focus-visible:bg-neutral-100/5 dark:data-[highlighted]:bg-neutral-100/5 dark:aria-selected:bg-neutral-100/9 dark:aria-disabled:text-neutral-100/40";
const overlayEnter =
  "transition-opacity duration-[160ms] ease-[cubic-bezier(0.2,0,0,1)] starting:opacity-0 motion-reduce:transition-none";
const sidebarSurface =
  "border-r border-neutral-900/8 bg-white dark:border-neutral-100/8 dark:bg-neutral-900";
const navItem =
  "flex h-8 w-full min-w-0 cursor-pointer items-center gap-2 rounded-[var(--rb-r-sm,6px)] px-2 text-left text-[14px]/[16px] font-[450] text-neutral-900/80 transition-[background-color,color] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] [&_svg]:shrink-0 hover:bg-neutral-900/5 hover:text-neutral-900 aria-[current=page]:bg-neutral-900/9 aria-[current=page]:text-neutral-900 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] dark:text-neutral-100/80 dark:hover:bg-neutral-100/5 dark:hover:text-neutral-100 dark:aria-[current=page]:bg-neutral-100/9 dark:aria-[current=page]:text-neutral-100";
const topBar =
  "border-b border-neutral-900/8 bg-white dark:border-neutral-100/8 dark:bg-neutral-900";
const fadeTop =
  "bg-linear-to-b from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeBottom =
  "bg-linear-to-t from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-950";
const fadeStartOnTray =
  "bg-linear-to-r from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-900";
const fadeEndOnTray =
  "bg-linear-to-l from-white to-transparent transition-opacity duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] dark:from-neutral-900";

const sidebarTitle = cx(typeHeadingXs, text1);
const sidebarMeta = cx(typeCaption, text2);
const optionText = cx(typeBodySm, text2);
const footerStatus = cx(typeBodySm, text2);

function useScrollFade<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);
  const [edges, setEdges] = useState({ start: false, end: false });
  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const { scrollTop, scrollHeight, clientHeight } = el;
    setEdges({
      start: scrollTop > 1,
      end: scrollTop + clientHeight < scrollHeight - 1,
    });
  }, []);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    update();
    const RO = el.ownerDocument.defaultView?.ResizeObserver;
    const ro = RO ? new RO(update) : null;
    ro?.observe(el);
    return () => ro?.disconnect();
  }, [update]);
  return { ref, edges, onScroll: update };
}

function useScrollFadeX<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);
  const [edges, setEdges] = useState({ start: false, end: false });
  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const { scrollLeft, scrollWidth, clientWidth } = el;
    setEdges({
      start: scrollLeft > 1,
      end: scrollLeft + clientWidth < scrollWidth - 1,
    });
  }, []);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    update();
    const RO = el.ownerDocument.defaultView?.ResizeObserver;
    const ro = RO ? new RO(update) : null;
    ro?.observe(el);
    return () => ro?.disconnect();
  }, [update]);
  return { ref, edges, onScroll: update };
}

function roomAround(node: HTMLElement) {
  const view = node.ownerDocument.defaultView;
  const box = node.getBoundingClientRect();
  let top = 0;
  let bottom = view ? view.innerHeight : box.bottom;
  for (let el = node.parentElement; el && view; el = el.parentElement) {
    if (view.getComputedStyle(el).overflowY === "visible") continue;
    const clip = el.getBoundingClientRect();
    top = Math.max(top, clip.top);
    bottom = Math.min(bottom, clip.bottom);
  }
  return { above: box.top - top, below: bottom - box.bottom };
}

type Option = { value: string; label: string };

type Field =
  | { kind: "text"; id: string; label: string; hint: string }
  | { kind: "prefix"; id: string; label: string; hint: string; prefix: string }
  | { kind: "textarea"; id: string; label: string; hint: string }
  | {
      kind: "select";
      id: string;
      label: string;
      hint: string;
      options: Option[];
    }
  | { kind: "switch"; id: string; label: string; hint: string }
  | {
      kind: "checkgroup";
      id: string;
      label: string;
      hint: string;
      items: { id: string; label: string }[];
    };

export type Section = {
  id: string;
  label: string;
  title: string;
  blurb: string;
  fields: Field[];
};

const SECTIONS: Section[] = [
  {
    id: "workspace",
    label: "Workspace",
    title: "Workspace",
    blurb: "Details, defaults, and formats people use across the workspace.",
    fields: [
      {
        kind: "text",
        id: "ws-name",
        label: "Name",
        hint: "Shown in sidebars, invitations, and reports.",
      },
      {
        kind: "prefix",
        id: "ws-slug",
        label: "Workspace URL",
        hint: "Lowercase letters, numbers, and hyphens only.",
        prefix: "northwind.app/",
      },
      {
        kind: "select",
        id: "ws-region",
        label: "Primary region",
        hint: "Where new projects store data by default.",
        options: [
          { value: "us-east", label: "US East (N. Virginia)" },
          { value: "us-west", label: "US West (Oregon)" },
          { value: "eu-west", label: "Europe (Frankfurt)" },
          { value: "ap-south", label: "Asia Pacific (Singapore)" },
        ],
      },
      {
        kind: "textarea",
        id: "ws-description",
        label: "Description",
        hint: "A short note for people joining the workspace.",
      },
      {
        kind: "select",
        id: "ws-language",
        label: "Language",
        hint: "The default language for shared workspace screens.",
        options: [
          { value: "en", label: "English" },
          { value: "de", label: "German" },
          { value: "fr", label: "French" },
          { value: "ja", label: "Japanese" },
        ],
      },
      {
        kind: "select",
        id: "ws-timezone",
        label: "Time zone",
        hint: "Used for timestamps in reports and exports.",
        options: [
          { value: "new_york", label: "New York, GMT−05:00" },
          { value: "london", label: "London, GMT+00:00" },
          { value: "berlin", label: "Berlin, GMT+01:00" },
          { value: "tokyo", label: "Tokyo, GMT+09:00" },
        ],
      },
      {
        kind: "select",
        id: "ws-date",
        label: "Date format",
        hint: "How dates appear in shared workspace screens.",
        options: [
          { value: "mdy", label: "Aug 5, 2026" },
          { value: "dmy", label: "5 Aug 2026" },
          { value: "iso", label: "2026-08-05" },
        ],
      },
    ],
  },
  {
    id: "billing",
    label: "Billing",
    title: "Billing",
    blurb: "Who is charged, how often, and what appears on the invoice.",
    fields: [
      {
        kind: "select",
        id: "bl-plan",
        label: "Plan",
        hint: "Changes apply on the next invoice.",
        options: [
          { value: "team", label: "Team" },
          { value: "business", label: "Business" },
          { value: "enterprise", label: "Enterprise" },
        ],
      },
      {
        kind: "select",
        id: "bl-cycle",
        label: "Billing cycle",
        hint: "Annual plans are invoiced once, in advance.",
        options: [
          { value: "monthly", label: "Monthly" },
          { value: "annual", label: "Annual" },
        ],
      },
      {
        kind: "text",
        id: "bl-email",
        label: "Billing email",
        hint: "Invoices and receipts are sent here.",
      },
      {
        kind: "textarea",
        id: "bl-address",
        label: "Billing address",
        hint: "Printed on every invoice.",
      },
      {
        kind: "text",
        id: "bl-tax",
        label: "Tax ID",
        hint: "Shown on invoices in regions that require it.",
      },
      {
        kind: "text",
        id: "bl-po",
        label: "Purchase order",
        hint: "Added to the invoice reference. Leave empty if unused.",
      },
      {
        kind: "switch",
        id: "bl-receipts",
        label: "Email receipts",
        hint: "Send a receipt to the billing address after each charge.",
      },
    ],
  },
  {
    id: "notifications",
    label: "Notifications",
    title: "Notifications",
    blurb: "Where workspace activity reaches people, and how often.",
    fields: [
      {
        kind: "checkgroup",
        id: "nt-email",
        label: "Email",
        hint: "Choose which messages reach the inbox.",
        items: [
          { id: "nt-email-product", label: "Product updates" },
          { id: "nt-email-security", label: "Security alerts" },
          { id: "nt-email-digest", label: "Weekly digest" },
        ],
      },
      {
        kind: "select",
        id: "nt-digest-day",
        label: "Digest day",
        hint: "When the weekly digest is sent.",
        options: [
          { value: "monday", label: "Monday" },
          { value: "friday", label: "Friday" },
          { value: "sunday", label: "Sunday" },
        ],
      },
      {
        kind: "select",
        id: "nt-quiet",
        label: "Quiet hours",
        hint: "Hold non-urgent alerts until the morning.",
        options: [
          { value: "off", label: "Off" },
          { value: "18-09", label: "18:00 to 09:00" },
          { value: "22-07", label: "22:00 to 07:00" },
        ],
      },
      {
        kind: "text",
        id: "nt-reply",
        label: "Reply-to address",
        hint: "Where replies to workspace email are delivered.",
      },
      {
        kind: "switch",
        id: "nt-desktop",
        label: "Desktop alerts",
        hint: "Show alerts while the app is open.",
      },
      {
        kind: "switch",
        id: "nt-mobile",
        label: "Mobile push",
        hint: "Send alerts to signed-in mobile devices.",
      },
      {
        kind: "switch",
        id: "nt-mentions",
        label: "Mentions",
        hint: "Notify people when someone mentions their name.",
      },
    ],
  },
  {
    id: "security",
    label: "Security",
    title: "Security",
    blurb: "How members prove who they are and how long they stay signed in.",
    fields: [
      {
        kind: "switch",
        id: "sc-2fa",
        label: "Two-factor authentication",
        hint: "Require a second factor for every member.",
      },
      {
        kind: "select",
        id: "sc-sso",
        label: "Single sign-on",
        hint: "Members sign in through your identity provider.",
        options: [
          { value: "off", label: "Off" },
          { value: "okta", label: "Okta" },
          { value: "google", label: "Google Workspace" },
        ],
      },
      {
        kind: "select",
        id: "sc-timeout",
        label: "Session timeout",
        hint: "Sign members out after this much inactivity.",
        options: [
          { value: "8h", label: "8 hours" },
          { value: "24h", label: "24 hours" },
          { value: "7d", label: "7 days" },
          { value: "30d", label: "30 days" },
        ],
      },
      {
        kind: "text",
        id: "sc-domains",
        label: "Allowed email domains",
        hint: "Only these domains can join without an invitation.",
      },
      {
        kind: "textarea",
        id: "sc-ips",
        label: "IP allowlist",
        hint: "One CIDR range per line. Empty allows every address.",
      },
      {
        kind: "select",
        id: "sc-audit",
        label: "Audit log retention",
        hint: "How long member activity is kept.",
        options: [
          { value: "30d", label: "30 days" },
          { value: "1y", label: "1 year" },
          { value: "3y", label: "3 years" },
        ],
      },
      {
        kind: "switch",
        id: "sc-devices",
        label: "Device approval",
        hint: "Ask an admin to approve every new device.",
      },
    ],
  },
  {
    id: "advanced",
    label: "Advanced",
    title: "Advanced",
    blurb: "Defaults for projects, the API, and scheduled exports.",
    fields: [
      {
        kind: "select",
        id: "ad-visibility",
        label: "New project visibility",
        hint: "Where new projects start out.",
        options: [
          { value: "workspace", label: "Everyone in the workspace" },
          { value: "private", label: "Only the creator" },
        ],
      },
      {
        kind: "select",
        id: "ad-rate",
        label: "API rate limit",
        hint: "Requests per minute across every key.",
        options: [
          { value: "1000", label: "1,000 requests" },
          { value: "5000", label: "5,000 requests" },
          { value: "10000", label: "10,000 requests" },
        ],
      },
      {
        kind: "prefix",
        id: "ad-webhook",
        label: "Webhook endpoint",
        hint: "Workspace events are posted here.",
        prefix: "https://",
      },
      {
        kind: "select",
        id: "ad-export",
        label: "Export format",
        hint: "Used by scheduled workspace exports.",
        options: [
          { value: "json", label: "JSON" },
          { value: "csv", label: "CSV" },
          { value: "parquet", label: "Parquet" },
        ],
      },
      {
        kind: "prefix",
        id: "ad-domain",
        label: "Custom domain",
        hint: "Serve shared links from your own domain.",
        prefix: "https://",
      },
      {
        kind: "switch",
        id: "ad-beta",
        label: "Beta features",
        hint: "Try unreleased features before general release.",
      },
      {
        kind: "switch",
        id: "ad-analytics",
        label: "Usage analytics",
        hint: "Share anonymous usage data to help prioritise work.",
      },
    ],
  },
];

type Values = Record<string, string | boolean>;

const SAVED_VALUES: Values = {
  "ws-name": "Northwind",
  "ws-slug": "northwind",
  "ws-region": "us-east",
  "ws-description":
    "Internal tools and dashboards for the operations team. Shared with 24 people.",
  "ws-language": "en",
  "ws-timezone": "new_york",
  "ws-date": "mdy",

  "bl-plan": "business",
  "bl-cycle": "annual",
  "bl-email": "accounts-payable@northwind.co",
  "bl-address": "18 Wharf Road\nLondon N1 7GS\nUnited Kingdom",
  "bl-tax": "GB 428 9971 07",
  "bl-po": "",
  "bl-receipts": true,

  "nt-email-product": false,
  "nt-email-security": true,
  "nt-email-digest": true,
  "nt-digest-day": "monday",
  "nt-quiet": "18-09",
  "nt-reply": "no-reply@northwind.co",
  "nt-desktop": true,
  "nt-mobile": false,
  "nt-mentions": true,

  "sc-2fa": true,
  "sc-sso": "okta",
  "sc-timeout": "24h",
  "sc-domains": "northwind.co, contractors.northwind.co",
  "sc-ips": "",
  "sc-audit": "1y",
  "sc-devices": false,

  "ad-visibility": "workspace",
  "ad-rate": "5000",
  "ad-webhook": "hooks.northwind.co/workspace",
  "ad-export": "json",
  "ad-domain": "",
  "ad-beta": false,
  "ad-analytics": true,
};

function Switch({
  id,
  checked,
  onChange,
  labelledBy,
  describedBy,
}: {
  id: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  labelledBy: string;
  describedBy: string;
}) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      onClick={() => onChange(!checked)}
      className={switchTrack}
    >
      <span className={switchThumb} />
    </button>
  );
}

function Select({
  id,
  value,
  onChange,
  options,
  describedBy,
}: {
  id: string;
  value: string;
  onChange: (v: string) => void;
  options: Option[];
  describedBy: string;
}) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [instant, setInstant] = useState(false);
  const [active, setActive] = useState(0);
  const [flip, setFlip] = useState(false);
  const [maxHeight, setMaxHeight] = useState<number | undefined>(undefined);
  const rootRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const typeahead = useRef({ buffer: "", at: 0 });

  const selectedIndex = options.findIndex((o) => o.value === value);
  const selected = selectedIndex >= 0 ? options[selectedIndex] : undefined;

  const openList = (viaKeys: boolean) => {
    const node = rootRef.current;
    if (node) {
      const { above, below } = roomAround(node);
      const listHeight = Math.min(options.length * 32 + 10, 264);
      const up = below < listHeight + 6 && above > below;
      const room = (up ? above : below) - 10;
      setFlip(up);
      setMaxHeight(room < listHeight ? Math.max(room, 72) : undefined);
    }
    setInstant(viaKeys);
    setActive(selectedIndex >= 0 ? selectedIndex : 0);
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;
    const node = rootRef.current;
    const doc = node?.ownerDocument;
    if (!node || !doc) return;
    const onPointer = (e: Event) => {
      if (!node.contains(e.target as Node)) setOpen(false);
    };
    doc.addEventListener("mousedown", onPointer);
    doc.addEventListener("touchstart", onPointer);
    return () => {
      doc.removeEventListener("mousedown", onPointer);
      doc.removeEventListener("touchstart", onPointer);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const list = listRef.current;
    const row = list?.querySelector<HTMLElement>(`[data-index="${active}"]`);
    if (!list || !row) return;
    const top = row.offsetTop;
    const bottom = top + row.offsetHeight;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (bottom > list.scrollTop + list.clientHeight)
      list.scrollTop = bottom - list.clientHeight;
  }, [open, active]);

  const commit = (index: number) => {
    const option = options[index];
    if (!option) return;
    onChange(option.value);
    setOpen(false);
    rootRef.current?.querySelector("button")?.focus({ preventScroll: true });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (!open) {
      if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(e.key)) {
        e.preventDefault();
        openList(true);
      }
      return;
    }
    if (e.key === "Escape" || e.key === "Tab") {
      if (e.key === "Escape") e.stopPropagation();
      setOpen(false);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, options.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Home") {
      e.preventDefault();
      setActive(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActive(options.length - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      commit(active);
    } else if (e.key.length === 1 && /\S/.test(e.key)) {
      const now = Date.now();
      const t = typeahead.current;
      t.buffer = now - t.at > 700 ? e.key : t.buffer + e.key;
      t.at = now;
      const hit = options.findIndex((o) =>
        o.label.toLowerCase().startsWith(t.buffer.toLowerCase()),
      );
      if (hit >= 0) setActive(hit);
    }
  };

  return (
    <div ref={rootRef} className="relative flex">
      <button
        id={id}
        type="button"
        role="combobox"
        aria-expanded={open}
        aria-controls={listId}
        aria-haspopup="listbox"
        aria-activedescendant={open ? `${listId}-${active}` : undefined}
        aria-describedby={describedBy}
        onClick={() => (open ? setOpen(false) : openList(false))}
        onKeyDown={onKeyDown}
        className={cx(selectTrigger, "flex-1")}
      >
        <span className="truncate">{selected?.label}</span>
        <HugeiconsIcon
          icon={UnfoldMoreIcon}
          size={16}
          strokeWidth={1.8}
          aria-hidden="true"
          className={selectCaret}
        />
      </button>

      {open && (
        <ul
          ref={listRef}
          id={listId}
          role="listbox"
          tabIndex={-1}
          style={maxHeight ? { maxHeight } : undefined}
          className={cx(
            menuPopup,
            !instant && overlayEnter,
            "absolute left-0 z-30 max-h-[264px] w-full min-w-[100%]",
            flip ? "bottom-full mb-1.5" : "top-full mt-1.5",
          )}
        >
          {options.map((o, i) => {
            const isSelected = o.value === value;
            return (
              <li
                key={o.value}
                id={`${listId}-${i}`}
                role="option"
                aria-selected={isSelected}
                data-highlighted={i === active ? "" : undefined}
                data-index={i}
                onMouseEnter={() => setActive(i)}
                onClick={() => commit(i)}
                className={cx(menuItem, "pe-8")}
              >
                <span className="truncate">{o.label}</span>
                {isSelected && (
                  <HugeiconsIcon
                    icon={Tick02Icon}
                    size={16}
                    strokeWidth={1.8}
                    aria-hidden="true"
                    className="absolute end-2"
                  />
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function SettingsRow({
  id,
  label,
  description,
  native,
  inline,
  changed,
  children,
}: {
  id: string;
  label: string;
  description: string;
  native: boolean;
  inline?: boolean;
  changed?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={cx(
        "px-3 py-2.5",
        inline
          ? "flex items-center justify-between gap-4"
          : "flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4",
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-h-5 items-center gap-2">
          {native ? (
            <label htmlFor={id} className={fieldLabel}>
              {label}
            </label>
          ) : (
            <span id={`${id}-label`} className={fieldLabel}>
              {label}
            </span>
          )}
          {changed ? (
            <span className={cx(badge, badgeNeutral)}>Changed</span>
          ) : null}
        </div>
        <p id={`${id}-hint`} className={cx(fieldDescription, "mt-0.5")}>
          {description}
        </p>
      </div>
      <div className={inline ? "shrink-0" : "w-full sm:w-80 sm:shrink-0"}>
        {children}
      </div>
    </div>
  );
}

export default function SettingsForm1({
  sections = SECTIONS,
  savedValues = SAVED_VALUES,
  onSave,
}: {
  sections?: Section[];
  savedValues?: Values;
  onSave?: (values: Values) => void;
}) {
  const reduce = useReducedMotion();
  const {
    ref: navRef,
    edges: navEdges,
    onScroll: onNavScroll,
  } = useScrollFadeX<HTMLElement>();
  const {
    ref: bodyRef,
    edges: bodyEdges,
    onScroll: onBodyScroll,
  } = useScrollFade<HTMLDivElement>();
  const [active, setActive] = useState(sections[0].id);
  const [values, setValues] = useState<Values>({
    ...savedValues,
    "ws-name": "Northwind Labs",
  });
  const [baseline, setBaseline] = useState<Values>(savedValues);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<string | null>(null);

  const dirtyKeys = useMemo(
    () => Object.keys(values).filter((k) => values[k] !== baseline[k]),
    [values, baseline],
  );
  const dirty = dirtyKeys.length > 0;

  const dirtySection = useMemo(
    () =>
      sections.find((s) =>
        s.fields.some((f) =>
          f.kind === "checkgroup"
            ? f.items.some((i) => dirtyKeys.includes(i.id))
            : dirtyKeys.includes(f.id),
        ),
      ),
    [dirtyKeys],
  );

  const section = sections.find((s) => s.id === active) ?? sections[0];

  const setValue = (key: string, value: string | boolean) =>
    setValues((prev) => ({ ...prev, [key]: value }));

  const save = () => {
    if (!dirty || saving) return;
    setSaving(true);
    onSave?.(values);
    window.setTimeout(
      () => {
        setBaseline(values);
        setSaving(false);
        setSavedAt("just now");
      },
      reduce ? 0 : 700,
    );
  };

  const discard = () => setValues(baseline);

  const renderNavItem = (id: string, label: string, layout: string) => {
    const selected = id === active;
    return (
      <button
        key={id}
        type="button"
        aria-current={selected ? "page" : undefined}
        onClick={() => setActive(id)}
        className={cx(navItem, layout)}
      >
        {label}
      </button>
    );
  };

  const renderField = (field: Field) => {
    const describedBy = `${field.id}-hint`;
    const changed =
      field.kind === "checkgroup"
        ? field.items.some((i) => dirtyKeys.includes(i.id))
        : dirtyKeys.includes(field.id);

    if (field.kind === "switch") {
      return (
        <SettingsRow
          key={field.id}
          id={field.id}
          label={field.label}
          description={field.hint}
          native={false}
          inline
          changed={changed}
        >
          <Switch
            id={field.id}
            checked={Boolean(values[field.id])}
            onChange={(v) => setValue(field.id, v)}
            labelledBy={`${field.id}-label`}
            describedBy={describedBy}
          />
        </SettingsRow>
      );
    }

    if (field.kind === "checkgroup") {
      return (
        <SettingsRow
          key={field.id}
          id={field.id}
          label={field.label}
          description={field.hint}
          native={false}
          changed={changed}
        >
          <div
            role="group"
            aria-labelledby={`${field.id}-label`}
            aria-describedby={describedBy}
            className="flex flex-col gap-2"
          >
            {field.items.map((item) => (
              <label
                key={item.id}
                className={cx(
                  optionText,
                  "flex cursor-pointer items-center gap-2",
                )}
              >
                <span className={checkboxBox}>
                  <input
                    type="checkbox"
                    checked={Boolean(values[item.id])}
                    onChange={(e) => setValue(item.id, e.target.checked)}
                    className={checkbox}
                  />
                  <HugeiconsIcon
                    icon={Tick02Icon}
                    size={12}
                    strokeWidth={2.5}
                    aria-hidden="true"
                    className={checkboxMark}
                  />
                </span>
                {item.label}
              </label>
            ))}
          </div>
        </SettingsRow>
      );
    }

    return (
      <SettingsRow
        key={field.id}
        id={field.id}
        label={field.label}
        description={field.hint}
        native
        changed={changed}
      >
        {field.kind === "select" ? (
          <Select
            id={field.id}
            value={String(values[field.id] ?? "")}
            onChange={(v) => setValue(field.id, v)}
            options={field.options}
            describedBy={describedBy}
          />
        ) : field.kind === "textarea" ? (
          <textarea
            id={field.id}
            rows={2}
            value={String(values[field.id] ?? "")}
            aria-describedby={describedBy}
            onChange={(e) => setValue(field.id, e.target.value)}
            className={textareaClass}
          />
        ) : field.kind === "prefix" ? (
          <div className={cx(inputBox, fieldMd)}>
            <span data-affix className={inputAffix}>
              {field.prefix}
            </span>
            <input
              id={field.id}
              value={String(values[field.id] ?? "")}
              aria-describedby={describedBy}
              onChange={(e) => setValue(field.id, e.target.value)}
              className={inputControl}
            />
          </div>
        ) : (
          <div className={cx(inputBox, fieldMd)}>
            <input
              id={field.id}
              value={String(values[field.id] ?? "")}
              aria-describedby={describedBy}
              onChange={(e) => setValue(field.id, e.target.value)}
              className={inputControl}
            />
          </div>
        )}
      </SettingsRow>
    );
  };

  return (
    <div
      className={cx(
        canvas,
        text1,
        "relative flex h-full min-h-[800px] w-full overflow-hidden",
      )}
    >
      <aside
        className={cx(
          sidebarSurface,
          "hidden w-64 shrink-0 flex-col gap-4 p-4 sm:p-5 lg:flex",
        )}
      >
        <div className="px-2">
          <h2 className={sidebarTitle}>Settings</h2>
          <p className={cx(sidebarMeta, "mt-0.5")}>Northwind Labs</p>
        </div>
        <nav aria-label="Settings sections" className="flex flex-col gap-0.5">
          {sections.map((s) => renderNavItem(s.id, s.label, "text-left"))}
        </nav>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="relative shrink-0 lg:hidden">
          <nav
            ref={navRef}
            onScroll={onNavScroll}
            aria-label="Settings sections"
            className={cx(
              topBar,
              "flex items-center gap-2 overflow-x-auto px-4 py-2",
            )}
          >
            {sections.map((s) =>
              renderNavItem(s.id, s.label, "max-w-max shrink-0"),
            )}
          </nav>
          <div
            aria-hidden="true"
            className={cx(
              fadeStartOnTray,
              "pointer-events-none absolute top-0 bottom-px left-0 w-8",
              navEdges.start ? "opacity-100" : "opacity-0",
            )}
          />
          <div
            aria-hidden="true"
            className={cx(
              fadeEndOnTray,
              "pointer-events-none absolute top-0 bottom-px right-0 w-8",
              navEdges.end ? "opacity-100" : "opacity-0",
            )}
          />
        </div>

        <div className="relative min-h-0 flex-1">
          <div
            ref={bodyRef}
            onScroll={onBodyScroll}
            className="h-full overflow-y-auto"
          >
            <div className="mx-auto max-w-2xl p-4 sm:p-6">
              <div className={panel}>
                <div className={cx(panelHead, "py-3")}>
                  <div className="min-w-0">
                    <h3 className={frameTitle}>{section.title}</h3>
                    <p className={cx(frameDescription, "mt-0.5")}>
                      {section.blurb}
                    </p>
                  </div>
                </div>

                <div className={cx(panelBody, rowsRuled)}>
                  {section.fields.map(renderField)}
                </div>

                <div
                  className={cx(
                    panelFoot,
                    "max-sm:flex-col max-sm:items-stretch max-sm:has-[button]:pe-4",
                  )}
                >
                  <p
                    aria-live="polite"
                    className={cx(
                      footerStatus,
                      "flex min-w-0 items-center gap-2 sm:me-auto",
                    )}
                  >
                    {dirty ? (
                      <span className="truncate">
                        Unsaved changes in {dirtySection?.label ?? "Workspace"}
                      </span>
                    ) : (
                      <>
                        <HugeiconsIcon
                          icon={Tick02Icon}
                          size={16}
                          strokeWidth={1.8}
                          aria-hidden="true"
                          className={cx(text3, "shrink-0")}
                        />
                        <span className="truncate">
                          {savedAt ? `Saved ${savedAt}` : "All changes saved"}
                        </span>
                      </>
                    )}
                  </p>
                  <div className="flex flex-col-reverse gap-2 sm:flex-row">
                    <button
                      type="button"
                      onClick={discard}
                      disabled={!dirty || saving}
                      className={cx(
                        secondaryButton,
                        buttonSm,
                        "w-full sm:w-auto",
                      )}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      onClick={save}
                      disabled={!dirty}
                      aria-busy={saving}
                      className={cx(
                        primaryButton,
                        buttonSm,
                        "w-full min-w-[7.5rem] sm:w-auto",
                      )}
                    >
                      <span className={pendingStack}>
                        <span
                          className={cx(
                            "inline-flex items-center gap-2",
                            saving && "invisible",
                          )}
                        >
                          Save changes
                        </span>
                        {saving && (
                          <span className="inline-flex items-center gap-2">
                            <HugeiconsIcon
                              icon={Loading03Icon}
                              size={16}
                              strokeWidth={1.8}
                              aria-hidden="true"
                              className={pendingRing}
                            />
                            Saving
                          </span>
                        )}
                      </span>
                    </button>
                  </div>
                </div>
              </div>
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
      </div>
    </div>
  );
}
