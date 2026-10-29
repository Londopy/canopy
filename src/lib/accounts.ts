// account actions shared by the Projects view, the sidebar and the palette

import { ask } from "@tauri-apps/plugin-dialog";
import { api, errText } from "./api";
import { app } from "./state.svelte";
import type { Account } from "./types";

export function initials(label: string): string {
  const words = label.split(/[\s.\-_:]+/).filter(Boolean);
  const two = words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2);
  return two.toUpperCase();
}

export function serverLabel(server: string): string {
  return server.replace(/^https:\/\//, "");
}

export async function removeAccount(account: Account) {
  const yes = await ask(
    `Remove "${account.label}"? Canopy signs it out and deletes its saved browser data. Your projects on ${serverLabel(account.server)} aren't touched.`,
    { title: "Remove account", kind: "warning", okLabel: "Remove", cancelLabel: "Cancel" },
  );
  if (!yes) return;
  try {
    app.config = await api.removeAccount(account.id);
    delete app.pages[account.id];
    if (app.activeAccount === account.id) {
      app.activeAccount = null;
      if (app.view === "web") app.view = "projects";
    }
    app.toast("ok", `Removed ${account.label}.`);
  } catch (e) {
    app.toast("error", `Couldn't remove the account: ${errText(e)}`);
  }
}
