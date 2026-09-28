// Windows builds carry the game's icon as the exe's resource (Explorer,
// shortcuts, taskbar). The window sets its own at run time: src/icon.rs.
fn main() {
    println!("cargo:rerun-if-changed=icon/necromy.rc");
    println!("cargo:rerun-if-changed=icon/necromy.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("icon/necromy.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
