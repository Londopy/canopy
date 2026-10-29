//! Running a compile: the engine, BibTeX or Biber and the reruns a document
//! needs, with a timeout and a way to cancel.
//!
//! Project files come from Overleaf, where collaborators can add anything, so
//! a compile must not run code from them: shell escape stays off, and
//! latexmk is told not to read the project's `.latexmkrc` (it is Perl).
//!
//! Build output goes to a folder of its own, outside the project, so the
//! sync never sees it.

use crate::log::{self, Diagnostic, Severity};
use crate::{exe_extensions, find_program, Engine};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Which TeX program builds the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Program {
    /// pdfLaTeX, the default.
    Pdflatex,
    /// XeLaTeX, for system fonts (fontspec).
    Xelatex,
    /// LuaLaTeX.
    Lualatex,
}

impl Program {
    fn binary(self) -> &'static str {
        match self {
            Program::Pdflatex => "pdflatex",
            Program::Xelatex => "xelatex",
            Program::Lualatex => "lualatex",
        }
    }

    // what keeps a document from running code or loading files it names:
    // no shell escape, no "%&format" first line, and LuaTeX's safer mode
    fn safety_flags(self) -> &'static [&'static str] {
        match self {
            Program::Lualatex => &["-no-shell-escape", "-no-parse-first-line", "--safer"],
            _ => &["-no-shell-escape", "-no-parse-first-line"],
        }
    }

    fn latexmk_flag(self) -> &'static str {
        match self {
            Program::Pdflatex => "-pdf",
            Program::Xelatex => "-xelatex",
            Program::Lualatex => "-lualatex",
        }
    }
}

/// The program a main file asks for: a `% !TEX program = xelatex` line at
/// the top, else XeLaTeX when it loads fontspec, else pdfLaTeX.
pub fn program_for(main_source: &str) -> Program {
    for line in main_source.lines().take(20) {
        let l = line.trim().to_ascii_lowercase();
        if let Some(rest) = l.strip_prefix('%') {
            let rest = rest.trim();
            if let Some(v) = rest
                .strip_prefix("!tex program")
                .or_else(|| rest.strip_prefix("!tex ts-program"))
            {
                let v = v.trim_start_matches([' ', '=']).trim();
                return match v {
                    "xelatex" | "xetex" => Program::Xelatex,
                    "lualatex" | "luatex" => Program::Lualatex,
                    _ => Program::Pdflatex,
                };
            }
        }
    }
    let preamble = main_source
        .split("\\begin{document}")
        .next()
        .unwrap_or(main_source);
    if preamble.contains("{fontspec}") || preamble.contains("{unicode-math}") {
        Program::Xelatex
    } else {
        Program::Pdflatex
    }
}

/// Whether a project path can be a main file: a `.tex` file none of whose
/// parts starts with `-`, which a program would read as an option
/// (`-pdflatex=...` makes latexmk run any command).
pub fn safe_main(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".tex")
        && !path.is_empty()
        && path
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && !part.starts_with('-') && part != ".." && part != ".")
}

/// Picks the main file of a project folder: `main.tex` when there is one,
/// else a `.tex` file at the top that has `\documentclass`, else any that has.
pub fn find_main(root: &Path, tex_files: &[String]) -> Option<String> {
    let tex_files: Vec<String> = tex_files.iter().filter(|f| safe_main(f)).cloned().collect();
    let tex_files = &tex_files[..];
    if tex_files.iter().any(|f| f == "main.tex") {
        return Some("main.tex".into());
    }
    let has_class = |f: &String| {
        std::fs::read_to_string(root.join(f))
            .map(|s| {
                s.lines()
                    .any(|l| l.trim_start().starts_with("\\documentclass"))
            })
            .unwrap_or(false)
    };
    let mut top: Vec<&String> = tex_files.iter().filter(|f| !f.contains('/')).collect();
    top.sort();
    if let Some(f) = top.into_iter().find(|f| has_class(f)) {
        return Some(f.clone());
    }
    let mut all: Vec<&String> = tex_files.iter().collect();
    all.sort();
    all.into_iter().find(|f| has_class(f)).cloned()
}

