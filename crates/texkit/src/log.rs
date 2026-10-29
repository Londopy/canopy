//! Reading a TeX log for the problems panel: errors, warnings and bad boxes,
//! each with the file and line it's about when the log says.
//!
//! TeX logs are made for people. Two quirks drive this parser:
//! - lines are wrapped at 79 columns, splitting file names and messages, so
//!   full-width lines are joined back up first;
//! - warnings say "on input line N" but not which file. The log opens each
//!   file with `(path` and closes it with `)`, so a stack of open files says
//!   which one a message belongs to.

use regex::Regex;
use serde::Serialize;
use std::path::Path;
use std::sync::LazyLock;

/// How bad a log entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The document has an error; the PDF may be missing or wrong.
    Error,
    /// LaTeX or a package warned: undefined references, missing glyphs.
    Warning,
    /// An overfull or underfull box: text sticking into the margin.
    Badbox,
}

/// One problem from the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// How bad it is.
    pub severity: Severity,
    /// The file, relative to the project folder with `/` separators when it's
    /// in the project, absolute when it's a package or class file.
    pub file: Option<String>,
    /// The line in that file, 1-based.
    pub line: Option<u32>,
    /// What went wrong, on one line.
    pub message: String,
}

/// TeX's default `max_print_line`: longer lines are wrapped at this width.
const WRAP: usize = 79;

static FILE_LINE_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+?\.(?:tex|sty|cls|bib|bbl|ltx|dtx|def|cfg|clo|fd|aux|toc)):(\d+): (.*)$")
        .expect("valid regex")
});
static WARNING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:LaTeX|Package [\w.@-]+|Class [\w.@-]+|pdfTeX)(?: Font)? Warning: (.*)$")
        .expect("valid regex")
});
static ON_INPUT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"on input line (\d+)").expect("valid regex"));
static BADBOX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:Over|Under)full \\[hv]box .*?)(?:in paragraph at lines (\d+)--\d+|in alignment at lines (\d+)--\d+|detected at line (\d+)|has occurred while \\output is active)")
        .expect("valid regex")
});
static ERROR_CONTEXT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^l\.(\d+)").expect("valid regex"));

// joins lines TeX split at the 79th column back together
fn unwrap(log: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut joining = false;
    for raw in log.lines() {
        let line = raw.trim_end_matches('\r');
        if joining {
            if let Some(last) = out.last_mut() {
                last.push_str(line);
            }
        } else {
            out.push(line.to_string());
        }
        joining = line.chars().count() == WRAP;
    }
    out
}

// a project-relative, `/`-separated name for a file the log mentions
fn tidy_path(raw: &str, root: &Path) -> String {
    let p = raw.trim().trim_matches('"').replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p).to_string();
    let root_s = root.to_string_lossy().replace('\\', "/");
    let root_s = root_s.trim_end_matches('/');
    let lower = |s: &str| {
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    if !root_s.is_empty() && lower(&p).starts_with(&(lower(root_s) + "/")) {
        p[root_s.len() + 1..].to_string()
    } else {
        p
    }
}

// whether a token after "(" is a file TeX opened
fn looks_like_file(tok: &str) -> bool {
    let t = tok.trim_matches('"');
    !t.is_empty()
        && t.len() < 1024
        && (t.contains('/') || t.contains('\\') || t.contains('.'))
        && t.rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name.contains('.'))
}

// tracks which file the log is in, from its "(file" and ")" marks
#[derive(Default)]
struct FileStack {
    stack: Vec<Option<String>>,
    // the project's own files, longest first: TeX writes names with spaces or
    // parentheses unquoted ("(my chapter.tex"), so those are matched whole
    known: Vec<String>,
}

impl FileStack {
    // a project file the text right after "(" names, with its length
    fn known_at(&self, rest: &str, root: &Path) -> Option<(String, usize)> {
        let root_s = root.to_string_lossy().replace('\\', "/");
        let rest_n = rest.replace('\\', "/");
        for k in &self.known {
            for prefix in [
                "./".to_string(),
                String::new(),
                format!("{}/", root_s.trim_end_matches('/')),
            ] {
                let full = format!("{prefix}{k}");
                let hit = if cfg!(windows) {
                    rest_n.len() >= full.len() && rest_n[..full.len()].eq_ignore_ascii_case(&full)
                } else {
                    rest_n.starts_with(&full)
                };
                if hit {
                    return Some((k.clone(), full.chars().count()));
                }
            }
        }
        None
    }

