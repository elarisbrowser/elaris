fn main() {
    println!("cargo::rustc-check-cfg=cfg(elaris_ui, values(\"linux\", \"macos\", \"other\"))");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LINUX_UI");
    // The window's interface picks its layout from `elaris_ui`. The `linux-ui` feature forces the Linux layout so
    // macOS can preview it.
    let ui = if std::env::var_os("CARGO_FEATURE_LINUX_UI").is_some() {
        "linux"
    } else {
        match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("linux") => "linux",
            Ok("macos") => "macos",
            _ => "other",
        }
    };
    println!("cargo::rustc-cfg=elaris_ui=\"{ui}\"");
    versions();

    if std::env::var_os("CARGO_FEATURE_LADYBIRD_BRIDGE").is_none() {
        return;
    }

    let library_dir = std::env::var("ELARIS_LADYBIRD_LIB_DIR")
        .expect("ELARIS_LADYBIRD_LIB_DIR is required with ladybird-bridge");
    println!("cargo:rustc-link-search=native={library_dir}");
    println!("cargo:rustc-link-lib=dylib=elaris_ladybird");
    // Development builds load the bridge from where CMake built it.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{library_dir}");
    println!("cargo:rerun-if-env-changed=ELARIS_LADYBIRD_LIB_DIR");
}

/// Records the committed Elaris, Ladybird and Rune revisions for the About settings.
///
/// Ladybird's master has no version number of its own, so Elaris identifies it by the pinned
/// commit and that commit's date. Each value is empty when Git can't tell, such as in a source
/// tarball.
fn versions() {
    let elaris = git(&["rev-parse", "HEAD"]);
    let ladybird = git(&["rev-parse", "HEAD:vendor/ladybird"]);
    let ladybird_date = if ladybird.is_empty() {
        String::new()
    } else {
        git(&[
            "-C",
            "vendor/ladybird",
            "show",
            "-s",
            "--format=%cs",
            &ladybird,
        ])
    };
    println!("cargo:rustc-env=ELARIS_COMMIT={elaris}");
    println!("cargo:rustc-env=ELARIS_LADYBIRD_COMMIT={ladybird}");
    println!("cargo:rustc-env=ELARIS_LADYBIRD_DATE={ladybird_date}");
    println!("cargo:rustc-env=ELARIS_RUNE_COMMIT={}", rune_revision());

    // Re-run when a commit moves HEAD, which is also when the Ladybird pin changes.
    println!("cargo:rerun-if-changed=Cargo.toml");
    for path in ["HEAD", "packed-refs"] {
        let path = git(&["rev-parse", "--git-path", path]);
        if !path.is_empty() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    if !branch.is_empty() {
        println!(
            "cargo:rerun-if-changed={}",
            git(&["rev-parse", "--git-path", &branch])
        );
    }
}

/// The `rev` Cargo.toml pins the `rune` dependency to.
fn rune_revision() -> String {
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap_or_default();
    manifest
        .split("[dependencies.rune]")
        .nth(1)
        .and_then(|table| table.split("\n[").next())
        .and_then(|table| {
            table.lines().find_map(|line| {
                let value = line.trim().strip_prefix("rev")?.trim().strip_prefix('=')?;
                Some(value.trim().trim_matches('"').to_owned())
            })
        })
        .unwrap_or_default()
}

fn git(args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
        .unwrap_or_default()
}
