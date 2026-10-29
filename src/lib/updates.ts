// update checks: 15 s after launch, then every 6 h, while auto-update is on.
// a manual check always answers, even "up to date" and errors.

import { api, errText } from "./api";
import { app } from "./state.svelte";

const FIRST_CHECK_MS = 15_000;
const EVERY_MS = 6 * 60 * 60 * 1000;

let checking = false;

export async function checkForUpdates(manual: boolean) {
  if (checking) return;
  checking = true;
  try {
    const found = await api.checkUpdate();
    const skipped = app.config?.updater.skipped_version;
    if (found && (manual || found.version !== skipped)) {
      app.update = found;
    } else if (manual) {
      app.toast("ok", `Canopy v${app.info?.version} is up to date.`);
    }
  } catch (e) {
    // automatic checks stay quiet offline; a manual one says what went wrong
    if (manual) app.toast("error", `Update check failed: ${errText(e)}`);
  } finally {
    checking = false;
  }
}

export function startUpdateSchedule(): () => void {
  const tick = () => {
    if (app.config?.updater.auto_update) void checkForUpdates(false);
  };
  const first = setTimeout(tick, FIRST_CHECK_MS);
  const every = setInterval(tick, EVERY_MS);
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
}

export async function skipUpdate() {
  const v = app.update?.version;
  app.update = null;
  if (v) await app.save((c) => (c.updater.skipped_version = v));
}

export async function installUpdate() {
  try {
    app.toast("info", "Downloading the update…", 60_000);
    await api.installUpdate();
  } catch (e) {
    app.toast("error", `Update failed: ${errText(e)}`);
  }
}
