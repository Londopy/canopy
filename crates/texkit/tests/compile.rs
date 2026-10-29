//! Real compiles with the TeX install on this machine:
//!
//!     cargo test -p canopy-texkit --test compile -- --ignored
//!
//! Ignored by default so CI doesn't need TeX.

use canopy_texkit::compile::{compile, Job, Program};
use canopy_texkit::log::Severity;
use canopy_texkit::synctex::Synctex;
use canopy_texkit::{search_path, EnginePref, Toolchain};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn project(dir: &Path, chapter: &str) {
    std::fs::create_dir_all(dir.join("chapters")).unwrap();
    std::fs::write(
        dir.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nHello from Canopy \\cite{knuth}.\n\n\\input{chapters/one}\n\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(dir.join("chapters").join("one.tex"), chapter).unwrap();
    std::fs::write(
        dir.join("refs.bib"),
        "@book{knuth, author={Donald Knuth}, title={The TeXbook}, year={1984}, publisher={Addison-Wesley}}\n",
    )
    .unwrap();
}

fn job(root: &Path, out: &Path) -> Option<Job> {
    let tc = Toolchain::detect(&[]);
    let engine = tc.choose(EnginePref::Auto)?;
    Some(Job {
        engine,
        search: search_path(&[]),
        root: root.to_path_buf(),
        main: "main.tex".into(),
        out_dir: out.to_path_buf(),
        timeout: Duration::from_secs(300),
    })
}

#[test]
#[ignore = "needs a TeX install"]
fn compiles_with_bibliography_and_synctex() {
    let dir = tempfile::tempdir().unwrap();
    let (root, out) = (dir.path().join("p"), dir.path().join("build"));
    project(&root, "This is chapter one.\n\nA second paragraph.\n");
    let Some(job) = job(&root, &out) else {
        eprintln!("no TeX installed; skipping");
        return;
    };
    let o = compile(&job, &AtomicBool::new(false)).unwrap();
    assert!(o.ok, "{o:#?}");
    assert_eq!(o.program, Program::Pdflatex);
    assert!(o.pdf.is_some());
    // the citation resolved, so BibTeX and the reruns happened
    assert!(
        !o.diagnostics
            .iter()
            .any(|d| d.message.contains("undefined")),
        "{:#?}",
        o.diagnostics
    );
    let sync = Synctex::load(o.synctex.as_ref().unwrap()).unwrap();
    let spots = sync.forward(&root.join("chapters").join("one.tex"), 3);
    assert_eq!(spots.len(), 1);
    let back = sync
        .backward(
            spots[0].page,
            spots[0].x + 5.0,
            spots[0].y + spots[0].height * 0.8,
        )
        .unwrap();
    assert!(back.file.ends_with("one.tex"), "{back:?}");
    assert_eq!(back.line, 3);
    // nothing landed in the project folder
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["chapters", "main.tex", "refs.bib"]);
}

#[test]
#[ignore = "needs a TeX install"]
fn reports_errors_with_file_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let (root, out) = (dir.path().join("p"), dir.path().join("build"));
    project(&root, "Fine.\n\nA broken \\textbff{command}.\n");
    let Some(job) = job(&root, &out) else {
        return;
    };
    let o = compile(&job, &AtomicBool::new(false)).unwrap();
    assert!(!o.ok);
    let e = o
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .expect("an error");
    assert_eq!(e.file.as_deref(), Some("chapters/one.tex"));
    assert_eq!(e.line, Some(3));
}

#[test]
#[ignore = "needs a TeX install"]
fn a_cancelled_compile_stops() {
    let dir = tempfile::tempdir().unwrap();
    let (root, out) = (dir.path().join("p"), dir.path().join("build"));
    project(&root, "x\n");
    let Some(job) = job(&root, &out) else {
        return;
    };
    let o = compile(&job, &AtomicBool::new(true)).unwrap();
    assert!(o.cancelled && !o.ok);
}
