import type { ReactNode } from "react";
import type { ModuleDef } from "../types";
import { PageHeader, StatusBadge } from "../components";
import { AppearancePage } from "./appearance";

/** The 10 primary modules from PROJECT_SPEC.md section 3. Each is self-contained and registers here. */
export const MODULES: ModuleDef[] = [
  { id: "library", title: "Game Library & Builds", glyph: "▦", status: "not_investigated", specRef: "Module 1", summary: "Organize games, installations and isolated build variants; launch and roll back. Original installs are never modified." },
  { id: "mods", title: "Mods & Mod Builder", glyph: "⚙", status: "not_investigated", specRef: "Module 2", summary: "Profiles, ownership-aware deploy/purge with dry-run, conflict analysis, and reproducible experimental builds." },
  { id: "security", title: "Security & Trust", glyph: "⬡", status: "not_investigated", specRef: "Module 3", summary: "Package scanning, quarantine, credential safety, and evidence-based integrity checks that distinguish confirmed from suspicious." },
  { id: "diagnostics", title: "Diagnostics & Performance", glyph: "⌁", status: "not_investigated", specRef: "Module 4", summary: "Readable logs, compatibility testing, and measured clean-vs-modded comparisons with recorded conditions." },
  { id: "social", title: "Social & Networking", glyph: "♧", status: "not_investigated", specRef: "Module 5", summary: "Independent accounts, invitations and supported networking, plus Discord Rich Presence where APIs allow." },
  { id: "capture", title: "Capture & Clips", glyph: "◉", status: "not_investigated", specRef: "Module 6", summary: "Medal.tv integration through supported interfaces; SUL stores links and permitted metadata only." },
  { id: "remote", title: "Remote Play", glyph: "▱", status: "not_investigated", specRef: "Module 7", summary: "Host-controlled streaming and controller handoff with an immediate override. Scoped input only." },
  { id: "news", title: "News & Updates", glyph: "≋", status: "not_investigated", specRef: "Module 8", summary: "Official news and version monitoring; community sources labelled. Never auto-updates a preserved build." },
  { id: "emulation", title: "Emulation", glyph: "▧", status: "not_investigated", specRef: "Module 9", summary: "Integrate existing emulator engines individually; netplay only via documented engine features." },
  { id: "appearance", title: "Appearance", glyph: "◐", status: "partial", specRef: "Module 10", summary: "Themes, accessible contrast, and (later) branding. Theme presets are implemented; custom palettes and rebranding are not." },
];

export function renderModule(id: string): ReactNode {
  if (id === "appearance") return <AppearancePage />;
  const m = MODULES.find((x) => x.id === id);
  if (!m) return null;
  return (
    <>
      <PageHeader title={m.title} subtitle={`${m.specRef} — placeholder`} />
      <section className="card">
        <StatusBadge status={m.status} />
        <p>{m.summary}</p>
        <p className="muted">Nothing in this module is implemented yet. See ROADMAP.md for when it is scheduled.</p>
      </section>
    </>
  );
}
