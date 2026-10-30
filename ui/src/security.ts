// What the Security & Trust page says. It is a report, not a control: every line here is a statement that was
// checked, with how it was checked, or a capability with its label. Nothing on the page can change a setting.
//
// Wording rule: say what was looked at and what was not. "Design rule" is not "audited", and a label is never
// stronger than the evidence (the page headline is the weakest label on it, as everywhere else in the app).

import { weakestStatus } from "./capabilities";
import type { Status } from "./types";

/** The day the statements below were checked against the code. Change it only when they are checked again. */
export const CHECKED_ON = "2026-10-30";

/** A rule SUL follows by design. It is not a capability, so it carries no capability label, and it says so. */
export interface TrustRule {
  id: string;
  title: string;
  statement: string;
  /** What was looked at, and what was not. */
  evidence: string;
}

/** A capability, or something that exists or does not, with its label. */
export interface TrustRow {
  id: string;
  title: string;
  status: Status;
  text: string;
  limitation?: string;
}

export const RULE_TAG = "Design rule, not an audit";

export const CREDENTIALS: TrustRule = {
  id: "credentials",
  title: "Credentials",
  statement:
    "SUL does not store passwords, tokens or API keys. To start a Steam game it asks the Steam app to open a steam://run address; it never handles your Steam login.",
  evidence:
    `Rule from decision 0003. On ${CHECKED_ON} the settings the window keeps were listed: theme and colours, background (and the picture you choose), display density, ` +
    "the Discord switch and the Discord small-image name. No credential is among them. Logs hide any value kept under a name such as " +
    "password, token, secret, cookie or webhook, and any Discord webhook address inside text (unit tests). The rest of what SUL writes to disk was not re-audited.",
};

export const DATA_SHARING: TrustRule = {
  id: "data-sharing",
  title: "Data sharing",
  statement:
    "By default SUL sends nothing off this PC. It has no telemetry and no upload code. Two things could send data; they are listed below.",
  evidence:
    `On ${CHECKED_ON} SUL's own Rust code was searched: it uses no network API and declares no HTTP client. The window may only load the app's own files, ` +
    "and it has no file, shell or network permission. A test fails if network code is added to SUL's own source. Not checked: network traffic was never " +
    "captured, and the Tauri framework SUL is built on includes an HTTP client library (reqwest) whose use was not audited.",
};

/** The two things that could send data off this PC. */
export const SENDERS: TrustRow[] = [
  {
    id: "rich-presence",
    title: "Discord Rich Presence",
    status: "experimental",
    text:
      "Off until you switch it on (Settings > Discord). When on, SUL tells the Discord app on this PC that you are “Browsing the launcher” and, " +
      "if you set one, which small image to show. The Discord app then shows that on your Discord profile.",
    limitation: "Never run against a live Discord client.",
  },
  {
    id: "webhooks",
    title: "Automated Social/Discord Webhooks",
    status: "not_investigated",
    text:
      "Not built, so nothing can be posted. If it is built it will be off until you switch it on and give it a destination, " +
      "and the webhook address will be kept in Windows Credential Manager, not in a settings file (decision 0004).",
  },
];

export const PROTECTIONS: TrustRow[] = [
  { id: "package-scanning", title: "Package scanning", status: "not_investigated", text: "Not built. Nothing scans mod packages yet." },
  { id: "quarantine", title: "Quarantine", status: "not_investigated", text: "Not built. Nothing is isolated or removed." },
  {
    id: "integrity-checks",
    title: "Integrity checks",
    status: "not_investigated",
    text:
      "Not built. When it exists, a confirmed integrity violation and a suspicious signal will be shown apart, and a suspicious signal will never be reported as a confirmed violation.",
  },
];

export const RULES: readonly TrustRule[] = [CREDENTIALS, DATA_SHARING];
export const ALL_ROWS: readonly TrustRow[] = [...SENDERS, ...PROTECTIONS];

/** The label for the whole page: the weakest label on it, so the headline can never say more than a row does. */
export function headlineStatus(rows: readonly TrustRow[] = ALL_ROWS): Status {
  return weakestStatus(rows.map((r) => r.status));
}
