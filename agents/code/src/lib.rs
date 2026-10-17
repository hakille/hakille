use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeSymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Method,
    Constant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSymbol {
    pub name: String,
    pub kind: CodeSymbolKind,
    pub line_number: usize,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFileSummary {
    pub file_path: String,
    pub language: String,
    pub line_count: usize,
    pub symbols: Vec<CodeSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePatch {
    pub file_path: String,
    pub original_content: String,
    pub modified_content: String,
    pub unified_diff: String,
    pub explanation: String,
}

pub struct WorkspaceCodeIndexer {
    indexed_files: Vec<CodeFileSummary>,
}

impl WorkspaceCodeIndexer {
    pub fn new() -> Self {
        Self {
            indexed_files: Vec::new(),
        }
    }

    pub fn index_file(&mut self, path: &str, content: &str) -> &CodeFileSummary {
        let lang = if path.ends_with(".rs") {
            "Rust"
        } else if path.ends_with(".py") {
            "Python"
        } else if path.ends_with(".ts") || path.ends_with(".js") {
            "TypeScript"
        } else if path.ends_with(".toml") {
            "TOML"
        } else {
            "Plaintext"
        };

        let symbols = Self::extract_symbols(content, lang);
        let summary = CodeFileSummary {
            file_path: path.to_string(),
            language: lang.to_string(),
            line_count: content.lines().count(),
            symbols,
        };

        self.indexed_files.retain(|f| f.file_path != path);
        self.indexed_files.push(summary);
        self.indexed_files.last().unwrap()
    }

    pub fn extract_symbols(content: &str, lang: &str) -> Vec<CodeSymbol> {
        let mut symbols = Vec::new();
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let line_num = i + 1;

            if lang == "Rust" {
                if let Some(fn_pos) = trimmed.find("fn ") {
                    let prefix = &trimmed[..fn_pos];
                    let valid_prefix = prefix.split_whitespace().all(|w| {
                        w == "pub"
                            || w.starts_with("pub(")
                            || w == "async"
                            || w == "const"
                            || w == "unsafe"
                            || w == "extern"
                            || w.starts_with('"')
                    });
                    if (prefix.is_empty() || valid_prefix) && trimmed[fn_pos..].contains('(') {
                        let after_fn = &trimmed[fn_pos + 3..];
                        let name = after_fn
                            .split('(')
                            .next()
                            .unwrap_or("")
                            .split('<')
                            .next()
                            .unwrap_or("")
                            .trim();
                        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            let sig = trimmed.split('{').next().unwrap_or(trimmed).trim();
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Function,
                                line_number: line_num,
                                signature: sig.to_string(),
                            });
                        }
                    }
                } else if let Some(struct_pos) = trimmed.find("struct ") {
                    let prefix = &trimmed[..struct_pos];
                    let valid_prefix = prefix
                        .split_whitespace()
                        .all(|w| w == "pub" || w.starts_with("pub("));
                    if prefix.is_empty() || valid_prefix {
                        let after_struct = &trimmed[struct_pos + 7..];
                        let name = after_struct
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .split('<')
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('{')
                            .trim_end_matches(';')
                            .trim();
                        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Struct,
                                line_number: line_num,
                                signature: trimmed
                                    .split('{')
                                    .next()
                                    .unwrap_or(trimmed)
                                    .trim()
                                    .to_string(),
                            });
                        }
                    }
                } else if let Some(enum_pos) = trimmed.find("enum ") {
                    let prefix = &trimmed[..enum_pos];
                    let valid_prefix = prefix
                        .split_whitespace()
                        .all(|w| w == "pub" || w.starts_with("pub("));
                    if prefix.is_empty() || valid_prefix {
                        let after_enum = &trimmed[enum_pos + 5..];
                        let name = after_enum
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .split('<')
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('{')
                            .trim();
                        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Enum,
                                line_number: line_num,
                                signature: trimmed
                                    .split('{')
                                    .next()
                                    .unwrap_or(trimmed)
                                    .trim()
                                    .to_string(),
                            });
                        }
                    }
                } else if let Some(trait_pos) = trimmed.find("trait ") {
                    let prefix = &trimmed[..trait_pos];
                    let valid_prefix = prefix
                        .split_whitespace()
                        .all(|w| w == "pub" || w.starts_with("pub("));
                    if prefix.is_empty() || valid_prefix {
                        let after_trait = &trimmed[trait_pos + 6..];
                        let name = after_trait
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .split('<')
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('{')
                            .trim();
                        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Trait,
                                line_number: line_num,
                                signature: trimmed
                                    .split('{')
                                    .next()
                                    .unwrap_or(trimmed)
                                    .trim()
                                    .to_string(),
                            });
                        }
                    }
                } else if trimmed.starts_with("pub const ") || trimmed.starts_with("const ") {
                    let after_const = trimmed.split("const ").nth(1).unwrap_or("");
                    let name = after_const
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .split('=')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        symbols.push(CodeSymbol {
                            name: name.to_string(),
                            kind: CodeSymbolKind::Constant,
                            line_number: line_num,
                            signature: trimmed
                                .split('=')
                                .next()
                                .unwrap_or(trimmed)
                                .trim()
                                .to_string(),
                        });
                    }
                }
            } else if lang == "Python" {
                if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
                    let name = trimmed
                        .split("def ")
                        .nth(1)
                        .and_then(|s| s.split('(').next())
                        .unwrap_or("unknown")
                        .trim();
                    symbols.push(CodeSymbol {
                        name: name.to_string(),
                        kind: CodeSymbolKind::Function,
                        line_number: line_num,
                        signature: trimmed.trim_end_matches(':').to_string(),
                    });
                } else if trimmed.starts_with("class ") {
                    let name = trimmed
                        .split("class ")
                        .nth(1)
                        .and_then(|s| s.split('(').next())
                        .and_then(|s| s.split(':').next())
                        .unwrap_or("unknown")
                        .trim();
                    symbols.push(CodeSymbol {
                        name: name.to_string(),
                        kind: CodeSymbolKind::Struct,
                        line_number: line_num,
                        signature: trimmed.trim_end_matches(':').to_string(),
                    });
                }
            } else if lang == "TypeScript" || lang == "JavaScript" {
                if let Some(fn_pos) = trimmed.find("function ") {
                    let prefix = &trimmed[..fn_pos];
                    let valid_prefix = prefix
                        .split_whitespace()
                        .all(|w| w == "export" || w == "async" || w == "default");
                    if prefix.is_empty() || valid_prefix {
                        let after_fn = &trimmed[fn_pos + 9..];
                        let name = after_fn
                            .split('(')
                            .next()
                            .unwrap_or("")
                            .split('<')
                            .next()
                            .unwrap_or("")
                            .trim();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Function,
                                line_number: line_num,
                                signature: trimmed
                                    .split('{')
                                    .next()
                                    .unwrap_or(trimmed)
                                    .trim()
                                    .to_string(),
                            });
                        }
                    }
                } else if let Some(class_pos) = trimmed.find("class ") {
                    let prefix = &trimmed[..class_pos];
                    let valid_prefix = prefix
                        .split_whitespace()
                        .all(|w| w == "export" || w == "default" || w == "abstract");
                    if prefix.is_empty() || valid_prefix {
                        let after_class = &trimmed[class_pos + 6..];
                        let name = after_class
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .split('{')
                            .next()
                            .unwrap_or("")
                            .trim();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Struct,
                                line_number: line_num,
                                signature: trimmed
                                    .split('{')
                                    .next()
                                    .unwrap_or(trimmed)
                                    .trim()
                                    .to_string(),
                            });
                        }
                    }
                } else if let Some(iface_pos) = trimmed.find("interface ") {
                    let prefix = &trimmed[..iface_pos];
                    let valid_prefix = prefix.split_whitespace().all(|w| w == "export");
                    if prefix.is_empty() || valid_prefix {
                        let after_iface = &trimmed[iface_pos + 10..];
                        let name = after_iface
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .split('{')
                            .next()
                            .unwrap_or("")
                            .trim();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol {
                                name: name.to_string(),
                                kind: CodeSymbolKind::Trait,
                                line_number: line_num,
                                signature: trimmed
                                    .split('{')
                                    .next()
                                    .unwrap_or(trimmed)
                                    .trim()
                                    .to_string(),
                            });
                        }
                    }
                }
            }
        }
        symbols
    }

    pub fn search_symbols(&self, query: &str) -> Vec<(&CodeFileSummary, &CodeSymbol)> {
        let q_lower = query.to_lowercase();
        let mut results = Vec::new();
        for file in &self.indexed_files {
            for sym in &file.symbols {
                if sym.name.to_lowercase().contains(&q_lower) {
                    results.push((file, sym));
                }
            }
        }
        results
    }

    pub fn indexed_files(&self) -> &[CodeFileSummary] {
        &self.indexed_files
    }
}

