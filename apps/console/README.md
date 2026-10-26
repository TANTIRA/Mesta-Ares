# Mesta-Ares SOC Console

The SOC console frontend for Mesta-Ares, an agentic SIEM+SOAR. It is a
standalone Next.js application inside the Rust monorepo and talks to the
`api-server` crate over HTTP and SSE.

## Stack

- Next.js 15 (App Router) + React 19 + TypeScript
- Tailwind CSS v4 (`@tailwindcss/postcss`), dark mode by default via
  `next-themes`
- shadcn-style component registry configuration (`components.json`)
- React Bits Pro Ultimate App UI blocks (`components/blocks/`)
- Zod-validated API layer (`lib/api.ts`)

## Setup

```bash
cd apps/console
npm install
npm run dev
```

### Environment variables

Create `apps/console/.env.local` (already ignored by `.gitignore`):

| Variable                 | Required | Default                 | Purpose                                     |
| ------------------------ | -------- | ----------------------- | ------------------------------------------- |
| `NEXT_PUBLIC_API_URL`    | No       | `http://localhost:8080` | Base URL of the Rust `api-server`.          |
| `REACTBITS_LICENSE_KEY`  | Yes*     | —                       | React Bits Pro license; used only by the shadcn CLI when (re)installing blocks. |

\* Required only to reinstall the React Bits blocks. The committed source
runs without it.

## React Bits Pro blocks

All blocks live in `components/blocks/` and keep their `"use client"`
directives — do not remove them; the blocks rely on Motion and client-side
state.

To reinstall or add blocks, export the license key and run the shadcn CLI
against the registries configured in `components.json`:

```bash
export REACTBITS_LICENSE_KEY=...   # same key as the repo root .env.local

npx shadcn@latest add @reactbits-pro/app-shell-1
npx shadcn@latest add @reactbits-pro/data-table-1 @reactbits-pro/filtering-1
npx shadcn@latest add @reactbits-pro/kanban-1
npx shadcn@latest add @reactbits-pro/agent-activity-1 @reactbits-pro/agent-plan-1 @reactbits-pro/tool-calls-1
npx shadcn@latest add @reactbits-pro/dashboard-1 @reactbits-pro/analytics-1 @reactbits-pro/monitoring-1
npx shadcn@latest add @reactbits-pro/ai-chat-1 @reactbits-pro/prompt-input-1
npx shadcn@latest add @reactbits-pro/settings-form-1 @reactbits-pro/integrations-1 @reactbits-pro/notifications-1
npx shadcn@latest add @reactbits-pro/agent-approval-1
npx shadcn@latest add @reactbits-pro/card-1 @reactbits-pro/list-1 @reactbits-pro/app-dialog-1
```

Installed blocks: `agent-activity-1`, `agent-approval-1`, `agent-plan-1`,
`ai-chat-1`, `analytics-1`, `app-dialog-1`, `app-shell-1`, `card-1`,
`dashboard-1`, `data-table-1`, `filtering-1`, `integrations-1`, `kanban-1`,
`list-1`, `monitoring-1`, `notifications-1`, `prompt-input-1`,
`settings-form-1`, `tool-calls-1` (19 blocks, all from the Pro registry —
no local fallbacks).

The blocks ship with demo data; in this app they have been retargeted to
accept backend-derived props. If you reinstall a block, reapply the prop
interfaces used by the screens (check the page that renders it).

## Layout constraint: bounded height

Every screen must render inside a height-bounded parent. The App UI blocks
use internal scroll areas and `h-full`/`min-h-0` flex layouts that collapse
without a bounded ancestor.

`app/(console)/layout.tsx` provides the bound:

```tsx
<div className="h-dvh w-full overflow-hidden">{children}</div>
```

Each page then uses `h-full` (plus `overflow-y-auto` for scrollable
screens) and gives each block a fixed or flexible height slot, for example
`h-[560px]` or `min-h-0 flex-1`. When adding a screen, keep this chain:
`h-dvh` layout → `h-full` page → fixed/flex slot per block.

## Design language

Dense monochrome SOC styling at ~13px, dark theme by default
(`defaultTheme="dark"` in `app/layout.tsx`). Shared radius/spacing tokens
are defined in `app/globals.css` (`--rb-*` variables consumed by the
blocks).

## Screens and endpoints

All data flows through `lib/api.ts`, which validates responses with Zod
schemas and normalizes list endpoints that return either a bare array or
`{ items: [...] }`. Every screen renders an empty state
(`components/empty-state.tsx`) when its endpoint is unreachable.

| Route            | Blocks used                                            | Backend endpoints                                   |
| ---------------- | ------------------------------------------------------ | --------------------------------------------------- |
| `/`              | `dashboard-1`, `monitoring-1`, `analytics-1`           | `GET /metrics`, `GET /alerts?limit=20`              |
| `/alerts`        | `data-table-1`, `filtering-1`                          | `GET /alerts?limit&status`                          |
| `/alerts/[id]`   | `card-1`, `list-1`                                     | `GET /alerts/:id`                                   |
| `/cases`         | `kanban-1`                                             | `GET /cases`                                        |
| `/cases/[id]`    | `agent-plan-1`, `tool-calls-1`, `card-1`               | `GET /cases/:id`, `GET /plans/:case_id`, `GET /audit` |
| `/agents`        | `agent-activity-1`                                     | `GET /stream/activity` (SSE), `GET /audit` (backlog) |
| `/audit`         | `tool-calls-1`                                         | `GET /audit`                                        |
| `/hunt`          | `ai-chat-1`, `prompt-input-1`                          | `POST /hunt` (stub response rendered on 404/absence) |
| `/coverage`      | `monitoring-1`                                         | `GET /coverage` (ATT&CK technique chips)             |
| `/playbooks`     | `@xyflow/react` + dagre (custom nodes)                 | `GET /playbooks`, `GET /playbooks/:name`             |
| `/settings`      | `settings-form-1`, `integrations-1`, `app-dialog-1`, `card-1` | `GET /controls/kill-switch`, `POST /controls/kill-switch` |
| `/notifications` | `notifications-1`                                      | `GET /alerts?limit=30` (derived feed)                |

### Kill switch

`/settings` renders a red kill-switch card. It reads state with
`GET /controls/kill-switch` and toggles with
`POST /controls/kill-switch`. Every toggle requires confirmation through
`app-dialog-1` before the POST fires.

### Live activity

`/agents` subscribes to `GET /stream/activity` via `EventSource` using the
`sse()` helper in `lib/api.ts`, which validates each frame against
`ActivityEventSchema` and skips malformed events. The page also loads
`GET /audit` once as a backlog. The connection closes on unmount.

## Scripts

```bash
npm run dev        # next dev
npm run build      # next build
npm run lint       # eslint .
npm run typecheck  # tsc --noEmit
```

## Notes

- The license key must never be committed; `.env.local` is git-ignored.
- `outputFileTracingRoot` is set in `next.config.ts` because the app lives
  inside a monorepo with multiple lockfiles.
- `npm audit` reported 3 vulnerabilities in transitive deps at install
  time; not yet remediated.
