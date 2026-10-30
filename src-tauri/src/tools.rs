// tools.rs
// programs canopy downloads for you, into its own tools folder: for now
// texlab, the LaTeX language server. each download is a release build pinned
// by version and SHA-256, so what runs is exactly what was checked.
//
// only in the full build.

use crate::config;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

pub const TEXLAB_VERSION: &str = "5.26.0";

// (target, asset, sha256) for texlab 5.26.0 from github.com/latex-lsp/texlab
const TEXLAB: &[(&str, &str, &str)] = &[
    (
        "x86_64-windows",
        "texlab-x86_64-windows.zip",
        "cb028d44c3d2b85d36a2ed52d41a0ff43a341b1f04c500c56c4524c4eb72b316",
    ),
    (
        "aarch64-windows",
        "texlab-aarch64-windows.zip",
        "99b215e9a44169eb8d786c33484b963055e1c3dd40f68e14fcc47ae1c84e92c1",
    ),
    (
        "x86-windows",
        "texlab-i686-windows.zip",
        "581b12e5c0fa3f2d22bc3a44a4310c6d10273cfcdb4bd0174d590d5575ad8700",
    ),
    (
        "x86_64-macos",
        "texlab-x86_64-macos.tar.gz",
        "6091611f756b28e1a57612b130c196df4b0bb6e22dde5cf5d890578513397daf",
    ),
    (
        "aarch64-macos",
        "texlab-aarch64-macos.tar.gz",
        "af7972ffd230711ba04ada9b69cc32ce9111d9196ba69538062872faefdbee56",
    ),
    (
        "x86_64-linux",
        "texlab-x86_64-linux.tar.gz",
        "8697bd5e479d4584b14b7eed5c320c80ec4e1d91ebefbb6801e6bf38e9971300",
    ),
    (
        "aarch64-linux",
        "texlab-aarch64-linux.tar.gz",
        "a85cdfcd22454b8d8550f4b0f0620c45ab51760f302fac7a12bc18a890f70f8c",
    ),
];

pub fn tools_dir() -> PathBuf {
    config::data_dir().join("tools")
}

/// A folder holding a harmless `latexmk`, to put first on texlab's PATH.
///
/// texlab runs `latexmk -dir-report` in a copy of the project's
/// `.latexmkrc` folder to find build directories, and latexmk runs that rc
/// file, which is Perl anyone on the project can write. The stand-in does
/// nothing: on Windows it's Canopy itself under the name latexmk.exe (main.rs
/// exits at once when started that way), elsewhere a two-line shell script.
pub fn latexmk_stub() -> Result<PathBuf, String> {
    let dir = tools_dir().join("stub");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        let target = dir.join("latexmk.exe");
        let same = |a: &std::path::Path, b: &std::path::Path| {
            std::fs::metadata(a).ok().map(|m| m.len()) == std::fs::metadata(b).ok().map(|m| m.len())
        };
        if !(target.exists() && same(&me, &target)) {
            let _ = std::fs::remove_file(&target);
            // a hard link costs no space; a copy when the data folder is on
            // another drive
            if std::fs::hard_link(&me, &target).is_err() {
                std::fs::copy(&me, &target).map_err(|e| e.to_string())?;
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let target = dir.join("latexmk");
        std::fs::write(&target, "#!/bin/sh\nexit 0\n").map_err(|e| e.to_string())?;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "texlab.exe"
    } else {
        "texlab"
    }
}

// the release asset and hash for this computer, if texlab ships one
fn asset() -> Option<(&'static str, &'static str)> {
    let target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    TEXLAB
        .iter()
        .find(|(t, _, _)| *t == target)
        .map(|(_, a, h)| (*a, *h))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// the one executable in a release archive
fn extract(archive: &[u8], name: &str) -> Result<Vec<u8>, String> {
    if name.ends_with(".zip") {
        let mut zip =
            zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| e.to_string())?;
        let mut f = zip.by_name(exe_name()).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    } else {
        // a gzipped tar holding just the binary: walk the 512-byte headers
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(archive)
            .read_to_end(&mut raw)
            .map_err(|e| e.to_string())?;
        let mut at = 0;
        while at + 512 <= raw.len() {
            let header = &raw[at..at + 512];
            if header.iter().all(|b| *b == 0) {
                break;
            }
            let file = String::from_utf8_lossy(&header[..100])
                .trim_end_matches('\0')
                .to_string();
            let size_field = String::from_utf8_lossy(&header[124..136])
                .trim_matches(|c: char| c == '\0' || c == ' ')
                .to_string();
            let size = usize::from_str_radix(&size_field, 8).map_err(|e| e.to_string())?;
            let body = at + 512;
            // a regular file only ("0", or NUL in old archives), never a link
            let regular = matches!(header[156], b'0' | 0);
            if regular && file.rsplit('/').next() == Some(exe_name()) && body + size <= raw.len() {
                return Ok(raw[body..body + size].to_vec());
            }
            at = body + size.div_ceil(512) * 512;
        }
        Err(format!("{} isn't in the download", exe_name()))
    }
}

/// Downloads texlab into the tools folder, checking it against the pinned
/// hash. Returns where it went.
pub async fn install_texlab() -> Result<PathBuf, String> {
    let (name, want) = asset().ok_or("texlab has no build for this computer")?;
    let url =
        format!("https://github.com/latex-lsp/texlab/releases/download/v{TEXLAB_VERSION}/{name}");
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent(concat!("Canopy/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't download texlab: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("couldn't download texlab: {e}"))?;
    let got = hex(&Sha256::digest(&bytes));
    if got != want {
        return Err(format!(
            "the texlab download doesn't match its checksum ({got}); not installing it"
        ));
    }
    let exe = extract(&bytes, name)?;
    let dir = tools_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(exe_name());
    let tmp = dir.join(format!("{}.download", exe_name()));
    std::fs::write(&tmp, &exe).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    crate::dlog!("installed texlab {TEXLAB_VERSION} to {}", target.display());
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_has_a_build() {
        // every platform canopy ships for has a pinned texlab
        assert!(
            asset().is_some(),
            "no texlab for {}",
            std::env::consts::ARCH
        );
    }

    #[test]
    fn reads_a_tar_with_one_file() {
        // a minimal ustar archive holding "texlab"
        let body = b"#!/bin/sh\necho hi\n";
        let mut header = [0u8; 512];
        header[..6].copy_from_slice(b"texlab");
        let size = format!("{:011o}\0", body.len());
        header[124..136].copy_from_slice(size.as_bytes());
        let mut tar = header.to_vec();
        tar.extend_from_slice(body);
        tar.resize(512 + 512, 0);
        tar.extend_from_slice(&[0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        let gz = gz.finish().unwrap();
        if cfg!(windows) {
            // the Windows build looks for texlab.exe, which this archive lacks
            assert!(extract(&gz, "x.tar.gz").is_err());
        } else {
            assert_eq!(extract(&gz, "x.tar.gz").unwrap(), body);
        }
    }
}