    fn scan(&mut self, line: &str, root: &Path) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '(' => {
                    let start = i + 1;
                    let rest: String = chars[start..].iter().collect();
                    if let Some((file, len)) = self.known_at(&rest, root) {
                        self.stack.push(Some(file));
                        i = start + len;
                        continue;
                    }
                    let mut end = start;
                    // a quoted name may contain spaces
                    if chars.get(start) == Some(&'"') {
                        end += 1;
                        while end < chars.len() && chars[end] != '"' {
                            end += 1;
                        }
                        end = (end + 1).min(chars.len());
                    } else {
                        while end < chars.len()
                            && !matches!(chars[end], ' ' | '(' | ')' | '[' | '{' | '<')
                        {
                            end += 1;
                        }
                    }
                    let tok: String = chars[start..end].iter().collect();
                    self.stack
                        .push(looks_like_file(&tok).then(|| tidy_path(&tok, root)));
                    i = end;
                }
                ')' => {
                    self.stack.pop();
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }

    fn current(&self) -> Option<String> {
        self.stack.iter().rev().flatten().next().cloned()
    }
}

/// The problems in a TeX log. `root` is the project folder, used to show
/// project files by their relative path.
pub fn parse(log: &str, root: &Path) -> Vec<Diagnostic> {
    parse_with_files(log, root, &project_sources(root))
}

// the project's TeX source files, relative with "/" separators
fn project_sources(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(t) = e.file_type() else { continue };
            let p = e.path();
            if t.is_dir() && !t.is_symlink() {
                if !e.file_name().to_string_lossy().starts_with('.') && out.len() < 5000 {
                    stack.push(p);
                }
            } else if t.is_file() {
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let lower = rel.to_ascii_lowercase();
                if [
                    ".tex", ".sty", ".cls", ".bib", ".bbx", ".cbx", ".def", ".cfg", ".clo",
                ]
                .iter()
                .any(|e| lower.ends_with(e))
                {
                    out.push(rel);
                }
            }
        }
    }
    out
}

/// [`parse`] with the project's source files given rather than read from disk.
pub fn parse_with_files(log: &str, root: &Path, known: &[String]) -> Vec<Diagnostic> {
    let lines = unwrap(log);
    let mut known = known.to_vec();
    known.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let mut files = FileStack {
        known,
        ..Default::default()
    };
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];

        // file:line: message (the -file-line-error style)
        if let Some(c) = FILE_LINE_ERROR.captures(line) {
            let file = tidy_path(&c[1], root);
            let msg = c[3].trim().to_string();
            out.push(Diagnostic {
                severity: Severity::Error,
                file: Some(file),
                line: c[2].parse().ok(),
                message: msg,
            });
            i = skip_error_context(&lines, i + 1);
            continue;
        }

        // ! message ... l.N (the plain style, and Tectonic's)
        if let Some(msg) = line.strip_prefix("! ") {
            let mut line_no = None;
            let mut j = i + 1;
            while j < lines.len() && j < i + 12 {
                if let Some(c) = ERROR_CONTEXT_LINE.captures(&lines[j]) {
                    line_no = c[1].parse().ok();
                    break;
                }
                j += 1;
            }
            out.push(Diagnostic {
                severity: Severity::Error,
                file: files.current(),
                line: line_no,
                message: msg.trim().to_string(),
            });
            i = skip_error_context(&lines, i + 1);
            continue;
        }

        // warnings run on until a blank line
        if let Some(c) = WARNING.captures(line) {
            let mut msg = c[1].trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && j < i + 6 && !lines[j].trim().is_empty() {
                // package warnings indent their continuation with "(pkg)"
                let more = lines[j].trim();
                let more = more
                    .strip_prefix('(')
                    .and_then(|r| r.split_once(')'))
                    .map(|(_, rest)| rest.trim())
                    .unwrap_or(more);
                msg.push(' ');
                msg.push_str(more);
                j += 1;
            }
            let line_no = ON_INPUT_LINE.captures(&msg).and_then(|c| c[1].parse().ok());
            out.push(Diagnostic {
                severity: Severity::Warning,
                file: files.current(),
                line: line_no,
                message: tidy_message(&msg),
            });
            i = j;
            continue;
        }

        if let Some(c) = BADBOX.captures(line) {
            let line_no = c
                .get(2)
                .or_else(|| c.get(3))
                .or_else(|| c.get(4))
                .and_then(|m| m.as_str().parse().ok());
            out.push(Diagnostic {
                severity: Severity::Badbox,
                file: files.current(),
                line: line_no,
                message: c[1].trim().to_string(),
            });
            i += 1;
            continue;
        }

        files.scan(line, root);
        i += 1;
    }
    out
}

// past an error's help text and context: up to the "l.N" line, the line after
// it, then the help paragraph that ends at a blank line. none of it is the
// log's file structure, so its parentheses must not touch the stack
fn skip_error_context(lines: &[String], from: usize) -> usize {
    let mut j = from;
    let limit = (from + 20).min(lines.len());
    let mut seen_context = false;
    while j < limit {
        if !seen_context && ERROR_CONTEXT_LINE.is_match(&lines[j]) {
            seen_context = true;
            j += 2;
            continue;
        }
        if lines[j].trim().is_empty() {
            return if seen_context { j + 1 } else { j };
        }
        j += 1;
    }
    j.min(lines.len())
}