impl Default for WorkspaceCodeIndexer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CodePatchEngine;

impl CodePatchEngine {
    pub fn create_patch(
        file_path: &str,
        original: &str,
        modified: &str,
        explanation: &str,
    ) -> CodePatch {
        let mut diff_lines = Vec::new();
        diff_lines.push(format!("--- a/{}", file_path));
        diff_lines.push(format!("+++ b/{}", file_path));

        let orig_lines: Vec<&str> = original.lines().collect();
        let mod_lines: Vec<&str> = modified.lines().collect();

        diff_lines.push(format!(
            "@@ -1,{} +1,{} @@",
            orig_lines.len(),
            mod_lines.len()
        ));

        let mut i = 0;
        let mut j = 0;
        while i < orig_lines.len() || j < mod_lines.len() {
            if i < orig_lines.len() && j < mod_lines.len() && orig_lines[i] == mod_lines[j] {
                diff_lines.push(format!(" {}", orig_lines[i]));
                i += 1;
                j += 1;
            } else {
                if i < orig_lines.len() {
                    diff_lines.push(format!("-{}", orig_lines[i]));
                    i += 1;
                }
                if j < mod_lines.len() {
                    diff_lines.push(format!("+{}", mod_lines[j]));
                    j += 1;
                }
            }
        }

        CodePatch {
            file_path: file_path.to_string(),
            original_content: original.to_string(),
            modified_content: modified.to_string(),
            unified_diff: diff_lines.join("\n"),
            explanation: explanation.to_string(),
        }
    }

