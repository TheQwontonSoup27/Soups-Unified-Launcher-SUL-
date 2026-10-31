// What the Security & Trust page says. It is a report, not a control: every line here is a statement that was
// checked, with how it was checked, or a capability with its label. Nothing on the page can change a setting.
//
// Wording rule: say what was looked at and what was not. "Design rule" is not "audited", and a label is never
// stronger than the evidence (the page headline is the weakest label on it, as everywhere else in the app).

import { weakestStatus } from "./capabilities";
import type { Status } from "./types";

/** The day the statements below were checked against the code. Change it only when they are checked again. */
export const CHECKED_ON = "2026-10-31";

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
    "the Discord switch and the Discord small-image name. No credential is among them. Every place SUL's Rust code creates, moves or deletes a file was listed and " +
    "given a reason, and a test fails if a new one appears, if a field or key named like a secret is added, or if a credential-store library is added. " +
    "Logs hide any value kept under a name such as password, token, secret, cookie or webhook, and any Discord webhook address inside text (unit tests). " +
    "Not checked: what Steam, Discord or Windows keep on their own, and what a future change might add that the lists above do not describe.",
};

export const DATA_SHARING: TrustRule = {
  id: "data-sharing",
  title: "Data sharing",
  statement:
    "By default SUL sends nothing off this PC. It has no telemetry and no upload code. Two things could send data; they are listed below.",
  evidence:
    `On ${CHECKED_ON} SUL's own Rust code was searched: it uses no network API, and a test fails if one is added. For the Windows build, the list of libraries ` +
    "that go into the app was read from the build tool and contains no HTTP, socket, TLS or WebSocket library and no networking feature (a check that runs on every change). " +
    "The window may only load the app's own files, and it has no file, shell or network permission. " +
    "Not checked: network traffic was never captured; what Microsoft's WebView2 (the browser engine inside the window) does on its own was not audited; " +
    "and the inside of the Tauri, tokio and Windows libraries was searched for network calls as text but not read line by line.",
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
      "and the webhook address will be kept in Windows Credential Manager, not in a settings file (decision 0004). " +
      "Only the part that checks an address exists: it accepts a Discord webhook address and nothing else, and never prints the secret part of it.",
    limitation:
      "Posting needs a network library, which SUL does not have. Adding one changes the statements under Data sharing, and what the post contains is not designed yet.",
  },
];

/** Shown above the three protections: they exist as tested code, and the app does not use them. */
export const PROTECTIONS_NOTE =
  "These three are built as tested code inside SUL's core library. Nothing in the app calls them yet, so using SUL today does not scan, isolate or check anything. " +
  "Each one splits what it finds into confirmed problems and suspicious signals, and a result with no findings is never a verdict on the package, file or build.";

export const PROTECTIONS: TrustRow[] = [
  {
    id: "package-scanning",
    title: "Package scanning",
    status: "experimental",
    text:
      "In the code, not in the app. It reads only the names and sizes of the files in a mod folder or .zip file, never what is inside them, and never unpacks or opens anything. " +
      "Confirmed problems are facts about the package's structure: a path that climbs out of its folder, a name Windows treats specially, a duplicate or a link. " +
      "Suspicious signals need a person: program, script or shortcut files, a file with two extensions, an archive inside the archive, an unusually large or tightly packed file.",
    limitation:
      "Tested on made-up folders and .zip files, never on real mods. It cannot detect malware, and legitimate mods often contain program files. A .zip that uses ZIP64, several parts or two end records is refused, not scanned.",
  },
  {
    id: "quarantine",
    title: "Quarantine",
    status: "experimental",
    text:
      "In the code, not in the app. It moves one file you give it into a folder of its own: it copies the file, checks the copy matches, writes down where it came from, and only then removes the original. " +
      "It can put the file back, and never overwrites anything when it does. A file inside an original game install is refused, in both directions. It has no purge or empty operation.",
    limitation:
      "Tested on temporary folders with a simulated crash at each step; a power cut on a real disk was not tested, and the Windows link checks run only in Windows CI. It moves a file you hand it: it does not stop a program from running or find anything by itself.",
  },
  {
    id: "integrity-checks",
    title: "Integrity checks",
    status: "experimental",
    text:
      "In the code, not in the app. It compares a build folder with its saved list of file fingerprints and changes nothing. In a build that should stay as recorded, a changed or missing file is a confirmed integrity violation. " +
      "In an experimental or custom build the same difference is reported as a change, not a violation. An unlisted program file or a name that differs only by letter case is a suspicious signal, and a suspicious signal is never reported as a confirmed violation.",
    limitation:
      "Tested on made-up builds, never on a real game. It compares against a saved list; it does not know what the real game files should be, and it is not cheat or malware detection.",
  },
];

export const RULES: readonly TrustRule[] = [CREDENTIALS, DATA_SHARING];
export const ALL_ROWS: readonly TrustRow[] = [...SENDERS, ...PROTECTIONS];

/** The label for the whole page: the weakest label on it, so the headline can never say more than a row does. */
export function headlineStatus(rows: readonly TrustRow[] = ALL_ROWS): Status {
  return weakestStatus(rows.map((r) => r.status));
}
