fn main() {
    cxx_build::bridge("src/bridge.rs")
        .flag_if_supported("-std=c++17")
        .compile("mohr_frame_bridge");
    println!("cargo:rerun-if-changed=src/bridge.rs");
}
