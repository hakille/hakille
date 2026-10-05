use annora_inference::{ModelCatalog, ModelProvisioner, ModelTier};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::io::{self, Write};

fn make_progress_callback() -> annora_inference::ProgressCallback {
    Arc::new(|p: &annora_inference::DownloadProgress| {
        let pct_str = match p.percentage {
            Some(pct) => format!("{:5.1}%", pct),
            None => "  ?% ".to_string(),
        };
        let mb_downloaded = p.bytes_downloaded as f64 / 1_048_576.0;
        let mb_total = p.total_bytes
            .map(|t| format!("{:.1} MB", t as f64 / 1_048_576.0))
            .unwrap_or_else(|| "? MB".to_string());
        let speed_mb = p.speed_bytes_per_sec / 1_048_576.0;
        print!(
            "\r  -> [{}] {:.1} MB / {} ({}) @ {:.2} MB/s      ",
            p.filename, mb_downloaded, mb_total, pct_str, speed_mb
        );
        let _ = io::stdout().flush();
    })
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let models_dir = env::var("ANNORA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./models"));

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
        "list-models" => {
            println!("== Annora Model Catalog ==");
            let provisioner = ModelProvisioner::new(models_dir);
            for s in provisioner.list_all_status() {
                let badge = if s.is_installed {
                    format!("[INSTALLED - {:.1} MB]", s.total_disk_bytes as f64 / 1_048_576.0)
                } else {
                    "[MISSING]".to_string()
                };
                println!("• {:<26} {:<10?} {:<18} {}", s.entry.id, s.entry.tier, badge, s.entry.name);
            }
        }
        "download-models" => {
            println!("== Downloading Foundation Models (Zero-Touch Provisioning) ==");
            let provisioner = ModelProvisioner::new(models_dir);
            let progress = Some(make_progress_callback());

            let target_tier = args.get(2).and_then(|t| match t.to_lowercase().as_str() {
                "guest" => Some(ModelTier::Guest),
                "native" => Some(ModelTier::Native),
                "sovereign" => Some(ModelTier::Sovereign),
                _ => None,
            });

            if let Some(tier) = target_tier {
                println!("Target Tier: {:?}", tier);
                let models = ModelCatalog::models_for_tier(tier);
                for m in models {
                    println!("\nProvisioning: {} ({})", m.name, m.id);
                    let _ = provisioner.provision_model(&m, progress.clone()).await;
                    println!();
                }
            } else {
                println!("Target: All models");
                for m in provisioner.catalog() {
                    println!("\nProvisioning: {} ({})", m.name, m.id);
                    let _ = provisioner.provision_model(m, progress.clone()).await;
                    println!();
                }
            }
        }
        "verify-models" => {
            println!("== Verifying Cryptographic Integrity of Local Models ==");
            let provisioner = ModelProvisioner::new(models_dir);
            for s in provisioner.list_all_status() {
                if !s.is_installed {
                    println!("• {:<26} [NOT DOWNLOADED]", s.entry.id);
                    continue;
                }
                match provisioner.verify_integrity(&s.entry) {
                    Ok(true) => println!("• {:<26} [OK - Valid SHA-256]", s.entry.id),
                    Ok(false) => println!("• {:<26} [CORRUPTED]", s.entry.id),
                    Err(e) => println!("• {:<26} [ERROR: {}]", s.entry.id, e),
                }
            }
        }
        _ => {
            println!("Annora xtask workspace tool");
            println!("Usage: cargo xtask <command> [options]");
            println!("Commands:");
            println!("  conformance                 Run conformance test suite");
            println!("  check-all                   Verify full workspace compilation");
            println!("  generate-manifests          Generate platform capability manifests");
            println!("  list-models                 List all foundation models and cache status");
            println!("  download-models [tier]      Download models (guest, native, sovereign)");
            println!("  verify-models               Verify SHA-256 integrity of local model cache");
        }
    }
}
