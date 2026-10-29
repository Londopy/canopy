// typed wrappers over the tauri commands in src-tauri/src/lib.rs

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  AppInfo,
  Change,
  CompileOutcome,
  FileEntry,
  LinkStatus,
  ProjectSummary,
  Rect,
  RestoreReport,
  Routed,
  Snapshot,
  SourceSpot,
  Spot,
  SyncReport,
  TexReport,
  UpdateInfo,
  WebAction,
} from "./types";

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
  // sync (full build only)
  listProjects: (accountId: string) => invoke<ProjectSummary[]>("list_projects", { accountId }),
  linkProject: (accountId: string, projectId: string, projectName: string, parent: string) =>
    invoke<AppConfig>("link_project", { accountId, projectId, projectName, parent }),
  unlinkProject: (linkId: string, deleteHistory: boolean) =>
    invoke<AppConfig>("unlink_project", { linkId, deleteHistory }),
  setLinkAuto: (linkId: string, on: boolean) => invoke<AppConfig>("set_link_auto", { linkId, on }),
  syncNow: (linkId: string, allowMassDelete = false) =>
    invoke<SyncReport>("sync_now", { linkId, allowMassDelete }),
  syncStatuses: () => invoke<LinkStatus[]>("sync_statuses"),
  linkHistory: (linkId: string) => invoke<Snapshot[]>("link_history", { linkId }),
  snapshotChanges: (linkId: string, snapshotId: string) =>
    invoke<Change[]>("snapshot_changes", { linkId, snapshotId }),
  snapshotDiff: (linkId: string, snapshotId: string, path: string) =>
    invoke<string>("snapshot_diff", { linkId, snapshotId, path }),
  restoreSnapshot: (linkId: string, snapshotId: string) =>
    invoke<RestoreReport>("restore_snapshot", { linkId, snapshotId }),
  openLinkFolder: (linkId: string) => invoke<void>("open_link_folder", { linkId }),
  // local mode (full build only)
  localFiles: (linkId: string) => invoke<FileEntry[]>("local_files", { linkId }),
  readText: (linkId: string, path: string) => invoke<string>("read_text", { linkId, path }),
  // `base` is the text the editor last loaded or saved: a change made to the
  // file since then is merged in; same-line edits fail with "conflict: ..."
  writeText: (linkId: string, path: string, text: string, base: string | null, force: boolean) =>
    invoke<{ text: string; merged: boolean }>("write_text", { linkId, path, text, base, force }),
  createText: (linkId: string, path: string) => invoke<void>("create_text", { linkId, path }),
  mainFile: (linkId: string) => invoke<string | null>("main_file", { linkId }),
  setMainFile: (linkId: string, path: string) => invoke<void>("set_main_file", { linkId, path }),
  compile: (linkId: string) => invoke<CompileOutcome>("compile_project", { linkId }),
  cancelCompile: (linkId: string) => invoke<void>("cancel_compile", { linkId }),
  // raw bytes, not JSON
  readPdf: (linkId: string) => invoke<ArrayBuffer>("read_pdf", { linkId }),
  synctexForward: (linkId: string, path: string, line: number) =>
    invoke<Spot[]>("synctex_forward", { linkId, path, line }),
  synctexBackward: (linkId: string, page: number, x: number, y: number) =>
    invoke<SourceSpot | null>("synctex_backward", { linkId, page, x, y }),
};

// tauri commands reject with a plain string; anything else gets stringified
export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
