//! The local TeX toolchain: which tools are installed and which engine to use.
//!
//! Canopy prefers a TeX Live or MiKTeX install driven by `latexmk`, and falls
//! back to Tectonic, which fetches packages on demand. Running compiles and
//! parsing their logs arrive with local mode; this crate only finds the tools.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Which compiler the user asked for in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnginePref {
    /// `latexmk` when installed, otherwise Tectonic.
    #[default]
    Auto,
    /// Always `latexmk` (TeX Live / MiKTeX).
    Latexmk,
    /// Always Tectonic.
    Tectonic,
    /// Never compile locally; leave it to the Overleaf server.
    Remote,
}

/// A compiler Canopy can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// `latexmk` from a TeX Live or MiKTeX install.
    Latexmk,
    /// The self-contained Tectonic engine.
    Tectonic,
}

/// The TeX tools found on this machine. A `None` means not installed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    /// `latexmk`, the build driver that ships with TeX Live and MiKTeX.
    pub latexmk: Option<PathBuf>,
    /// `pdflatex`, a sign that a TeX distribution is really there.
    pub pdflatex: Option<PathBuf>,
    /// `tectonic`, either on PATH or downloaded by Canopy.
    pub tectonic: Option<PathBuf>,
    /// `texlab`, the LaTeX language server behind completion and diagnostics.
    pub texlab: Option<PathBuf>,
    /// `synctex`, for jumping between source and PDF.
    pub synctex: Option<PathBuf>,
}

impl Toolchain {
    /// Looks for every tool on `PATH`, plus `extra_dirs` (where Canopy keeps
    /// the tools it downloads), which are searched first.
    pub fn detect(extra_dirs: &[PathBuf]) -> Self {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs: Vec<PathBuf> = extra_dirs.to_vec();
        dirs.extend(std::env::split_paths(&path));
        Self::detect_in(&dirs, &exe_extensions())
    }

    /// [`Toolchain::detect`] with an explicit search path, for tests.
    pub fn detect_in(dirs: &[PathBuf], exts: &[OsString]) -> Self {
        let find = |name: &str| find_program(name, dirs, exts);
        Self {
            latexmk: find("latexmk"),
            pdflatex: find("pdflatex"),
            tectonic: find("tectonic"),
            texlab: find("texlab"),
            synctex: find("synctex"),
        }
    }

    /// The engine a compile should use under `pref`, or `None` when the
    /// preferred tool is missing (the UI then offers to install Tectonic) or
    /// the user chose remote compiles.
    pub fn choose(&self, pref: EnginePref) -> Option<Engine> {
        match pref {
            EnginePref::Auto if self.latexmk.is_some() => Some(Engine::Latexmk),
            EnginePref::Auto if self.tectonic.is_some() => Some(Engine::Tectonic),
            EnginePref::Latexmk if self.latexmk.is_some() => Some(Engine::Latexmk),
            EnginePref::Tectonic if self.tectonic.is_some() => Some(Engine::Tectonic),
            _ => None,
        }
    }
}

/// Executable suffixes to try: `PATHEXT` on Windows, nothing elsewhere.
pub fn exe_extensions() -> Vec<OsString> {
    if cfg!(windows) {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        pathext
            .split(';')
            .filter(|e| !e.is_empty())
            .map(OsString::from)
            .collect()
    } else {
        Vec::new()
    }
}

/// Finds `name` in `dirs`, trying each extension in `exts` (and the bare name
/// when `exts` is empty). Returns the first match that is a file.
pub fn find_program(name: &str, dirs: &[PathBuf], exts: &[OsString]) -> Option<PathBuf> {
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if exts.is_empty() {
            let p = dir.join(name);
            if is_executable(&p) {
                return Some(p);
            }
        }
        for ext in exts {
            let mut file = OsString::from(name);
            file.push(ext);
            let p = dir.join(file);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch_exe(dir: &Path, file: &str) -> PathBuf {
        let p = dir.join(file);
        fs::write(&p, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    fn exts() -> Vec<OsString> {
        if cfg!(windows) {
            vec![".EXE".into(), ".BAT".into()]
        } else {
            Vec::new()
        }
    }

    fn exe(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.EXE")
        } else {
            name.to_string()
        }
    }

    #[test]
    fn finds_tools_and_prefers_earlier_dirs() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let tect_a = touch_exe(a.path(), &exe("tectonic"));
        touch_exe(b.path(), &exe("tectonic"));
        let mk = touch_exe(b.path(), &exe("latexmk"));
        let dirs = vec![a.path().to_path_buf(), b.path().to_path_buf()];
        let tc = Toolchain::detect_in(&dirs, &exts());
        assert_eq!(tc.tectonic, Some(tect_a));
        assert_eq!(tc.latexmk, Some(mk));
        assert_eq!(tc.texlab, None);
    }

    #[test]
    fn skips_directories_and_empty_entries() {
        let a = tempfile::tempdir().unwrap();
        fs::create_dir(a.path().join(exe("latexmk"))).unwrap();
        let dirs = vec![PathBuf::new(), a.path().to_path_buf()];
        assert_eq!(Toolchain::detect_in(&dirs, &exts()).latexmk, None);
    }

    #[cfg(unix)]
    #[test]
    fn skips_non_executable_files() {
        let a = tempfile::tempdir().unwrap();
        fs::write(a.path().join("latexmk"), b"").unwrap();
        let dirs = vec![a.path().to_path_buf()];
        assert_eq!(Toolchain::detect_in(&dirs, &[]).latexmk, None);
    }

    #[test]
    fn chooses_engine() {
        let none = Toolchain::default();
        let mk = Toolchain {
            latexmk: Some("latexmk".into()),
            ..Default::default()
        };
        let tect = Toolchain {
            tectonic: Some("tectonic".into()),
            ..Default::default()
        };
        let both = Toolchain {
            latexmk: mk.latexmk.clone(),
            tectonic: tect.tectonic.clone(),
            ..Default::default()
        };

        assert_eq!(none.choose(EnginePref::Auto), None);
        assert_eq!(mk.choose(EnginePref::Auto), Some(Engine::Latexmk));
        assert_eq!(tect.choose(EnginePref::Auto), Some(Engine::Tectonic));
        assert_eq!(both.choose(EnginePref::Auto), Some(Engine::Latexmk));
        assert_eq!(both.choose(EnginePref::Tectonic), Some(Engine::Tectonic));
        assert_eq!(tect.choose(EnginePref::Latexmk), None);
        assert_eq!(both.choose(EnginePref::Remote), None);
    }

    #[test]
    fn engine_pref_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&EnginePref::Tectonic).unwrap(),
            "\"tectonic\""
        );
        let p: EnginePref = serde_json::from_str("\"remote\"").unwrap();
        assert_eq!(p, EnginePref::Remote);
    }
}
