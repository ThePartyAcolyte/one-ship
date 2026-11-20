fn main() {
    // Identifies the compiled library so the UI can detect a daemon running an older build.
    // Re-runs whenever any source file changes, so every rebuilt binary gets a new id.
    println!("cargo:rerun-if-changed=src");
    let build_epoch_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .expect("system clock is before the Unix epoch");
    println!("cargo:rustc-env=ONESHIP_BUILD_ID={}", build_epoch_seconds);
    tauri_build::build()
}
