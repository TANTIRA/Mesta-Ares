export function EmptyState({
  title,
  hint,
}: {
  title: string;
  hint?: string;
}) {
  return (
    <div className="flex h-full min-h-[240px] w-full flex-col items-center justify-center gap-1 p-6 text-center">
      <p className="text-[14px] font-medium text-neutral-900 dark:text-neutral-100">
        {title}
      </p>
      <p className="max-w-[48ch] text-[13px] text-neutral-900/66 dark:text-neutral-100/66">
        {hint ?? "The api-server did not return data for this view. Check that it is running and the endpoint exists."}
      </p>
    </div>
  );
}
