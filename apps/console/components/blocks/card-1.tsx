"use client";

import { useState } from "react";

const cx = (...c: (string | false | null | undefined)[]) =>
  c.filter(Boolean).join(" ");

const canvas = "bg-white dark:bg-neutral-950";
const text1 = "text-neutral-900 dark:text-neutral-100";
const text2 = "text-neutral-900/80 dark:text-neutral-100/80";
const text3 = "text-neutral-900/66 dark:text-neutral-100/66";
const typeLabel = "text-[14px]/[16px] font-[450]";
const typeBodySm = "text-[13px]/[18px] font-normal";
const typeCaption = "text-[12px]/[16px] font-normal tabular-nums";
const primaryButton =
  "relative inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none transition-[color,background-color,border-color,box-shadow,outline-color,scale,filter] duration-[120ms] ease-[cubic-bezier(0.2,0,0,1)] active:scale-[0.98] active:duration-[60ms] motion-reduce:active:scale-100 disabled:cursor-not-allowed aria-busy:cursor-progress focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--rb-accent,oklch(20.5%_0_0))] dark:focus-visible:outline-[var(--rb-accent,oklch(100%_0_0))] bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_94%,black))] text-[var(--rb-accent-fg,oklch(100%_0_0))] shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.22),0_2px_5px_-2px_rgb(0_0_0/0.3)] hover:not-disabled:brightness-110 active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(20.5%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] disabled:opacity-40 disabled:shadow-none dark:bg-[linear-gradient(180deg,color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_84%,white),color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_94%,black))] dark:text-[var(--rb-accent-fg,oklch(20.5%_0_0))] dark:shadow-[inset_0_2px_0_-1px_rgb(255_255_255/0.2),inset_0_-2px_0_-1px_rgb(0_0_0/0.3),inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),0_1px_1px_rgb(0_0_0/0.66),0_2px_5px_-2px_rgb(0_0_0/0.9)] dark:active:shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--rb-accent,oklch(100%_0_0))_88%,black),inset_0_1px_2px_rgb(0_0_0/0.42)] dark:disabled:shadow-none";
const buttonMd =
  "h-8 gap-2 rounded-[var(--rb-r-sm,6px)] px-4 text-[14px]/[16px] font-[450]";
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

export type Member = {
  id: string;
  name: string;
  initials: string;
  role: string;
  location: string;
  online: boolean;
  bio: string;
};

const MEMBERS: Member[] = [
  {
    id: "rivera",
    name: "Sam Rivera",
    initials: "SR",
    role: "Product lead",
    location: "Lisbon",
    online: true,
    bio: "Runs discovery and roadmap for the payments surface.",
  },
  {
    id: "hale",
    name: "Imani Hale",
    initials: "IH",
    role: "Staff engineer",
    location: "Berlin",
    online: true,
    bio: "Owns the ledger service and the move to event sourcing.",
  },
  {
    id: "okafor",
    name: "Dana Okafor",
    initials: "DO",
    role: "Design director",
    location: "New York",
    online: false,
    bio: "Sets the visual system and reviews every shipped surface.",
  },
  {
    id: "moss",
    name: "Rowan Moss",
    initials: "RM",
    role: "Data scientist",
    location: "Toronto",
    online: false,
    bio: "Builds the forecasting models behind the revenue dashboard.",
  },
  {
    id: "silva",
    name: "Nina Silva",
    initials: "NS",
    role: "Support manager",
    location: "São Paulo",
    online: true,
    bio: "Leads the escalation desk across three regions.",
  },
  {
    id: "abara",
    name: "Tomas Abara",
    initials: "TA",
    role: "Solutions architect",
    location: "Singapore",
    online: false,
    bio: "Pairs with enterprise accounts through their first integration.",
  },
];

export default function Card1({
  members = MEMBERS,
  title,
}: {
  members?: Member[];
  title?: string;
}) {
  const [connected, setConnected] = useState<Set<string>>(
    () => new Set(["hale"]),
  );

  const toggle = (id: string) =>
    setConnected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div
      className={cx(
        canvas,
        text1,
        "h-full min-h-[560px] w-full overflow-y-auto p-6 sm:p-8",
      )}
    >
      <ul className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {members.map((member) => {
          const isConnected = connected.has(member.id);

          return (
            <li key={member.id} className={panel}>
              <div
                className={cx(
                  panelBody,
                  "mt-1 flex-1 items-center px-4 py-6 text-center",
                )}
              >
                <span
                  role="img"
                  aria-label={`${member.name}, ${member.online ? "online" : "offline"}`}
                  className={cx(avatar, "size-14")}
                >
                  <span aria-hidden="true" className={avatarFallback}>
                    {member.initials}
                  </span>
                </span>

                <p className={cx(typeLabel, text1, "mt-3 w-full truncate")}>
                  {member.name}
                </p>
                <p
                  className={cx(
                    typeCaption,
                    text3,
                    "mt-0.5 flex w-full min-w-0 justify-center gap-2",
                  )}
                >
                  <span className="truncate">{member.role}</span>
                  <span className="shrink-0">{member.location}</span>
                </p>

                <p className={cx(typeBodySm, text2, "mt-3")}>{member.bio}</p>
              </div>

              <button
                type="button"
                aria-pressed={isConnected}
                onClick={() => toggle(member.id)}
                className={cx(
                  isConnected ? chip : primaryButton,
                  buttonMd,
                  "m-1 justify-center",
                )}
              >
                {isConnected ? "Connected" : "Connect"}
                <span className="sr-only"> with {member.name}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
