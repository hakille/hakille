//! Générateur `cargo xtask tokens` : TOML source unique → Rust généré.
//! Mini-parseur suffisant pour notre format (sections, string/int/array).

use std::collections::BTreeMap;
use std::path::Path;

/// Normalise le source généré avec rustfmt (même résultat que
/// `cargo fmt --all`), pour que `--check` reste stable après un fmt.
pub fn rustfmt_str(_src: &str) -> Option<String> {
    // NOTE : volontairement désactivé — `rustfmt --emit stdout` a déjà
    // écrit un chemin au lieu du source sur cet environnement. L'émetteur
    // produit directement du code stable pour rustfmt (voir `fmt_list`).
    None
}

/// Liste Rust au style rustfmt : une ligne si ≤ 100 cols, sinon un élément
/// par ligne avec virgule traînante (rustfmt ne touche plus au fichier).
pub fn fmt_list(items: &[String]) -> String {
    let single = format!("[{}]", items.join(", "));
    if single.len() <= 84 {
        // 4 espaces d'indentation + `pub const … = ` tiennent sous 100 cols
        // dans tous nos cas courts (vérifié par `tokens --check` + fmt).
        single
    } else {
        let mut s = String::from("[\n");
        for it in items {
            s.push_str(&format!("        {it},\n"));
        }
        s.push_str("    ]");
        s
    }
}

#[derive(Debug, Clone)]
pub enum Val {
    Str(String),
    Int(i64),
    StrList(Vec<String>),
    IntList(Vec<i64>),
    IntPairList(Vec<(i64, i64)>),
}

pub type Doc = BTreeMap<String, BTreeMap<String, Val>>;

fn parse_scalar(s: &str) -> Val {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Val::Str(inner.to_string());
    }
    if let Ok(n) = s.parse::<i64>() {
        return Val::Int(n);
    }
    Val::Str(s.to_string())
}

fn parse_value(s: &str) -> Val {
    let s = s.trim();
    if !s.starts_with('[') {
        return parse_scalar(s);
    }
    // Retire UNE seule paire externe [...] pour préserver les paires
    // internes [[a, b], ...] (sons Karplus-Strong). L'ancien
    // `trim_start_matches('[')` supprimait tous les `[` et `inner` ne
    // commençait plus par `[` → paires parsed comme StrList avec crochets.
    let inner = s
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .map(|r| r.trim())
        .unwrap_or("");
    if inner.is_empty() {
        return Val::StrList(vec![]);
    }
    if inner.contains('[') {
        // [[a, b], ...] : paires d'entiers
        let mut out = vec![];
        for chunk in inner.split("],") {
            let c = chunk.replace(['[', ']'], "");
            let nums: Vec<i64> = c.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if nums.len() == 2 {
                out.push((nums[0], nums[1]));
            }
        }
        return Val::IntPairList(out);
    }
    let parts: Vec<String> = inner
        .split(',')
        .map(|x| x.trim().trim_matches('"').trim_matches('\'').to_string())
        .collect();
    if parts.iter().all(|p| p.parse::<i64>().is_ok()) {
        Val::IntList(parts.iter().map(|p| p.parse().unwrap()).collect())
    } else {
        Val::StrList(parts)
    }
}

pub fn parse_toml(path: &Path) -> Result<Doc, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path:?}: {e}"))?;
    let mut doc: Doc = BTreeMap::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        doc.entry(section.clone())
            .or_default()
            .insert(k.trim().to_string(), parse_value(v));
    }
    Ok(doc)
}

fn rust_ident(s: &str) -> String {
    s.to_uppercase().replace(['.', '-', '/'], "_")
}

pub fn emit_color(doc: &Doc) -> String {
    let mut out = String::from("pub mod color {\n");
    for (section, keys) in doc {
        let base = rust_ident(section);
        for (k, v) in keys {
            if k == "allowed_in" {
                continue;
            }
            if let Val::Str(hex) = v {
                let name = format!("{}_{}", base, rust_ident(k));
                let num = hex.trim_start_matches('#');
                out.push_str(&format!("    pub const {name}: u32 = 0x{num};\n"));
            }
        }
    }
    out.push_str("}\n");
    out
}

