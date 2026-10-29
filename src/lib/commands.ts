// everything the command palette can run. views, the tray and keyboard
// shortcuts all go through here so there is one list to keep honest.

import { api, errText } from "./api";
import { exportSettings, importSettings } from "./settingsio";
import { app } from "./state.svelte";
import type { View } from "./types";
import { checkForUpdates } from "./updates";

export interface Command {
  id: string;
  title: string;
  group: string;
  keys?: string;
  run: () => void | Promise<void>;
}

const go = (view: View) => () => {
  app.view = view;
};

export const VIEWS: { view: View; title: string; icon: string; keys: string; full?: boolean }[] = [
  { view: "projects", title: "Projects", icon: "folder", keys: "Ctrl+1" },
  { view: "search", title: "Search", icon: "search", keys: "Ctrl+2", full: true },
  { view: "sync", title: "Sync", icon: "sync", keys: "Ctrl+3", full: true },
  { view: "history", title: "Snapshots", icon: "history", keys: "Ctrl+4", full: true },
  { view: "dashboard", title: "Writing dashboard", icon: "chart", keys: "Ctrl+5" },
];

export function commands(): Command[] {
  const views: Command[] = VIEWS.filter((v) => !v.full || !app.lite).map((v) => ({
    id: `view.${v.view}`,
    title: `Go to ${v.title}`,
    group: "View",
    keys: v.keys,
    run: go(v.view),
  }));
  const accounts: Command[] = app.accounts.map((a) => ({
    id: `account.open.${a.id}`,
    title: `Open ${a.label}`,
    group: "Overleaf",
    run: () => app.openAccount(a.id),
  }));
  // page commands only make sense while a page is showing
  const page = app.view === "web" ? app.account : null;
  const pageCommands: Command[] = page
    ? (
        [
          ["reload", "Reload the page", "Ctrl+Shift+R"],
          ["home", "Go to all projects", undefined],
          ["back", "Go back", undefined],
          ["open-in-browser", "Open this page in your browser", undefined],
        ] as const
      ).map(([action, title, keys]) => ({
        id: `page.${action}`,
        title,
        group: page.label,
        keys,
        run: () => api.webAction(page.id, action).catch((e) => app.toast("error", errText(e))),
      }))
    : [];
  return [
    ...pageCommands,
    ...accounts,
    {
      id: "account.add",
      title: "Add an Overleaf account…",
      group: "Overleaf",
      run: () => {
        app.accountDialog = { mode: "add" };
      },
    },
    ...views,
    { id: "view.settings", title: "Open Settings", group: "View", keys: "Ctrl+,", run: go("settings") },
    {
      id: "theme.toggle",
      title: app.dark ? "Switch to light theme" : "Switch to dark theme",
      group: "Appearance",
      keys: "Ctrl+Shift+L",
      run: () => app.setTheme(app.dark ? "light" : "dark"),
    },
    { id: "theme.system", title: "Follow the system theme", group: "Appearance", run: () => app.setTheme("system") },
    {
      id: "lite.toggle",
      title: app.config?.general.lite_mode ? "Turn off Lite mode" : "Turn on Lite mode",
      group: "Canopy",
      run: () => app.save((c) => (c.general.lite_mode = !c.general.lite_mode)),
    },
    { id: "update.check", title: "Check for updates", group: "Canopy", run: () => checkForUpdates(true) },
    { id: "settings.export", title: "Export settings…", group: "Canopy", run: exportSettings },
    { id: "settings.import", title: "Import settings…", group: "Canopy", run: importSettings },
    {
      id: "data.open",
      title: "Open the data folder",
      group: "Canopy",
      run: () => api.openDataDir().catch((e) => app.toast("error", errText(e))),
    },
  ];
}

// subsequence match ("gtset" finds "Go to Settings"), scored so tight and
// early matches sort first. -1 means no match
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase().replace(/\s+/g, "");
  const t = text.toLowerCase();
  if (!q) return 0;
  let score = 0;
  let ti = 0;
  let last = -1;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return -1;
    // reward adjacent letters and word starts, punish gaps
    if (found === last + 1) score += 3;
    if (found === 0 || t[found - 1] === " ") score += 2;
    score -= Math.min(found - ti, 5) * 0.2;
    last = found;
    ti = found + 1;
  }
  return score;
}
