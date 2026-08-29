import { useEffect, useState } from "react";
import { Nav } from "./components/Nav";
import type { View } from "./components/Nav";
import { ToastStack } from "./components/ToastStack";
import { DashboardView } from "./views/DashboardView";
import { ServersView } from "./views/ServersView";
import { RulesView } from "./views/RulesView";
import { RuleResourcesView } from "./views/RuleResourcesView";
import { ConnectionsView } from "./views/ConnectionsView";
import { SettingsView } from "./views/SettingsView";
import { useAppStore } from "./store";

function App() {
  const [view, setView] = useState<View>("dashboard");
  const refreshConfig = useAppStore((s) => s.refreshConfig);

  useEffect(() => {
    refreshConfig();
  }, [refreshConfig]);

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background text-fg">
      <Nav active={view} onChange={setView} />
      <main className="min-w-0 flex-1 overflow-y-auto">
        {view === "dashboard" && <DashboardView />}
        {view === "servers" && <ServersView />}
        {view === "rules" && <RulesView />}
        {view === "ruleResources" && <RuleResourcesView />}
        {view === "connections" && <ConnectionsView />}
        {view === "settings" && <SettingsView />}
      </main>
      <ToastStack />
    </div>
  );
}

export default App;
