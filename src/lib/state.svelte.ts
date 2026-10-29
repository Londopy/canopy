// app-wide state, shared by every component through runes

import { api, errText } from "./api";
import type { AppConfig, AppInfo, Theme, UpdateInfo, View } from "./types";

export interface Toast {
  id: number;
  kind: "info" | "ok" | "error";
  text: string;
}

class AppState {
  config = $state<AppConfig | null>(null);
  info = $state<AppInfo | null>(null);
  view = $state<View>("projects");
  paletteOpen = $state(false);
  update = $state<UpdateInfo | null>(null);
  toasts = $state<Toast[]>([]);
  // what the OS says, for theme = "system"
  systemDark = $state(false);

  private nextToast = 1;

  get dark(): boolean {
    const t = this.config?.general.theme ?? "system";
    return t === "dark" || (t === "system" && this.systemDark);
  }

  // web mode only: no local editor, watcher or language server
  get lite(): boolean {
    return !this.info?.full_build || !!this.config?.general.lite_mode;
  }

  async load() {
    [this.info, this.config] = await Promise.all([api.appInfo(), api.getConfig()]);
  }

  // applies a change to the config and saves it; rolls back if saving fails
  async save(change: (c: AppConfig) => void) {
    if (!this.config) return;
    const before = $state.snapshot(this.config);
    change(this.config);
    try {
      await api.setConfig($state.snapshot(this.config));
    } catch (e) {
      this.config = before;
      this.toast("error", `Couldn't save settings: ${errText(e)}`);
    }
  }

  setTheme(theme: Theme) {
    return this.save((c) => (c.general.theme = theme));
  }

  toast(kind: Toast["kind"], text: string, ms = 4500) {
    const id = this.nextToast++;
    this.toasts.push({ id, kind, text });
    setTimeout(() => this.dismiss(id), ms);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const app = new AppState();