pub fn emit_motion(doc: &Doc) -> String {
    let mut out = String::from("pub mod motion {\n");
    for (section, keys) in doc {
        let base = rust_ident(section);
        for (k, v) in keys {
            match v {
                Val::Int(n) => {
                    out.push_str(&format!(
                        "    pub const {}_{}: i64 = {n};\n",
                        base,
                        rust_ident(k)
                    ));
                }
                Val::Str(s) => {
                    out.push_str(&format!(
                        "    pub const {}_{}: &str = \"{s}\";\n",
                        base,
                        rust_ident(k)
                    ));
                }
                Val::IntList(items) => {
                    let strs: Vec<String> = items.iter().map(|n| n.to_string()).collect();
                    let list = fmt_list(&strs);
                    out.push_str(&format!(
                        "    pub const {}_{}: [i64; {}] = {list};\n",
                        base,
                        rust_ident(k),
                        items.len()
                    ));
                }
                Val::IntPairList(pairs) => {
                    let strs: Vec<String> =
                        pairs.iter().map(|(a, b)| format!("({a}, {b})")).collect();
                    let list = fmt_list(&strs);
                    out.push_str(&format!(
                        "    pub const {}_{}: [(i64, i64); {}] = {list};\n",
                        base,
                        rust_ident(k),
                        pairs.len()
                    ));
                }
                Val::StrList(items) => {
                    let strs: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
                    let list = fmt_list(&strs);
                    out.push_str(&format!(
                        "    pub const {}_{}: [&str; {}] = {list};\n",
                        base,
                        rust_ident(k),
                        items.len()
                    ));
                }
            }
        }
    }
    out.push_str("}\n");
    out
}

pub fn emit_type(doc: &Doc) -> String {
    let mut out = String::from("pub mod ty {\n");
    for (section, keys) in doc {
        let base = rust_ident(section);
        for (k, v) in keys {
            match v {
                Val::Int(n) => {
                    out.push_str(&format!(
                        "    pub const {}_{}: i64 = {n};\n",
                        base,
                        rust_ident(k)
                    ));
                }
                Val::Str(s) => {
                    out.push_str(&format!(
                        "    pub const {}_{}: &str = \"{s}\";\n",
                        base,
                        rust_ident(k)
                    ));
                }
                Val::StrList(items) => {
                    let strs: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
                    let list = fmt_list(&strs);
                    out.push_str(&format!(
                        "    pub const {}_{}: [&str; {}] = {list};\n",
                        base,
                        rust_ident(k),
                        items.len()
                    ));
                }
                Val::IntList(items) => {
                    let strs: Vec<String> = items.iter().map(|n| n.to_string()).collect();
                    let list = fmt_list(&strs);
                    out.push_str(&format!(
                        "    pub const {}_{}: [i64; {}] = {list};\n",
                        base,
                        rust_ident(k),
                        items.len()
                    ));
                }
                Val::IntPairList(_) => {}
            }
        }
    }
    out.push_str("}\n");
    out
}

