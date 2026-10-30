import { useState } from "react";
import { PageHeader } from "../components/PageHeader";
import { AppearancePanel } from "../modules/appearance";
import { DiscordPanel } from "./DiscordPanel";
import { SecurityPanel } from "./SecurityPanel";

type Pane = "appearance" | "discord" | "security";

const PANES: Array<{ id: Pane; label: string }> = [
  { id: "appearance", label: "Appearance" },
  { id: "discord", label: "Discord" },
  { id: "security", label: "Security & Trust" },
];

export default function SettingsView() {
  const [pane, setPane] = useState<Pane>("appearance");
  return (
    <>
      <PageHeader title="Settings" subtitle="Appearance, Discord presence, and Security & Trust live here." />
      <div className="seg" role="tablist" aria-label="Settings sections">
        {PANES.map((p) => (
          <button key={p.id} type="button" role="tab" aria-selected={pane === p.id} className={pane === p.id ? "selected" : ""} onClick={() => setPane(p.id)}>
            {p.label}
          </button>
        ))}
      </div>
      <div className="pane" role="tabpanel">
        {pane === "appearance" && <AppearancePanel />}
        {pane === "discord" && <DiscordPanel />}
        {pane === "security" && <SecurityPanel />}
      </div>
    </>
  );
}
