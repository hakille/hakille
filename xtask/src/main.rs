use hakille_inference::{ModelCatalog, ModelProvisioner, ModelTier};
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

mod tokens;

fn make_progress_callback() -> hakille_inference::ProgressCallback {
    Arc::new(|p: &hakille_inference::DownloadProgress| {
        let pct_str = match p.percentage {
            Some(pct) => format!("{:5.1}%", pct),
            None => "  ?% ".to_string(),
        };
        let mb_downloaded = p.bytes_downloaded as f64 / 1_048_576.0;
        let mb_total = p
            .total_bytes
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

    let models_dir = env::var("HAKILLE_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./models"));

    match command {
        "conformance" => {
            println!(
                "== Running Hakille Conformance Suite (fast gate, slow real-inference tests excluded) =="
            );
            let status = std::process::Command::new("cargo")
                .args(["test", "-p", "conformance"])
                .status()
                .expect("Failed to execute cargo test");
            if !status.success() {
                std::process::exit(1);
            }
        }
        "conformance-slow" => {
            println!(
                "== Running Hakille SLOW gate (real Qwen CPU decode, MiniLM/VAD ort — tens of minutes in debug) =="
            );
            println!("== Fast suite first, then --ignored real-inference tests ==");
            for args in [
                vec!["test", "-p", "conformance"],
                vec!["test", "-p", "conformance", "--", "--ignored"],
                vec!["test", "-p", "hakille-kernel", "--", "--ignored"],
            ] {
                let status = std::process::Command::new("cargo")
                    .args(&args)
                    .status()
                    .expect("Failed to execute cargo test");
                if !status.success() {
                    std::process::exit(1);
                }
            }
        }
        "check-all" => {
            println!("== Checking all Hakille workspace crates ==");
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
            println!("== Hakille Model Catalog ==");
            let provisioner = ModelProvisioner::new(models_dir);
            for s in provisioner.list_all_status() {
                let badge = if s.is_installed {
                    format!(
                        "[INSTALLED - {:.1} MB]",
                        s.total_disk_bytes as f64 / 1_048_576.0
                    )
                } else {
                    "[MISSING]".to_string()
                };
                println!(
                    "• {:<26} {:<10?} {:<18} {}",
                    s.entry.id, s.entry.tier, badge, s.entry.name
                );
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
        "tokens" => {
            println!("== Hakille design tokens: TOML source unique → Rust/Swift/Kotlin/CSS ==");
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
            let export_kind = args.iter().find_map(|a| match a.as_str() {
                "export-swift" => Some("swift"),
                "export-kotlin" => Some("kotlin"),
                "export-css" => Some("css"),
                _ => None,
            });
            if let Some(kind) = export_kind {
                let color =
                    tokens::parse_toml(&root.join("tokens/color.toml")).unwrap_or_else(|e| {
                        eprintln!("✗ {e}");
                        std::process::exit(1);
                    });
                let motion =
                    tokens::parse_toml(&root.join("tokens/motion.toml")).unwrap_or_else(|e| {
                        eprintln!("✗ {e}");
                        std::process::exit(1);
                    });
                let ty = tokens::parse_toml(&root.join("tokens/type.toml")).unwrap_or_else(|e| {
                    eprintln!("✗ {e}");
                    std::process::exit(1);
                });
                let out_dir = root.join("target/tokens");
                std::fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
                    eprintln!("✗ mkdir {} : {e}", out_dir.display());
                    std::process::exit(1);
                });
                let (name, content) = match kind {
                    "swift" => (
                        "HakilleTokens.swift",
                        tokens::emit_swift(&color, &motion, &ty),
                    ),
                    "kotlin" => (
                        "HakilleTokens.kt",
                        tokens::emit_kotlin(&color, &motion, &ty),
                    ),
                    _ => ("hakille-tokens.css", tokens::emit_css(&color, &motion, &ty)),
                };
                let target = out_dir.join(name);
                std::fs::write(&target, content).unwrap_or_else(|e| {
                    eprintln!("✗ écriture {target:?} : {e}");
                    std::process::exit(1);
                });
                println!("✓ {} généré depuis tokens/*.toml.", target.display());
                return;
            }
            let check_only = args.iter().any(|a| a == "--check");
            let jobs = [
                (
                    "tokens/color.toml",
                    tokens::emit_color as fn(&tokens::Doc) -> String,
                ),
                ("tokens/motion.toml", tokens::emit_motion),
                ("tokens/type.toml", tokens::emit_type),
            ];
            let mut body = String::new();
            for (rel, emit) in jobs {
                let doc = tokens::parse_toml(&root.join(rel)).unwrap_or_else(|e| {
                    eprintln!("✗ {e}");
                    std::process::exit(1);
                });
                body.push_str(&emit(&doc));
            }
            let mut out = "//! Constantes générées depuis `tokens/` par `cargo xtask tokens`.\n//! NE JAMAIS ÉDITER À LA MAIN — voir tokens/*.toml (source unique).\n\n".to_string()
                + &body;
            // Même normalisation que `cargo fmt` : le fichier reste stable.
            if let Some(fmted) = tokens::rustfmt_str(&out) {
                out = fmted;
            }
            let target = root.join("crates/hakille-design/src/generated.rs");
            let current = std::fs::read_to_string(&target).unwrap_or_default();
            if check_only {
                if current != out {
                    let n = current
                        .chars()
                        .zip(out.chars())
                        .take_while(|(a, b)| a == b)
                        .count();
                    eprintln!(
                        "✗ generated.rs périmé : lancez `cargo xtask tokens`. (commun: {n} chars, actuel: {} chars, attendu: {} chars)",
                        current.len(),
                        out.len()
                    );
                    std::process::exit(1);
                }
                println!("✓ generated.rs à jour.");
            } else {
                std::fs::write(&target, out).unwrap_or_else(|e| {
                    eprintln!("✗ écriture {target:?} : {e}");
                    std::process::exit(1);
                });
                println!("✓ {} régénéré.", target.display());
            }
        }
        _ => {
            println!("Hakille xtask workspace tool");
            println!("Usage: cargo xtask <command> [options]");
            println!("Commands:");
            println!("  conformance                 Run conformance test suite (fast gate)");
            println!(
                "  conformance-slow            Run conformance + ignored real-inference tests (tens of minutes)"
            );
            println!("  check-all                   Verify full workspace compilation");
            println!("  generate-manifests          Generate platform capability manifests");
            println!("  list-models                 List all foundation models and cache status");
            println!("  download-models [tier]      Download models (guest, native, sovereign)");
            println!("  verify-models               Verify SHA-256 integrity of local model cache");
            println!(
                "  tokens [--check]            Régénérer/vérifier les tokens design (TOML → Rust)"
            );
            println!(
                "  tokens export-swift       Générer target/tokens/HakilleTokens.swift (Swift struct)"
            );
            println!(
                "  tokens export-kotlin      Générer target/tokens/HakilleTokens.kt (Kotlin object)"
            );
            println!(
                "  tokens export-css         Générer target/tokens/hakille-tokens.css (CSS :root)"
            );
        }
    }
}