fn swift_ident(section: &str, key: &str) -> String {
    let s = format!("{section}_{key}")
        .replace(['.', '-', '/'], "_")
        .replace("__", "_");
    let mut out = String::new();
    let mut upper_next = false;
    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else if i == 0 {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn css_var(section: &str, key: &str) -> String {
    format!(
        "--hakille-{}-{}",
        section.replace(['.', '/'], "-"),
        key.replace(['.', '/'], "-")
    )
}

/// Export Swift depuis les MÊMES TOML (braise/surface inclus).
pub fn emit_swift(color: &Doc, motion: &Doc, ty: &Doc) -> String {
    let mut out = String::from(
        "// Généré par `cargo xtask tokens export-swift` depuis tokens/*.toml. NE PAS ÉDITER.\n// braise + surface : source unique tokens/color.toml.\nimport SwiftUI\n\npublic enum HakilleTokens {\n",
    );
    for (section, keys) in color {
        for (k, v) in keys {
            if k == "allowed_in" {
                continue;
            }
            if let Val::Str(hex) = v {
                let name = swift_ident(section, k);
                out.push_str(&format!("    public static let {name} = \"{hex}\"\n"));
            }
        }
    }
    for (section, keys) in motion {
        for (k, v) in keys {
            let name = swift_ident(section, k);
            match v {
                Val::Int(n) => {
                    out.push_str(&format!("    public static let {name}: Int = {n}\n"));
                }
                // Sons Karplus-Strong : paires (note MIDI, durée ms).
                Val::IntPairList(pairs) => {
                    let list = pairs
                        .iter()
                        .map(|(a, b)| format!("({a}, {b})"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "    public static let {name}: [(Int, Int)] = [{list}]\n"
                    ));
                }
                _ => {}
            }
        }
    }
    for (section, keys) in ty {
        for (k, v) in keys {
            if let Val::Int(n) = v {
                let name = swift_ident(section, k);
                out.push_str(&format!("    public static let {name}: Int = {n}\n"));
            }
        }
    }
    out.push_str("}\n");
    out
}

/// Export Kotlin depuis les MÊMES TOML (braise/surface inclus).
pub fn emit_kotlin(color: &Doc, motion: &Doc, ty: &Doc) -> String {
    let mut out = String::from(
        "// Généré par `cargo xtask tokens export-kotlin` depuis tokens/*.toml. NE PAS ÉDITER.\n// braise + surface : source unique tokens/color.toml.\npackage hakille.tokens\n\nobject HakilleTokens {\n",
    );
    for (section, keys) in color {
        for (k, v) in keys {
            if k == "allowed_in" {
                continue;
            }
            if let Val::Str(hex) = v {
                let name = swift_ident(section, k);
                out.push_str(&format!("    const val {name} = \"{hex}\"\n"));
            }
        }
    }
    for (section, keys) in motion {
        for (k, v) in keys {
            let name = swift_ident(section, k);
            match v {
                Val::Int(n) => {
                    out.push_str(&format!("    const val {name}: Long = {n}L\n"));
                }
                // Sons : List<Pair> (pas const : listOf n'est pas const).
                Val::IntPairList(pairs) => {
                    let list = pairs
                        .iter()
                        .map(|(a, b)| format!("{a}L to {b}L"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "    val {name}: List<Pair<Long, Long>> = listOf({list})\n"
                    ));
                }
                _ => {}
            }
        }
    }
    for (section, keys) in ty {
        for (k, v) in keys {
            if let Val::Int(n) = v {
                let name = swift_ident(section, k);
                out.push_str(&format!("    const val {name}: Long = {n}L\n"));
            }
        }
    }
    out.push_str("}\n");
    out
}

/// Export CSS depuis les MÊMES TOML (braise/surface inclus).
pub fn emit_css(color: &Doc, motion: &Doc, ty: &Doc) -> String {
    let mut out = String::from(
        "/* Généré par `cargo xtask tokens export-css` depuis tokens/*.toml. NE PAS ÉDITER. */\n/* braise + surface : source unique tokens/color.toml. */\n:root {\n",
    );
    for (section, keys) in color {
        for (k, v) in keys {
            if k == "allowed_in" {
                continue;
            }
            if let Val::Str(hex) = v {
                out.push_str(&format!("  {}: {};\n", css_var(section, k), hex));
            }
        }
    }
    for (section, keys) in motion {
        for (k, v) in keys {
            match v {
                Val::Int(n) => {
                    out.push_str(&format!("  {}: {}ms;\n", css_var(section, k), n));
                }
                // Sons : `--hakille-motion-sound-eveil: 69 90ms, 76 90ms;`
                Val::IntPairList(pairs) => {
                    let list = pairs
                        .iter()
                        .map(|(a, b)| format!("{a} {b}ms"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("  {}: {};\n", css_var(section, k), list));
                }
                _ => {}
            }
        }
    }
    for (section, keys) in ty {
        for (k, v) in keys {
            if let Val::Int(n) = v {
                out.push_str(&format!("  {}: {};\n", css_var(section, k), n));
            }
        }
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_docs() -> (Doc, Doc, Doc) {
        let mut color: Doc = BTreeMap::new();
        color
            .entry("color.braise".to_string())
            .or_default()
            .insert("dark".to_string(), Val::Str("#FF7A2F".to_string()));
        color
            .entry("color.surface.0".to_string())
            .or_default()
            .insert("dark".to_string(), Val::Str("#0B0C0F".to_string()));
        let motion: Doc = BTreeMap::new();
        let ty: Doc = BTreeMap::new();
        (color, motion, ty)
    }

    #[test]
    fn test_exports_contain_braise_and_surface() {
        let (color, motion, ty) = sample_docs();
        let swift = emit_swift(&color, &motion, &ty);
        let kotlin = emit_kotlin(&color, &motion, &ty);
        let css = emit_css(&color, &motion, &ty);
        for export in [&swift, &kotlin, &css] {
            assert!(
                export.to_lowercase().contains("braise"),
                "export doit contenir braise : {export}"
            );
            assert!(
                export.to_lowercase().contains("surface"),
                "export doit contenir surface : {export}"
            );
        }
        assert!(swift.contains("HakilleTokens"));
        assert!(kotlin.contains("HakilleTokens"));
        assert!(css.contains(":root"));
        // Valeurs exactes issues des TOML (jamais modifiées ici).
        assert!(swift.contains("#FF7A2F") && swift.contains("#0B0C0F"));
        assert!(kotlin.contains("#FF7A2F") && kotlin.contains("#0B0C0F"));
        assert!(css.contains("#FF7A2F") && css.contains("#0B0C0F"));
    }

    #[test]
    fn test_parse_value_sound_pairs() {
        // Bug sons : `inner` après trim ne commençait plus par `[`
        // (`trim_start_matches('[')` supprimait les deux crochets).
        for (src, expected) in [
            ("[[69, 90], [76, 90]]", vec![(69, 90), (76, 90)]),
            ("[[72, 220]]", vec![(72, 220)]),
            ("[[67, 160], [67, 160]]", vec![(67, 160), (67, 160)]),
        ] {
            match parse_value(src) {
                Val::IntPairList(pairs) => assert_eq!(pairs, expected, "src={src}"),
                other => panic!("src={src} doit être IntPairList, got {other:?}"),
            }
        }
        // Listes simples intactes.
        match parse_value("[4, 8, 12]") {
            Val::IntList(v) => assert_eq!(v, vec![4, 8, 12]),
            other => panic!("IntList attendue, got {other:?}"),
        }
        match parse_value("[\"a\", \"b\"]") {
            Val::StrList(v) => assert_eq!(v, vec!["a".to_string(), "b".to_string()]),
            other => panic!("StrList attendue, got {other:?}"),
        }
    }

    #[test]
    fn test_sound_exports_swift_kotlin_css() {
        let color: Doc = BTreeMap::new();
        let mut motion: Doc = BTreeMap::new();
        motion
            .entry("motion.sound".to_string())
            .or_default()
            .insert(
                "eveil".to_string(),
                Val::IntPairList(vec![(69, 90), (76, 90)]),
            );
        let ty: Doc = BTreeMap::new();
        let swift = emit_swift(&color, &motion, &ty);
        let kotlin = emit_kotlin(&color, &motion, &ty);
        let css = emit_css(&color, &motion, &ty);
        assert!(
            swift.contains("(69, 90)") && swift.contains("(76, 90)"),
            "{swift}"
        );
        assert!(kotlin.contains("69L to 90L"), "{kotlin}");
        assert!(css.contains("69 90ms"), "{css}");
    }
}
