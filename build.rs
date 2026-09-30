// Windows builds carry the game's icon as the exe's resource (Explorer,
// shortcuts, taskbar). The window sets its own at run time: src/icon.rs.
//
// Every build carries the commit it was made from, shown in the menu with
// the protocol: which build someone plays tells whether it can meet the
// server. `NECROMY_BUILD` names it where there is no git (a nix build).
fn main() {
    println!("cargo:rerun-if-changed=icon/necromy.rc");
    println!("cargo:rerun-if-changed=icon/necromy.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("icon/necromy.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
    println!("cargo:rerun-if-env-changed=NECROMY_BUILD");
    let build = std::env::var("NECROMY_BUILD").ok().or_else(|| {
        let hash = git(&["rev-parse", "--short", "HEAD"])?;
        let date = git(&["log", "-1", "--format=%cs"])?;
        // A new commit or checkout moves the reflog: build again then.
        if let Some(log) = git(&["rev-parse", "--git-path", "logs/HEAD"]) {
            println!("cargo:rerun-if-changed={log}");
        }
        Some(format!("{hash} от {date}"))
    });
    println!(
        "cargo:rustc-env=NECROMY_BUILD={}",
        build.unwrap_or_else(|| "без git".into())
    );
}

fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}
