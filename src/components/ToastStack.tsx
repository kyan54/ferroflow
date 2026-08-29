import { useAppStore } from "../store";

const KIND_STYLES: Record<string, string> = {
  info: "bg-slate-800 text-slate-100",
  error: "bg-red-600 text-white",
  success: "bg-emerald-600 text-white",
};

export function ToastStack() {
  const toasts = useAppStore((s) => s.toasts);
  const dismissToast = useAppStore((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`flex items-start justify-between gap-3 rounded-lg px-4 py-3 text-sm shadow-lg ${
            KIND_STYLES[toast.kind]
          }`}
        >
          <span className="break-words">{toast.message}</span>
          <button
            onClick={() => dismissToast(toast.id)}
            className="shrink-0 opacity-70 hover:opacity-100"
            aria-label="Dismiss"
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}