    pub fn apply_patch(patch: &CodePatch, current_content: &str) -> Result<String, String> {
        if current_content != patch.original_content {
            return Err(
                "Erreur d'application du patch : Le contenu original a divergé (conflit de merge)."
                    .to_string(),
            );
        }
        Ok(patch.modified_content.clone())
    }
}

pub fn check_code_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Filesystem, CapabilityAction::Read),
        (CapabilityScope::Filesystem, CapabilityAction::Write),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_extraction_rust_and_python() {
        let rust_code = r#"
pub struct EngineConfig {
    pub threads: usize,
}

pub enum ExecutionTier {
    Native,
    Sovereign,
}

pub fn bootstrap_runtime() -> bool {
    true
}
"#;
        let mut indexer = WorkspaceCodeIndexer::new();
        let summary = indexer.index_file("src/runtime.rs", rust_code);
        assert_eq!(summary.symbols.len(), 3);
        assert_eq!(summary.symbols[0].name, "EngineConfig");
        assert_eq!(summary.symbols[0].kind, CodeSymbolKind::Struct);
        assert_eq!(summary.symbols[1].name, "ExecutionTier");
        assert_eq!(summary.symbols[1].kind, CodeSymbolKind::Enum);
        assert_eq!(summary.symbols[2].name, "bootstrap_runtime");
        assert_eq!(summary.symbols[2].kind, CodeSymbolKind::Function);

        let search_results = indexer.search_symbols("Engine");
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].1.name, "EngineConfig");
    }

    #[test]
    fn test_patch_creation_and_application() {
        let orig = "fn main() {\n    println!(\"Hello\");\n}\n";
        let modif = "fn main() {\n    println!(\"Hello, Annora!\");\n}\n";

        let patch =
            CodePatchEngine::create_patch("src/main.rs", orig, modif, "Salutation enrichie");
        assert!(patch.unified_diff.contains("--- a/src/main.rs"));

        // Applying to matching content succeeds
        let applied = CodePatchEngine::apply_patch(&patch, orig).unwrap();
        assert_eq!(applied, modif);

        // Applying to diverging content fails
        let conflict = "fn main() {\n    println!(\"Diverged\");\n}\n";
        assert!(CodePatchEngine::apply_patch(&patch, conflict).is_err());
    }

    #[test]
    fn test_qualified_rust_and_typescript_symbol_extraction() {
        let ts_code = r#"
export class NeuralBridge {
    constructor() {}
}

export interface CognitiveChannel {
    id: string;
}

export async function dispatchInference(prompt: string) {
    return true;
}
"#;
        let mut indexer = WorkspaceCodeIndexer::new();
        let summary_ts = indexer.index_file("src/bridge.ts", ts_code);
        assert_eq!(summary_ts.language, "TypeScript");
        assert_eq!(summary_ts.symbols.len(), 3);
        assert_eq!(summary_ts.symbols[0].name, "NeuralBridge");
        assert_eq!(summary_ts.symbols[0].kind, CodeSymbolKind::Struct);
        assert_eq!(summary_ts.symbols[1].name, "CognitiveChannel");
        assert_eq!(summary_ts.symbols[1].kind, CodeSymbolKind::Trait);
        assert_eq!(summary_ts.symbols[2].name, "dispatchInference");
        assert_eq!(summary_ts.symbols[2].kind, CodeSymbolKind::Function);

        let rust_code = r#"
pub(crate) async fn async_eval() -> bool {
    true
}

pub const MAX_TOKENS: usize = 4096;

pub(crate) struct TokenBuffer {
    data: Vec<u8>,
}
"#;
        let summary_rs = indexer.index_file("src/eval.rs", rust_code);
        assert_eq!(summary_rs.symbols.len(), 3);
        assert_eq!(summary_rs.symbols[0].name, "async_eval");
        assert_eq!(summary_rs.symbols[0].kind, CodeSymbolKind::Function);
        assert_eq!(summary_rs.symbols[1].name, "MAX_TOKENS");
        assert_eq!(summary_rs.symbols[1].kind, CodeSymbolKind::Constant);
        assert_eq!(summary_rs.symbols[2].name, "TokenBuffer");
        assert_eq!(summary_rs.symbols[2].kind, CodeSymbolKind::Struct);
    }
}