/// One compile to run.
#[derive(Debug, Clone)]
pub struct Job {
    /// latexmk, a direct engine run, or Tectonic.
    pub engine: Engine,
    /// Folders to look for the programs in (Canopy's tools folder first,
    /// then PATH), as [`crate::Toolchain::detect`] uses them.
    pub search: Vec<PathBuf>,
    /// The project folder.
    pub root: PathBuf,
    /// The main file, relative to `root` with `/` separators.
    pub main: String,
    /// Where the PDF, log and SyncTeX file go.
    pub out_dir: PathBuf,
    /// Give up after this long.
    pub timeout: Duration,
}

/// What a compile produced.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    /// The PDF was written and no errors were logged.
    pub ok: bool,
    /// The engine that ran.
    pub engine: Engine,
    /// The TeX program behind it.
    pub program: Program,
    /// The PDF, if one was written (it can exist despite errors).
    pub pdf: Option<PathBuf>,
    /// The SyncTeX file next to it.
    pub synctex: Option<PathBuf>,
    /// Errors, warnings and bad boxes from the log.
    pub diagnostics: Vec<Diagnostic>,
    /// The last lines of the programs' output, for when the log says nothing.
    pub output_tail: String,
    /// How many programs ran.
    pub runs: u32,
    /// Seconds from start to finish.
    pub seconds: f64,
    /// Stopped by the user.
    pub cancelled: bool,
    /// Stopped by the timeout.
    pub timed_out: bool,
}

/// Why a compile couldn't start.
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    /// The program to run isn't installed.
    #[error("{0} isn't installed")]
    Missing(String),
    /// The main file isn't there.
    #[error("the main file {0} doesn't exist")]
    NoMain(String),
    /// The main file's name could be read as a program option.
    #[error("{0} can't be the main file: a name starting with \"-\" could be taken for an option")]
    BadMain(String),
    /// Running a program failed.
    #[error("couldn't run {program}: {source}")]
    Spawn {
        /// Which program.
        program: String,
        /// Why.
        source: std::io::Error,
    },
}

enum RunEnd {
    Done(bool),
    Cancelled,
    TimedOut,
}

struct Runner<'a> {
    job: &'a Job,
    cancel: &'a AtomicBool,
    deadline: Instant,
    output: String,
    runs: u32,
}

