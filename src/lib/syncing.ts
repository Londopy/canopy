// sync actions shared by the Projects, Sync and Snapshots views

import { ask, open } from "@tauri-apps/plugin-dialog";
import { api, errText } from "./api";
import { app } from "./state.svelte";
import type { Account, LinkStatus, ProjectSummary, SyncLink } from "./types";

export async function linkProject(account: Account, project: ProjectSummary) {
  const parent = await open({
    directory: true,
    multiple: false,
    title: `Where should "${project.name}" go? Canopy makes a folder for it inside the one you pick.`,
  });
  if (typeof parent !== "string") return;
  try {
    app.config = await api.linkProject(account.id, project._id, project.name, parent);
    app.toast("info", `Syncing ${project.name}…`);
  } catch (e) {
    app.toast("error", errText(e));
  }
}

export async function syncNow(link: SyncLink, allowMassDelete = false) {
  try {
    const report = await api.syncNow(link.id, allowMassDelete);
    if (report.conflicts.length) {
      app.toast("error", `${link.project_name}: ${report.conflicts.length} conflict(s) to resolve.`);
    }
  } catch (e) {
    // the status line shows the details; this is just the nudge
    app.toast("error", `${link.project_name}: ${errText(e)}`);
  }
}

export async function syncAnyway(link: SyncLink) {
  const status = app.sync[link.id];
  const [remote, local] = status?.massDelete ?? [0, 0];
  const yes = await ask(
    `This deletes ${remote} file(s) on Overleaf and ${local} in your folder. Snapshots keep a copy, so you can restore them. Go ahead?`,
    { title: `Sync ${link.project_name}`, kind: "warning", okLabel: "Sync anyway", cancelLabel: "Cancel" },
  );
  if (yes) await syncNow(link, true);
}

export async function unlink(link: SyncLink) {
  const yes = await ask(
    `Stop syncing "${link.project_name}"? The folder and the project on Overleaf both stay as they are.`,
    { title: "Stop syncing", kind: "warning", okLabel: "Stop syncing", cancelLabel: "Cancel" },
  );
  if (!yes) return;
  try {
    app.config = await api.unlinkProject(link.id, false);
    delete app.sync[link.id];
    if (app.historyLink === link.id) app.historyLink = null;
  } catch (e) {
    app.toast("error", errText(e));
  }
}

export function stateLabel(s: LinkStatus | undefined): { text: string; cls: string } {
  switch (s?.state) {
    case "syncing":
      return { text: "Syncing…", cls: "busy" };
    case "synced":
      return { text: "Up to date", cls: "ok" };
    case "conflict":
      return { text: "Conflicts", cls: "warn" };
    case "error":
      return { text: "Didn't sync", cls: "bad" };
    default:
      return { text: "Waiting", cls: "" };
  }
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

export function ago(seconds: number | null | undefined): string {
  if (!seconds) return "never";
  const diff = seconds - Date.now() / 1000;
  const abs = Math.abs(diff);
  if (abs < 45) return "just now";
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  return rtf.format(Math.round(diff / 86400), "day");
}

export function when(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
