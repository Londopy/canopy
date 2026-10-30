// hides the console window in release builds on Windows
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    // started under the name "latexmk": canopy's stand-in for texlab, which
    // would otherwise run the real latexmk and with it a project's .latexmkrc
    // (Perl a collaborator wrote). do nothing, successfully
    let stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()));
    if stem.as_deref() == Some("latexmk") {
        return;
    }
    canopy_lib::run()
}
