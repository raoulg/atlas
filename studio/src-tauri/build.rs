fn main() {
    // the channel Studio is a build of, by which it finds the CLI of its own channel (`settings.rs`)
    println!("cargo:rerun-if-env-changed=ATLAS_CHANNEL");
    println!("cargo:rustc-env=ATLAS_CHANNEL={}", std::env::var("ATLAS_CHANNEL").unwrap_or_default());
    tauri_build::build()
}
