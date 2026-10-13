/** Full-width loading indicator for pages awaiting their first API response. */
export default function Loading({ label = 'Loading…' }: { label?: string }) {
  return (
    <div
      className="flex items-center justify-center gap-3 rounded-xl border border-neutral-800 bg-neutral-900 p-10 text-sm text-neutral-400"
      role="status"
      aria-live="polite"
    >
      <span
        className="h-4 w-4 animate-spin rounded-full border-2 border-neutral-600 border-t-brand-500"
        aria-hidden="true"
      />
      {label}
    </div>
  );
}
