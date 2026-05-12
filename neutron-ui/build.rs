fn main() {
    // No build-time UI compilation needed — egui is immediate mode
    println!("cargo:rerun-if-changed=ui/main.slint");
}
