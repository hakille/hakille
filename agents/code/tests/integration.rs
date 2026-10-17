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
