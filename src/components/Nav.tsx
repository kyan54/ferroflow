export type View = "dashboard" | "servers" | "settings";

const TABS: { id: View; label: string }[] = [
  { id: "dashboard", label: "Dashboard" },
  { id: "servers", label: "Servers" },
  { id: "settings", label: "Settings" },
];

export function Nav({ active, onChange }: { active: View; onChange: (view: View) => void }) {
  return (
    <nav className="flex gap-1 border-b border-slate-300 px-4 pt-3 dark:border-slate-700">
      {TABS.map((tab) => (
        <button
          key={tab.id}
          onClick={() => onChange(tab.id)}
          className={`rounded-t-lg px-4 py-2 text-sm font-medium transition-colors ${
            active === tab.id
              ? "bg-white text-slate-900 dark:bg-slate-800 dark:text-white"
              : "text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
          }`}
        >
          {tab.label}
        </button>
      ))}
    </nav>
  );
}
