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

export type View = "projects" | "search" | "sync" | "history" | "dashboard" | "settings";
