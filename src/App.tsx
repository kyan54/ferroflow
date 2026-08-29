import { useEffect, useState } from "react";
import { Nav } from "./components/Nav";
import type { View } from "./components/Nav";
import { ToastStack } from "./components/ToastStack";
import { DashboardView } from "./views/DashboardView";
import { ServersView } from "./views/ServersView";
import { SettingsView } from "./views/SettingsView";
import { useAppStore } from "./store";

function App() {
  const [view, setView] = useState<View>("dashboard");
  const refreshConfig = useAppStore((s) => s.refreshConfig);

  useEffect(() => {
    refreshConfig();
  }, [refreshConfig]);

  return (
    <div className="min-h-screen bg-slate-100 dark:bg-slate-900">
      <Nav active={view} onChange={setView} />
      <main className="bg-white dark:bg-slate-800/40">
        {view === "dashboard" && <DashboardView />}
        {view === "servers" && <ServersView />}
        {view === "settings" && <SettingsView />}
      </main>
      <ToastStack />
    </div>
  );
}

export default App;
