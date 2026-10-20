use agent_code::{CodePatchEngine, WorkspaceCodeIndexer};

fn main() {
    println!("Hakille Code Agent initialized for workspace refactoring.");
    let mut indexer = WorkspaceCodeIndexer::new();
    let file = indexer.index_file(
        "src/main.rs",
        "fn main() {\n    println!(\"Hello World\");\n}\n",
    );
    println!(
        "  -> Fichier indexé: {} ({} symboles)",
        file.file_path,
        file.symbols.len()
    );

    let patch = CodePatchEngine::create_patch(
        "src/main.rs",
        "fn main() {\n    println!(\"Hello World\");\n}\n",
        "fn main() {\n    println!(\"Hello Hakille!\");\n}\n",
        "Salutation personnalisée Hakille",
    );
    println!("  -> Patch généré:\n{}", patch.unified_diff);
}
