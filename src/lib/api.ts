// typed wrappers over the tauri commands in src-tauri/src/lib.rs

import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, AppInfo, Rect, Routed, TexReport, UpdateInfo, WebAction } from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getConfig: () => invoke<AppConfig>("get_config"),
  setConfig: (cfg: AppConfig) => invoke<void>("set_config", { cfg }),
  detectTex: () => invoke<TexReport>("detect_tex"),
  exportSettings: (path: string) => invoke<void>("export_settings", { path }),
  readSettingsFile: (path: string) => invoke<AppConfig>("read_settings_file", { path }),
  openDataDir: () => invoke<void>("open_data_dir"),
  routeLondopy: (path: string) => invoke<Routed>("route_londopy", { path }),
  takeStartupFile: () => invoke<string | null>("take_startup_file"),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
  addAccount: (label: string, server: string) => invoke<AppConfig>("add_account", { label, server }),
  renameAccount: (id: string, label: string) => invoke<AppConfig>("rename_account", { id, label }),
  removeAccount: (id: string) => invoke<AppConfig>("remove_account", { id }),
  webShow: (id: string, rect: Rect) => invoke<void>("web_show", { id, rect }),
  webBounds: (rect: Rect) => invoke<void>("web_bounds", { rect }),
  webHide: () => invoke<void>("web_hide"),
  webAction: (id: string, action: WebAction) => invoke<void>("web_action", { id, action }),
};

// tauri commands reject with a plain string; anything else gets stringified
export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