impl Runner<'_> {
    fn program(&self, name: &str) -> Result<PathBuf, CompileError> {
        find_program(name, &self.job.search, &exe_extensions())
            .ok_or_else(|| CompileError::Missing(name.to_string()))
    }

    fn run(&mut self, mut cmd: Command, name: &str) -> Result<RunEnd, CompileError> {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // TeX Live: no reading or writing files by absolute path or "..",
            // so a document can't pull other files on this computer into its
            // PDF. (MiKTeX ignores these; it refuses such writes on its own.)
            .env("openin_any", "p")
            .env("openout_any", "p");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // no console window flashing up for every run
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        // its own process group, so a kill reaches the engine latexmk starts
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let mut child = cmd.spawn().map_err(|source| CompileError::Spawn {
            program: name.to_string(),
            source,
        })?;
        self.runs += 1;
        // drain the pipes on threads so a chatty program never blocks
        let readers: Vec<_> = [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|r| {
            std::thread::spawn(move || {
                use std::io::Read;
                // keep the first 4 MB; the rest is read and dropped so the
                // program never blocks on a full pipe
                let mut buf = Vec::new();
                let mut r = r;
                let _ = (&mut r).take(4 * 1024 * 1024).read_to_end(&mut buf);
                let _ = std::io::copy(&mut r, &mut std::io::sink());
                buf
            })
        })
        .collect();

        let end = loop {
            match child.try_wait() {
                Ok(Some(status)) => break RunEnd::Done(status.success()),
                Ok(None) => {}
                Err(_) => break RunEnd::Done(false),
            }
            if self.cancel.load(Ordering::Relaxed) {
                kill_tree(&mut child);
                break RunEnd::Cancelled;
            }
            if Instant::now() > self.deadline {
                kill_tree(&mut child);
                break RunEnd::TimedOut;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        for r in readers {
            if let Ok(buf) = r.join() {
                self.output.push_str(&String::from_utf8_lossy(&buf));
            }
        }
        Ok(end)
    }
}

// latexmk runs perl, which runs the engine: stop the whole tree
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(unix)]
    {
        // the whole group: latexmk's engine keeps the output pipe open otherwise
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

// pdflatex won't create the folders \include writes .aux files into
fn mirror_dirs(root: &Path, out: &Path, main: &str) {
    let _ = std::fs::create_dir_all(out);
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&rel)) else {
            continue;
        };
        for e in entries.flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()) {
                let name = e.file_name();
                if name.to_string_lossy().starts_with('.') {
                    continue;
                }
                let sub = rel.join(&name);
                let _ = std::fs::create_dir_all(out.join(&sub));
                stack.push(sub);
            }
        }
    }
    if let Some(parent) = Path::new(main).parent() {
        let _ = std::fs::create_dir_all(out.join(parent));
    }
}

fn path_list(dirs: &[&Path]) -> std::ffi::OsString {
    let joined = std::env::join_paths(dirs).unwrap_or_default();
    // a trailing separator keeps TeX's default search path too
    let mut s = joined;
    s.push(if cfg!(windows) { ";" } else { ":" });
    s
}

