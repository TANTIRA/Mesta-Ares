"use client";

import { useState } from "react";
import SettingsForm1, {
  type Section,
} from "@/components/blocks/settings-form-1";
import Integrations1, {
  type App,
} from "@/components/blocks/integrations-1";
import AppDialog1 from "@/components/blocks/app-dialog-1";
import { api } from "@/lib/api";
import { useApi } from "@/lib/use-api";

const SECTIONS: Section[] = [
  {
    id: "detection",
    label: "Detection",
    title: "Detection",
    blurb: "Which sources feed the triage pipeline and how loudly they fire.",
    fields: [
      {
        kind: "checkgroup",
        id: "det-sources",
        label: "Telemetry sources",
        hint: "Alerts from disabled sources are dropped at ingestion.",
        items: [
          { id: "edr", label: "EDR" },
          { id: "cloud", label: "Cloud audit" },
          { id: "net", label: "Network" },
          { id: "email", label: "Email gateway" },
        ],
      },
      {
        kind: "select",
        id: "det-threshold",
        label: "Triage threshold",
        hint: "Minimum score that promotes an alert into the queue.",
        options: [
          { value: "low", label: "Low (25+)" },
          { value: "medium", label: "Medium (50+)" },
          { value: "high", label: "High (75+)" },
        ],
      },
      {
        kind: "text",
        id: "det-ruleset",
        label: "Detection ruleset",
        hint: "Rules bundle pinned for this workspace.",
      },
    ],
  },
  {
    id: "response",
    label: "Response",
    title: "Response",
    blurb: "How far agents may act before a human signs off.",
    fields: [
      {
        kind: "select",
        id: "resp-autonomy",
        label: "Agent autonomy",
        hint: "Autonomous still respects the kill switch.",
        options: [
          { value: "supervised", label: "Supervised — approve every step" },
          { value: "assisted", label: "Assisted — approve risky steps" },
          { value: "autonomous", label: "Autonomous — log everything" },
        ],
      },
      {
        kind: "switch",
        id: "resp-require-approval",
        label: "Require approval for containment",
        hint: "Hosts and accounts stay untouched until approved.",
      },
      {
        kind: "textarea",
        id: "resp-notes",
        label: "Standing notes",
        hint: "Context agents should read before acting.",
      },
    ],
  },
  {
    id: "retention",
    label: "Retention",
    title: "Retention",
    blurb: "How long alerts, audit events and plans are kept.",
    fields: [
      {
        kind: "select",
        id: "ret-days",
        label: "Audit retention",
        hint: "Audit events older than this are archived.",
        options: [
          { value: "30", label: "30 days" },
          { value: "90", label: "90 days" },
          { value: "365", label: "1 year" },
        ],
      },
      {
        kind: "switch",
        id: "ret-archive",
        label: "Cold archive",
        hint: "Move expired data to object storage instead of deleting.",
      },
    ],
  },
];

const VALUES = {
  edr: true,
  cloud: true,
  net: true,
  email: false,
  "det-threshold": "medium",
  "det-ruleset": "mesta-core/2025-10",
  "resp-autonomy": "assisted",
  "resp-require-approval": true,
  "resp-notes": "",
  "ret-days": "90",
  "ret-archive": true,
};

const APPS: App[] = [
  {
    name: "Splunk",
    mark: "SP",
    group: "SIEM",
    blurb: "Forward alerts and detection output to Splunk indexes.",
    connected: true,
  },
  {
    name: "CrowdStrike",
    mark: "CS",
    group: "EDR",
    blurb: "Pull detections; isolate hosts during containment.",
    connected: true,
  },
  {
    name: "VirusTotal",
    mark: "VT",
    group: "Intel",
    blurb: "Enrich hashes, domains and URLs in the triage pass.",
    connected: true,
  },
  {
    name: "Slack",
    mark: "SL",
    group: "Notify",
    blurb: "Post triage verdicts and approval requests to channels.",
  },
  {
    name: "PagerDuty",
    mark: "PD",
    group: "Notify",
    blurb: "Page the on-call responder on critical alerts.",
  },
  {
    name: "MISP",
    mark: "MI",
    group: "Intel",
    blurb: "Sync threat intel feeds into the enrichment pipeline.",
  },
  {
    name: "TheHive",
    mark: "TH",
    group: "Case",
    blurb: "Mirror cases and tasks to a TheHive instance.",
  },
  {
    name: "Defender",
    mark: "DF",
    group: "EDR",
    blurb: "Ingest Microsoft Defender alerts and run response actions.",
  },
];

export default function SettingsPage() {
  const { data: ks } = useApi(() => api.killSwitch());
  const [engaged, setEngaged] = useState<boolean | null>(null);
  const state = engaged ?? ks?.engaged ?? false;

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-2 p-2">
        <div
          className="shrink-0 rounded-[var(--rb-r-lg,10px)] border-2 p-5"
          style={{
            borderColor: state
              ? "oklch(55% 0.2 27)"
              : "oklch(55% 0.2 27 / 0.5)",
            background: state
              ? "oklch(55% 0.2 27 / 0.08)"
              : undefined,
          }}
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 className="text-[16px]/[20px] font-[450] text-[oklch(51%_0.18_26)] dark:text-[oklch(76%_0.14_26)]">
                Kill switch
              </h2>
              <p className="mt-0.5 max-w-prose text-[13px]/[18px] text-neutral-900/66 dark:text-neutral-100/66">
                {state
                  ? "ENGAGED — all agent activity is halted. Disengage to resume."
                  : "Halts every running agent and blocks new runs until disengaged."}
              </p>
            </div>
            <div className="w-[200px] shrink-0">
              <AppDialog1
                destructive={!state}
                triggerLabel={state ? "Disengage kill switch" : "Engage kill switch"}
                title={state ? "Disengage kill switch" : "Engage kill switch"}
                description={
                  state
                    ? "Agents resume normal operation and queued runs are picked up again."
                    : "All running agents stop immediately and no new workflow runs start while the switch is engaged."
                }
                changes={[
                  {
                    label: "Agent runs",
                    from: state ? "halted" : "running",
                    to: state ? "running" : "halted",
                  },
                  {
                    label: "Approvals",
                    from: state ? "paused" : "active",
                    to: state ? "active" : "paused",
                  },
                ]}
                confirmLabel={state ? "Disengage" : "Engage"}
                toastTitle={state ? "Kill switch disengaged" : "Kill switch engaged"}
                toastDescription={
                  state
                    ? "Agents are resuming normal operation."
                    : "All agent activity is halted until disengaged."
                }
                onConfirm={() => {
                  const next = !state;
                  setEngaged(next);
                  api.setKillSwitch(next).catch(() => setEngaged(!next));
                }}
              />
            </div>
          </div>
        </div>

        <div className="h-[680px] shrink-0">
          <SettingsForm1 sections={SECTIONS} savedValues={VALUES} />
        </div>

        <div className="h-[620px] shrink-0">
          <Integrations1 apps={APPS} />
        </div>
      </div>
    </div>
  );
}
