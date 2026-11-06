use agent_code::{CodePatchEngine, WorkspaceCodeIndexer};

/// Intégration code : indexation multi-fichiers + recherche + patch/apply.
#[test]
fn test_code_integration_index_and_patch() {
    let mut indexer = WorkspaceCodeIndexer::new();
    let s1 = indexer.index_file(
        "src/mail.rs",
        "pub struct MailClient {\n}\n\npub fn fetch_folder() {}\n",
    );
    assert!(s1.symbols.iter().any(|s| s.name == "MailClient"));
    indexer.index_file(
        "src/calendar.rs",
        "pub struct CalendarManager {\n}\n\npub fn add_event() {}\n",
    );
    assert_eq!(indexer.indexed_files().len(), 2);

    // Recherche transverse.
    let hits = indexer.search_symbols("Manager");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].1.name, "CalendarManager");

    // Patch : création + application OK + conflit sur contenu divergent.
    let orig = "fn run() -> i32 {\n    0\n}\n";
    let modified = "fn run() -> i32 {\n    1\n}\n";
    let patch = CodePatchEngine::create_patch("src/agent.rs", orig, modified, "Retourne 1");
    assert!(patch.unified_diff.contains("--- a/src/agent.rs"));
    assert!(patch.unified_diff.contains("+    1"));
    assert_eq!(
        CodePatchEngine::apply_patch(&patch, orig).unwrap(),
        modified
    );
    assert!(CodePatchEngine::apply_patch(&patch, "divergent").is_err());
}

/// Les patches exportés sont consommables par Git sans perdre les octets.
#[test]
fn exported_patches_apply_with_git_and_preserve_line_endings() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let root = std::env::temp_dir().join(format!(
        "hakille-code-patches-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .expect("Git requis pour vérifier le format de patch")
            .success()
    );
    for (name, before, after) in [
        ("file.rs", "abc", "abc\n"),
        ("file.rs", "abc\n", "abc"),
        ("file.rs", "a\r\nb\r\n", "a\nb\n"),
        ("file.rs", "a\nb\n", "a\r\nb\r\n"),
        ("file.rs", "", "nouveau\n"),
        ("file.rs", "ancien\n", ""),
        ("fichier avec espace.txt", "漢字\n", "été\nfin"),
        ("file.rs", "alpha\nbeta\ngamma\n", "avant\nalpha\ngamma\n"),
    ] {
        std::fs::write(root.join(name), before).unwrap();
        let patch = CodePatchEngine::create_patch(name, before, after, "test exact");
        // Vérifier les octets du patch, sans conversion de fins de ligne
        // imposée par la configuration Git globale (autocrlf sur Windows).
        let mut child = Command::new("git")
            .args([
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.eol=lf",
                "apply",
                "--whitespace=nowarn",
                "-",
            ])
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(patch.unified_diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            patch.unified_diff
        );
        assert_eq!(std::fs::read(root.join(name)).unwrap(), after.as_bytes());
    }
}

#[test]
fn no_op_patch_is_empty_and_path_cannot_inject_headers() {
    assert!(
        CodePatchEngine::create_patch("x", "identique", "identique", "")
            .unified_diff
            .is_empty()
    );
    let patch = CodePatchEngine::create_patch("a\n+++ b/victime", "x", "y", "");
    assert!(
        patch
            .unified_diff
            .starts_with("--- \"a/a\\012+++ b/victime\"\n")
    );
    assert_eq!(
        patch
            .unified_diff
            .lines()
            .filter(|line| line.starts_with("+++ "))
            .count(),
        1
    );
}
