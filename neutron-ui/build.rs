fn main() {
    // Slint UI compilation is handled in the neutron-app crate
    // where the Android SDK environment is properly available.
    println!("cargo:rerun-if-changed=ui/main.slint");
}