fn tidy_message(msg: &str) -> String {
    let words: Vec<&str> = msg.split_whitespace().collect();
    words.join(" ")
}

/// Whether the log asks for another run (references, page numbers, the table
/// of contents not settled yet).
pub fn wants_rerun(log: &str) -> bool {
    let flat = unwrap(log).join("\n");
    flat.contains("Rerun to get")
        || flat.contains("Label(s) may have changed")
        || flat.contains("There were undefined references") && flat.contains("Please (re)run")
        || flat.contains("Please rerun LaTeX")
        || flat.contains("rerun to get")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join(name),
        )
        .unwrap()
    }

    // the fixtures were compiled from inside the project, so the log names
    // project files relatively and the root doesn't matter
    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from("C:\\proj")
    }

    #[test]
    fn errors_and_warnings_from_a_real_log() {
        let d = parse(&fixture("error.log"), &root());
        let errors: Vec<_> = d.iter().filter(|x| x.severity == Severity::Error).collect();
        assert_eq!(errors.len(), 2, "{d:#?}");
        assert_eq!(errors[0].file.as_deref(), Some("chapters/one.tex"));
        assert_eq!(errors[0].line, Some(3));
        assert_eq!(errors[0].message, "Undefined control sequence.");
        assert_eq!(errors[1].file.as_deref(), Some("main.tex"));
        assert_eq!(errors[1].line, Some(9));
        // the message was wrapped at 79 columns in the log
        assert!(
            errors[1].message.contains(r"\end{document}"),
            "{}",
            errors[1].message
        );

        let warn = d
            .iter()
            .find(|x| x.message.contains("Reference `nope'"))
            .expect("undefined reference warning");
        assert_eq!(warn.severity, Severity::Warning);
        assert_eq!(warn.line, Some(8));
        assert_eq!(warn.file.as_deref(), Some("main.tex"));
    }

    #[test]
    fn a_clean_log_has_only_its_warning() {
        let d = parse(&fixture("clean.log"), &root());
        assert!(d.iter().all(|x| x.severity != Severity::Error), "{d:#?}");
        assert!(d
            .iter()
            .any(|x| x.line == Some(8) && x.file.as_deref() == Some("main.tex")));
    }

    #[test]
    fn plain_style_errors_take_the_open_file() {
        let log = "(./main.tex (./sec/a.tex\n! Undefined control sequence.\nl.12 \\foo\n          bar\n\n) )\n";
        let d = parse(log, Path::new("/p"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some("sec/a.tex"));
        assert_eq!(d[0].line, Some(12));
    }

    #[test]
    fn package_warnings_and_bad_boxes() {
        let log = "(./main.tex\nPackage hyperref Warning: Token not allowed in a PDF string (Unicode):\n(hyperref)                removing `math shift' on input line 30.\n\nOverfull \\hbox (12.3pt too wide) in paragraph at lines 40--42\n)\n";
        let d = parse(log, Path::new("/p"));
        assert_eq!(d.len(), 2, "{d:#?}");
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].line, Some(30));
        assert!(d[0].message.starts_with("Token not allowed"));
        assert!(d[0].message.contains("removing `math shift'"));
        assert_eq!(d[1].severity, Severity::Badbox);
        assert_eq!(d[1].line, Some(40));
        assert_eq!(d[1].file.as_deref(), Some("main.tex"));
    }

    #[test]
    fn file_names_with_spaces_and_parentheses() {
        let known = vec!["my chapter.tex".to_string(), "weird(1).tex".to_string()];
        let log = "(./main.tex (./my chapter.tex\nLaTeX Warning: Reference `a' on page 1 undefined on input line 4.\n\n) (./weird(1).tex\nLaTeX Warning: Reference `b' on page 1 undefined on input line 7.\n\n) )\n";
        let d = parse_with_files(log, Path::new("/p"), &known);
        let where_: Vec<_> = d.iter().map(|x| (x.file.as_deref(), x.line)).collect();
        assert_eq!(
            where_,
            vec![
                (Some("my chapter.tex"), Some(4)),
                (Some("weird(1).tex"), Some(7))
            ]
        );
    }

    #[test]
    fn absolute_paths_in_the_project_become_relative() {
        assert_eq!(
            tidy_path("C:\\proj\\ch\\a.tex", Path::new("C:\\proj")),
            "ch/a.tex"
        );
        assert_eq!(tidy_path("./b.tex", Path::new("C:\\proj")), "b.tex");
        assert_eq!(
            tidy_path("C:/tex/latex/base/article.cls", Path::new("C:\\proj")),
            "C:/tex/latex/base/article.cls"
        );
    }

    #[test]
    fn rerun_detection() {
        assert!(wants_rerun(
            "LaTeX Warning: Label(s) may have changed. Rerun to get cross-references right.\n"
        ));
        assert!(!wants_rerun("Output written on main.pdf (1 page).\n"));
    }
}
