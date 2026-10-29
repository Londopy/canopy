// mirrors of the rust types the commands send back and forth

export type Theme = "system" | "light" | "dark";
export type EnginePref = "auto" | "latexmk" | "tectonic" | "remote";
export type Engine = "latexmk" | "tectonic";

export interface Account {
  id: string;
  label: string;
  server: string;
}

export interface AppConfig {
  general: {
    theme: Theme;
    lite_mode: boolean;
    close_to_tray: boolean;
    start_with_system: boolean;
  };
  compile: { engine: EnginePref };
  updater: { auto_update: boolean; skipped_version: string };
  accounts: Account[];
  links: SyncLink[];
}

export interface SyncLink {
  id: string;
  account_id: string;
  project_id: string;
  project_name: string;
  folder: string;
  auto_sync: boolean;
}

export interface ProjectSummary {
  _id: string;
  name: string;
  accessLevel: string;
}

export type SyncStateName = "off" | "synced" | "syncing" | "pending" | "conflict" | "error";

export interface Conflict {
  path: string;
  kind: "text" | "binary" | "unresolved";
}

export interface LinkStatus {
  id: string;
  state: SyncStateName;
  lastSync: number | null;
  summary: string | null;
  error: string | null;
  conflicts: Conflict[];
  massDelete: [number, number] | null;
  failed: string[];
  unsyncable: string[];
  keptCopies: string[];
}

export interface RestoreReport {
  changed: number;
  before: string;
  sync: SyncReport;
}

export interface SyncReport {
  downloaded: string[];
  uploaded: string[];
  deleted_local: string[];
  deleted_remote: string[];
  merged: string[];
  conflicts: Conflict[];
  skipped: string[];
  failed: string[];
  unsyncable: string[];
  kept_copies: string[];
  snapshot: string | null;
}

export interface Snapshot {
  id: string;
  time: number;
  message: string;
}

export interface Change {
  path: string;
  kind: "added" | "modified" | "deleted";
}

export interface AppInfo {
  name: string;
  version: string;
  build_date: string;
  full_build: boolean;
  portable: boolean;
  can_self_update: boolean;
  data_dir: string;
  os: string;
  arch: string;
  homepage: string;
}

export interface Toolchain {
  latexmk: string | null;
  pdflatex: string | null;
  tectonic: string | null;
  texlab: string | null;
  synctex: string | null;
}

export interface TexReport {
  tools: Toolchain;
  engine: Engine | null;
}

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
  date: string | null;
}

export type Routed =
  | { result: "mine"; kind: string; path: string }
  | { result: "forwarded"; app: string }
  | { result: "not-installed"; app: string; homepage: string };

export type View = "projects" | "web" | "search" | "sync" | "history" | "dashboard" | "settings";

// what an account's page last reported (web.rs PageState)
export interface PageState {
  id: string;
  url: string;
  // null on pages that don't say, like an SSO provider's
  signedIn: boolean | null;
  loading: boolean;
  title?: string;
}

export type WebAction = "back" | "forward" | "reload" | "home" | "open-in-browser";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
