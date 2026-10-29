// settings export/import as .londopy files

import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText } from "./api";
import { app } from "./state.svelte";

const FILTER = [{ name: "Canopy file", extensions: ["londopy"] }];

export async function exportSettings() {
  const path = await save({ defaultPath: "Canopy settings.londopy", filters: FILTER });
  if (!path) return;
  try {
    await api.exportSettings(path);
    app.toast("ok", "Settings exported. Accounts stay on this computer.");
  } catch (e) {
    app.toast("error", `Export failed: ${errText(e)}`);
  }
}

export async function importSettings() {
  const path = await open({ multiple: false, directory: false, filters: FILTER });
  if (typeof path === "string") await applySettingsFile(path);
}

// shared by the Import command and by a double-clicked settings file
export async function applySettingsFile(path: string) {
  try {
    const cfg = await api.readSettingsFile(path);
    const yes = await ask("Replace your current settings with the ones in this file? Your accounts are kept.", {
      title: "Import settings",
      kind: "warning",
      okLabel: "Import",
      cancelLabel: "Cancel",
    });
    if (!yes) return;
    await api.setConfig(cfg);
    app.config = cfg;
    app.toast("ok", "Settings imported.");
  } catch (e) {
    app.toast("error", `Import failed: ${errText(e)}`);
  }
}

// a .londopy handed to Canopy from outside (double-click, second launch)
export async function openLondopy(path: string) {
  try {
    const routed = await api.routeLondopy(path);
    if (routed.result === "forwarded") {
      app.toast("info", `Opened in ${routed.app}.`);
    } else if (routed.result === "not-installed") {
      const go =
        routed.homepage &&
        (await ask(`This file belongs to ${routed.app}, which isn't installed.`, {
          title: "Not a Canopy file",
          okLabel: "Open download page",
          cancelLabel: "Cancel",
        }));
      if (go) await openUrl(routed.homepage);
    } else if (routed.kind === "settings") {
      await applySettingsFile(routed.path);
    } else {
      app.toast("error", `This Canopy file holds "${routed.kind}", which this version can't open yet.`);
    }
  } catch (e) {
    app.toast("error", errText(e));
  }
}
