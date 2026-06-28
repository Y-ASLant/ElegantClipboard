fn main() {
    let build_time = chrono::Local::now().format("%Y-%m-%d").to_string();
    println!("cargo:rustc-env=BUILD_TIME={build_time}");
    tauri_build::build();
}
