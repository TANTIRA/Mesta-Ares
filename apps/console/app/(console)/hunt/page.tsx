"use client";

import { Suspense, useCallback, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import AiChat1 from "@/components/blocks/ai-chat-1";
import PromptInput1 from "@/components/blocks/prompt-input-1";
import { api } from "@/lib/api";

function HuntInner() {
  const params = useSearchParams();
  const [answer, setAnswer] = useState<string | null>(null);
  const [initial, setInitial] = useState<string | null>(null);

  const hunt = useCallback((q: string) => api.hunt(q), []);

  useEffect(() => {
    const q = params.get("q");
    if (q && initial === null) setInitial(q);
  }, [params, initial]);

  return (
    <div className="flex h-full min-h-0 gap-2 p-2">
      <div className="min-w-0 flex-1">
        <AiChat1
          key={initial ?? "hunt"}
          onSend={hunt}
          title="Threat hunt"
          agent="hunt-agent"
          initialPrompt={initial ?? undefined}
        />
      </div>
      <aside className="hidden w-[400px] shrink-0 flex-col gap-2 xl:flex">
        <div className="min-h-0 flex-1 rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 dark:border-neutral-100/14">
          <PromptInput1
            placeholder="Describe the hunt hypothesis…"
            onSend={(q) => {
              hunt(q).then(setAnswer).catch(() => setAnswer("Request failed."));
            }}
          />
        </div>
        {answer && (
          <div className="max-h-40 shrink-0 overflow-y-auto rounded-[var(--rb-r-lg,10px)] border border-neutral-900/10 bg-white p-3 text-[13px] text-neutral-900/80 dark:border-neutral-100/14 dark:bg-neutral-900 dark:text-neutral-100/80">
            {answer}
          </div>
        )}
      </aside>
    </div>
  );
}

export default function HuntPage() {
  return (
    <Suspense>
      <HuntInner />
    </Suspense>
  );
}
