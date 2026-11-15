#[cfg(target_os = "macos")]
fn main() {
    let Some(name) = std::env::args().nth(1) else {
        eprintln!("usage: elaris-presentation-broker SERVICE_NAME");
        std::process::exit(2);
    };
    let status = elaris_presentation_ipc::run_service(&name);
    if status != 0 {
        std::process::exit(status);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Photon presentation broker is macOS-only");
    std::process::exit(1);
}
