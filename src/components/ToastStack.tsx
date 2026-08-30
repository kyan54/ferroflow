import { useAppStore } from "../store";
import { useTranslation } from "../i18n";

const KIND_STYLES: Record<string, string> = {
  info: "bg-surface-3 text-fg border border-line",
  error: "bg-err text-white",
  success: "bg-ok text-white",
};

export function ToastStack() {
  const { t } = useTranslation();
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
            aria-label={t.common.dismiss}
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}
