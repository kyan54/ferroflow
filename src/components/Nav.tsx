import type { SVGProps } from "react";

export type View = "dashboard" | "servers" | "rules" | "connections" | "settings";

function icon(props: SVGProps<SVGSVGElement>) {
  return {
    viewBox: "0 0 24 24",
    fill: "none" as const,
    stroke: "currentColor",
    strokeWidth: 1.9,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    ...props,
  };
}

function DashboardIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M3 11l9-8 9 8M5 10v10h14V10" />
    </svg>
  );
}

function ServersIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <rect x="3" y="4" width="18" height="4" rx="1" />
      <rect x="3" y="10" width="18" height="4" rx="1" />
      <rect x="3" y="16" width="18" height="4" rx="1" />
    </svg>
  );
}

function RulesIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M4 6h16M7 12h10M10 18h4" />
    </svg>
  );
}

function ConnectionsIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <path d="M3 12h4l3 8 4-16 3 8h4" />
    </svg>
  );
}

function SettingsIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...icon(props)}>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2" />
    </svg>
  );
}

const TABS: { id: View; label: string; icon: typeof DashboardIcon }[] = [
  { id: "dashboard", label: "Dashboard", icon: DashboardIcon },
  { id: "servers", label: "Servers", icon: ServersIcon },
  { id: "rules", label: "Rules", icon: RulesIcon },
  { id: "connections", label: "Connections", icon: ConnectionsIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

export function Nav({ active, onChange }: { active: View; onChange: (view: View) => void }) {
  return (
    <nav className="flex h-full w-[196px] shrink-0 flex-col border-r border-line bg-surface">
      <div className="flex items-center gap-2 px-5 pt-5 pb-4">
        <span className="flex h-7 w-7 items-center justify-center rounded-md bg-flow text-white">
          <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M4 6h16M4 12h10M4 18h6" strokeLinecap="round" />
          </svg>
        </span>
        <span className="font-display text-[15px] font-semibold tracking-tight text-fg">
          Ferroflow
        </span>
      </div>

      <ul className="flex flex-1 flex-col gap-0.5 px-2">
        {TABS.map((tab) => {
          const Icon = tab.icon;
          const isActive = active === tab.id;
          return (
            <li key={tab.id}>
              <button
                onClick={() => onChange(tab.id)}
                aria-current={isActive ? "page" : undefined}
                className={`relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                  isActive
                    ? "bg-flow-weak text-flow-hi"
                    : "text-fg-dim hover:bg-surface-2 hover:text-fg"
                }`}
              >
                {isActive && (
                  <span className="absolute left-0 top-1/2 h-4 w-[3px] -translate-y-1/2 rounded-full bg-flow" />
                )}
                <Icon className="h-[17px] w-[17px] shrink-0" strokeWidth={isActive ? 2.1 : 1.8} />
                {tab.label}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
