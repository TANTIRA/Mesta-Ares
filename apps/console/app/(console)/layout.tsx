import AppShell1 from "@/components/blocks/app-shell-1";

export default function ConsoleLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // App UI blocks require a height-bounded parent: h-dvh here gives every
  // block below a definite height to fill.
  return (
    <div className="h-dvh w-full overflow-hidden">
      <AppShell1>{children}</AppShell1>
    </div>
  );
}
