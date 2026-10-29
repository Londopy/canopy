// typed wrappers over the tauri commands in src-tauri/src/lib.rs

import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, AppInfo, Routed, TexReport, UpdateInfo } from "./types";

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
};

// tauri commands reject with a plain string; anything else gets stringified
export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
