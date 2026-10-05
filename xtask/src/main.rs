use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match command {
        "conformance" => {
            println!("== Running Annora Conformance Suite ==");
            let status = std::process::Command::new("cargo")
                .args(["test", "-p", "conformance"])
                .status()
                .expect("Failed to execute cargo test");
            if !status.success() {
                std::process::exit(1);
            }
        }
        "check-all" => {
            println!("== Checking all Annora workspace crates ==");
            let status = std::process::Command::new("cargo")
                .args(["check", "--workspace"])
                .status()
                .expect("Failed to execute cargo check");
            if !status.success() {
                std::process::exit(1);
            }
        }
        "generate-manifests" => {
            println!("== Generating Tier Manifests ==");
            println!("Tier Sovereign (Redox) -> manifests/sovereign.toml");
            println!("Tier Native (Windows/Linux/macOS) -> manifests/native.toml");
            println!("Tier Guest (Mobile/Web) -> manifests/guest.toml");
        }
        _ => {
            println!("Annora xtask workspace tool");
            println!("Usage: cargo xtask <command>");
            println!("Commands:");
            println!("  conformance         Run conformance test suite");
            println!("  check-all           Verify full workspace compilation");
            println!("  generate-manifests  Generate platform capability manifests");
        }
    }
}