/// Runs a compile. Blocks until it finishes, is cancelled or times out.
pub fn compile(job: &Job, cancel: &AtomicBool) -> Result<Outcome, CompileError> {
    let started = Instant::now();
    if !safe_main(&job.main) {
        return Err(CompileError::BadMain(job.main.clone()));
    }
    // "./name" so nothing can take the file for an option, whatever it's called
    let main_arg = format!("./{}", job.main);
    let main_path = job.root.join(&job.main);
    let source =
        std::fs::read_to_string(&main_path).map_err(|_| CompileError::NoMain(job.main.clone()))?;
    let program = if job.engine == Engine::Tectonic {
        Program::Xelatex
    } else {
        program_for(&source)
    };
    let stem = Path::new(&job.main)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    mirror_dirs(&job.root, &job.out_dir, &job.main);
    // a stale PDF from last time must not look like this run's
    let pdf_path = job.out_dir.join(format!("{stem}.pdf"));
    let log_path = job.out_dir.join(format!("{stem}.log"));
    // latexmk decides from the old files whether anything needs running, so
    // its log stays; the others always run
    if job.engine != Engine::Latexmk {
        let _ = std::fs::remove_file(&log_path);
    }
    let before_pdf = std::fs::metadata(&pdf_path).and_then(|m| m.modified()).ok();

    let mut r = Runner {
        job,
        cancel,
        deadline: started + job.timeout,
        output: String::new(),
        runs: 0,
    };
    let end = match job.engine {
        Engine::Latexmk => {
            let mut cmd = Command::new(r.program("latexmk")?);
            cmd.current_dir(&job.root)
                .arg("-norc")
                .arg(program.latexmk_flag())
                // the engine command latexmk runs: shell escape off, and no
                // "%&format" first line loading a format file from the project
                .arg(format!(
                    "-{}={} {} %O %S",
                    program.binary(),
                    program.binary(),
                    program.safety_flags().join(" ")
                ))
                .args([
                    "-interaction=nonstopmode",
                    "-synctex=1",
                    "-file-line-error",
                    "-bibtex-cond",
                ])
                .arg(format!("-outdir={}", job.out_dir.display()))
                .arg(&main_arg);
            r.run(cmd, "latexmk")?
        }
        Engine::Tectonic => {
            let mut cmd = Command::new(r.program("tectonic")?);
            cmd.current_dir(&job.root)
                .args([
                    "-X",
                    "compile",
                    // no shell escape or other unsafe features, whatever the
                    // document asks for
                    "--untrusted",
                    "--synctex",
                    "--keep-logs",
                    "--keep-intermediates",
                ])
                .arg("--outdir")
                .arg(&job.out_dir)
                .arg(&main_arg);
            r.run(cmd, "tectonic")?
        }
        Engine::Pdflatex => direct(&mut r, program, &stem, &main_arg)?,
    };

    let log_text = std::fs::read_to_string(&log_path).unwrap_or_default();
    let diagnostics = log::parse(&log_text, &job.root);
    let fresh_pdf = std::fs::metadata(&pdf_path)
        .and_then(|m| m.modified())
        .ok()
        .is_some_and(|t| Some(t) != before_pdf);
    let errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    let (cancelled, timed_out, success) = match end {
        RunEnd::Done(ok) => (false, false, ok),
        RunEnd::Cancelled => (true, false, false),
        RunEnd::TimedOut => (false, true, false),
    };
    // a run stopped halfway leaves a cut-off PDF behind: don't offer it
    let stopped = cancelled || timed_out;
    let pdf = (!stopped && pdf_path.exists()).then_some(pdf_path);
    let synctex = if stopped {
        None
    } else {
        [".synctex.gz", ".synctex"]
            .iter()
            .map(|ext| job.out_dir.join(format!("{stem}{ext}")))
            .find(|p| p.exists())
    };
    // latexmk found nothing to do: last time's PDF is still this document's
    let up_to_date = job.engine == Engine::Latexmk
        && (r.output.contains("Nothing to do for") || r.output.contains("are up-to-date"));
    let tail: Vec<&str> = r.output.lines().rev().take(40).collect();
    Ok(Outcome {
        ok: success && (fresh_pdf || up_to_date) && pdf.is_some() && !errors,
        engine: job.engine,
        program,
        pdf,
        synctex,
        diagnostics,
        output_tail: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
        runs: r.runs,
        seconds: started.elapsed().as_secs_f64(),
        cancelled,
        timed_out,
    })
}

