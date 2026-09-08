fn main() {
    // Ensure dist/client directory exists so rust-embed can compile even before frontend is built
    let _ = std::fs::create_dir_all("../../dist/client");

    #[cfg(feature = "desktop")]
    tauri_build::build();
}
