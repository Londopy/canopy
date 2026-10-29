// build.rs
// bakes the build date into the binary for the About panel, then lets tauri
// embed the icon and manifest. pure rust so it works on every CI runner
// (the londopy template shells out to powershell, which linux and macos lack).

fn main() {
    // SOURCE_DATE_EPOCH keeps reproducible builds reproducible
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    println!("cargo:rustc-env=BUILD_DATE={y:04}-{m:02}-{d:02}");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    tauri_build::build()
}

// days since 1970-01-01 -> (year, month, day), Howard Hinnant's algorithm
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}