// what latexmk would do, for when it can't run (MiKTeX without Perl): the
// engine, BibTeX or Biber when the document cites, MakeIndex for an index,
// and up to three more passes until references settle
fn direct(
    r: &mut Runner,
    program: Program,
    stem: &str,
    main_arg: &str,
) -> Result<RunEnd, CompileError> {
    let job = r.job;
    let engine = r.program(program.binary())?;
    let pass = |r: &mut Runner| -> Result<RunEnd, CompileError> {
        let mut cmd = Command::new(&engine);
        cmd.current_dir(&job.root)
            .args(program.safety_flags())
            .args(["-interaction=nonstopmode", "-synctex=1", "-file-line-error"])
            .arg(format!("-output-directory={}", job.out_dir.display()))
            .arg(main_arg);
        r.run(cmd, program.binary())
    };

    let mut end = pass(r)?;
    if !matches!(end, RunEnd::Done(_)) {
        return Ok(end);
    }
    let aux = std::fs::read_to_string(job.out_dir.join(format!("{stem}.aux"))).unwrap_or_default();
    let mut extra = false;
    if job.out_dir.join(format!("{stem}.bcf")).exists() {
        if let Ok(biber) = r.program("biber") {
            let mut cmd = Command::new(biber);
            // next to the .bcf, never in the project (it would run helper
            // programs or read a biber.conf from there); .bib files are
            // found through BIBINPUTS
            cmd.current_dir(&job.out_dir)
                .env("BIBINPUTS", path_list(&[&job.root]))
                .arg("--noconf")
                .arg(stem);
            end = r.run(cmd, "biber")?;
            extra = true;
        }
    } else if aux.contains("\\bibdata") {
        if let Ok(bibtex) = r.program("bibtex") {
            let mut cmd = Command::new(bibtex);
            // bibtex runs next to the .aux and finds .bib files in the project
            cmd.current_dir(&job.out_dir)
                .env("BIBINPUTS", path_list(&[&job.root]))
                .env("BSTINPUTS", path_list(&[&job.root]))
                .arg(stem);
            end = r.run(cmd, "bibtex")?;
            extra = true;
        }
    }
    if !matches!(end, RunEnd::Done(_)) {
        return Ok(end);
    }
    let idx = job.out_dir.join(format!("{stem}.idx"));
    if idx.exists() {
        if let Ok(makeindex) = r.program("makeindex") {
            let mut cmd = Command::new(makeindex);
            cmd.current_dir(&job.out_dir).arg(format!("{stem}.idx"));
            end = r.run(cmd, "makeindex")?;
            extra = true;
        }
    }
    let log_path = job.out_dir.join(format!("{stem}.log"));
    for _ in 0..3 {
        if !matches!(end, RunEnd::Done(_)) {
            return Ok(end);
        }
        let log_text = std::fs::read_to_string(&log_path).unwrap_or_default();
        if !extra && !log::wants_rerun(&log_text) {
            break;
        }
        extra = false;
        end = pass(r)?;
    }
    Ok(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_program() {
        assert_eq!(program_for("\\documentclass{article}"), Program::Pdflatex);
        assert_eq!(
            program_for("% !TEX program = xelatex\n\\documentclass{article}"),
            Program::Xelatex
        );
        assert_eq!(
            program_for("%!TEX TS-program = lualatex\n"),
            Program::Lualatex
        );
        assert_eq!(
            program_for("\\documentclass{article}\n\\usepackage{fontspec}\n\\begin{document}"),
            Program::Xelatex
        );
        // fontspec mentioned only in the body doesn't count
        assert_eq!(
            program_for("\\documentclass{article}\n\\begin{document}\n\\verb|{fontspec}|"),
            Program::Pdflatex
        );
    }

    #[test]
    fn option_like_names_are_never_main_files() {
        assert!(safe_main("main.tex"));
        assert!(safe_main("ch/thesis.tex"));
        assert!(!safe_main("-pdflatex=powershell -c calc;.tex"));
        assert!(!safe_main("ch/-x.tex"));
        assert!(!safe_main("../x.tex"));
        assert!(!safe_main("main.bib"));
        let dir = tempfile::tempdir().unwrap();
        let evil = "-pdflatex=calc;.tex".to_string();
        std::fs::write(dir.path().join(&evil), "\\documentclass{article}").unwrap();
        assert_eq!(find_main(dir.path(), std::slice::from_ref(&evil)), None);
        let job = Job {
            engine: Engine::Pdflatex,
            search: Vec::new(),
            root: dir.path().to_path_buf(),
            main: evil,
            out_dir: dir.path().join("out"),
            timeout: Duration::from_secs(5),
        };
        assert!(matches!(
            compile(&job, &AtomicBool::new(false)),
            Err(CompileError::BadMain(_))
        ));
    }

    #[test]
    fn finds_the_main_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("ch")).unwrap();
        std::fs::write(root.join("a.tex"), "just text").unwrap();
        std::fs::write(root.join("paper.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(root.join("ch").join("x.tex"), "\\documentclass{article}").unwrap();
        let files = vec!["a.tex".to_string(), "ch/x.tex".into(), "paper.tex".into()];
        assert_eq!(find_main(root, &files).as_deref(), Some("paper.tex"));
        let mut with_main = files.clone();
        with_main.push("main.tex".into());
        assert_eq!(find_main(root, &with_main).as_deref(), Some("main.tex"));
        assert_eq!(find_main(root, &["a.tex".into()]), None);
    }
}
