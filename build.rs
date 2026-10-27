// Embed the app icon (assets/icon.ico) into the .exe so Explorer, the
// taskbar and shortcuts show it.
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("assets/app.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
