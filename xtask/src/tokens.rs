//! Générateur `cargo xtask tokens` : TOML source unique → Rust généré.
//! Mini-parseur suffisant pour notre format (sections, string/int/array).

use std::collections::BTreeMap;
use std::path::Path;

/// Vérifie les exports réellement embarqués dans les applications natives.
/// Le même contrôle que le desktop doit détecter une dérive mobile en CI.
pub fn check_mobile_exports(root: &Path) -> Result<(), String> {
    let color = parse_toml(&root.join("tokens/color.toml"))?;
    let motion = parse_toml(&root.join("tokens/motion.toml"))?;
    let ty = parse_toml(&root.join("tokens/type.toml"))?;
    for (relative, expected, command) in [
        (
            "mobile/android/app/src/main/java/hakille/tokens/HakilleTokens.kt",
            emit_kotlin(&color, &motion, &ty),
            "export-kotlin",
        ),
        (
            "mobile/ios/Hakille/HakilleTokens.swift",
            emit_swift(&color, &motion, &ty),
            "export-swift",
        ),
    ] {
        let current = std::fs::read_to_string(root.join(relative))
            .map_err(|error| format!("lecture de {relative} impossible : {error}"))?;
        if current.replace("\r\n", "\n") != expected {
            return Err(format!(
                "{relative} périmé : exécuter `cargo xtask tokens {command}`, puis copier l'export comme indiqué dans mobile/README.md"
            ));
        }
    }
    Ok(())
}

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

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Str(String),
    Int(i64),
    Float(f64),
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
    if let Ok(f) = s.parse::<f64>() {
        return Val::Float(f);
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
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.1}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!(
                        "    pub const {}_{}: f64 = {s};\n",
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
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.1}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!(
                        "    pub const {}_{}: f64 = {s};\n",
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
        section.replace(['.', '_', '/'], "-"),
        key.replace(['.', '_', '/'], "-")
    )
}

fn parse_cubic_bezier(s: &str) -> Option<(f64, f64, f64, f64)> {
    let inner = s.trim().strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
    let parts: Vec<f64> = inner
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if parts.len() == 4 {
        Some((parts[0], parts[1], parts[2], parts[3]))
    } else {
        None
    }
}

/// Export Swift depuis les MÊMES TOML (braise/surface inclus).
pub fn emit_swift(color: &Doc, motion: &Doc, ty: &Doc) -> String {
    let mut out = String::from(
        "// Généré par `cargo xtask tokens export-swift` depuis tokens/*.toml. NE PAS ÉDITER.\n\
         // braise + surface : source unique tokens/color.toml.\n\
         import SwiftUI\n\n\
         #if canImport(SwiftUI)\n\
         extension Color {\n\
             public init(hex: String) {\n\
                 let clean = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)\n\
                 var int: UInt64 = 0\n\
                 Scanner(string: clean).scanHexInt64(&int)\n\
                 let a, r, g, b: UInt64\n\
                 switch clean.count {\n\
                 case 6: // RGB (24-bit)\n\
                     (a, r, g, b) = (255, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)\n\
                 case 8: // RGBA (32-bit)\n\
                     (r, g, b, a) = ((int >> 24) & 0xFF, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)\n\
                 default:\n\
                     (a, r, g, b) = (255, 0, 0, 0)\n\
                 }\n\
                 self.init(\n\
                     .sRGB,\n\
                     red: Double(r) / 255.0,\n\
                     green: Double(g) / 255.0,\n\
                     blue: Double(b) / 255.0,\n\
                     opacity: Double(a) / 255.0\n\
                 )\n\
             }\n\n\
             public init(rgb: UInt32) {\n\
                 self.init(\n\
                     .sRGB,\n\
                     red: Double((rgb >> 16) & 0xFF) / 255.0,\n\
                     green: Double((rgb >> 8) & 0xFF) / 255.0,\n\
                     blue: Double(rgb & 0xFF) / 255.0,\n\
                     opacity: 1.0\n\
                 )\n\
             }\n\
         }\n\n\
         public struct CubicBezier: Sendable, Equatable {\n\
             public let x1: Double\n\
             public let y1: Double\n\
             public let x2: Double\n\
             public let y2: Double\n\
             public let spec: String\n\n\
             public init(_ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double, spec: String) {\n\
                 self.x1 = x1\n\
                 self.y1 = y1\n\
                 self.x2 = x2\n\
                 self.y2 = y2\n\
                 self.spec = spec\n\
             }\n\n\
             public func animation(duration: Double) -> Animation {\n\
                 Animation.timingCurve(x1, y1, x2, y2, duration: duration)\n\
             }\n\n\
             public var animation: Animation {\n\
                 Animation.timingCurve(x1, y1, x2, y2)\n\
             }\n\
         }\n\
         #endif\n\n\
         public enum HakilleTokens {\n",
    );
    for (section, keys) in color {
        for (k, v) in keys {
            if k == "allowed_in" {
                continue;
            }
            if let Val::Str(hex) = v {
                let name = swift_ident(section, k);
                let num = hex.trim_start_matches('#');
                out.push_str(&format!(
                    "    public static let {name} = Color(hex: \"{hex}\")\n"
                ));
                out.push_str(&format!(
                    "    public static let {name}Hex: String = \"{hex}\"\n"
                ));
                out.push_str(&format!(
                    "    public static let {name}Rgb: UInt32 = 0x{num}\n"
                ));
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
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.1}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!("    public static let {name}: Double = {s}\n"));
                }
                Val::Str(s) => {
                    if let Some((x1, y1, x2, y2)) = parse_cubic_bezier(s) {
                        let fmt_f = |n: f64| {
                            if n.fract() == 0.0 {
                                format!("{n:.1}")
                            } else {
                                format!("{n}")
                            }
                        };
                        out.push_str(&format!(
                            "    public static let {name} = CubicBezier({}, {}, {}, {}, spec: \"{s}\")\n",
                            fmt_f(x1), fmt_f(y1), fmt_f(x2), fmt_f(y2)
                        ));
                        out.push_str(&format!(
                            "    public static let {name}Spec: String = \"{s}\"\n"
                        ));
                        out.push_str(&format!(
                            "    public static let {name}Animation: Animation = Animation.timingCurve({}, {}, {}, {})\n",
                            fmt_f(x1), fmt_f(y1), fmt_f(x2), fmt_f(y2)
                        ));
                    } else {
                        out.push_str(&format!("    public static let {name}: String = \"{s}\"\n"));
                    }
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
                Val::IntList(items) => {
                    let list = items
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    public static let {name}: [Int] = [{list}]\n"));
                }
                Val::StrList(items) => {
                    let list = items
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "    public static let {name}: [String] = [{list}]\n"
                    ));
                }
            }
        }
    }
    for (section, keys) in ty {
        for (k, v) in keys {
            let name = swift_ident(section, k);
            match v {
                Val::Int(n) => {
                    out.push_str(&format!("    public static let {name}: Int = {n}\n"));
                }
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.1}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!("    public static let {name}: Double = {s}\n"));
                }
                Val::Str(s) => {
                    out.push_str(&format!("    public static let {name}: String = \"{s}\"\n"));
                }
                Val::StrList(items) => {
                    let list = items
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "    public static let {name}: [String] = [{list}]\n"
                    ));
                }
                Val::IntList(items) => {
                    let list = items
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    public static let {name}: [Int] = [{list}]\n"));
                }
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
            }
        }
    }
    out.push_str("}\n");
    out
}

/// Export Kotlin depuis les MÊMES TOML (braise/surface inclus).
pub fn emit_kotlin(color: &Doc, motion: &Doc, ty: &Doc) -> String {
    let mut out = String::from(
        "// Généré par `cargo xtask tokens export-kotlin` depuis tokens/*.toml. NE PAS ÉDITER.\n\
         // braise + surface : source unique tokens/color.toml.\n\
         package hakille.tokens\n\n\
         import androidx.compose.animation.core.CubicBezierEasing\n\
         import androidx.compose.ui.graphics.Color\n\n\
         object HakilleTokens {\n",
    );
    for (section, keys) in color {
        for (k, v) in keys {
            if k == "allowed_in" {
                continue;
            }
            if let Val::Str(hex) = v {
                let name = swift_ident(section, k);
                let raw = hex.trim_start_matches('#');
                let color_arg = if raw.len() == 6 {
                    format!("0xFF{raw}L")
                } else if raw.len() == 8 {
                    let alpha = &raw[6..8];
                    let rgb = &raw[0..6];
                    format!("0x{alpha}{rgb}L")
                } else {
                    format!("0xFF{raw}L")
                };
                out.push_str(&format!("    val {name}: Color = Color({color_arg})\n"));
                out.push_str(&format!("    const val {name}Hex: String = \"{hex}\"\n"));
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
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.1}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!("    const val {name}: Double = {s}\n"));
                }
                Val::Str(s) => {
                    if let Some((x1, y1, x2, y2)) = parse_cubic_bezier(s) {
                        let fmt_f = |n: f64| {
                            if n.fract() == 0.0 {
                                format!("{n:.1}f")
                            } else {
                                format!("{n}f")
                            }
                        };
                        out.push_str(&format!(
                            "    val {name}: CubicBezierEasing = CubicBezierEasing({}, {}, {}, {})\n",
                            fmt_f(x1), fmt_f(y1), fmt_f(x2), fmt_f(y2)
                        ));
                        out.push_str(&format!("    const val {name}Spec: String = \"{s}\"\n"));
                    } else {
                        out.push_str(&format!("    const val {name}: String = \"{s}\"\n"));
                    }
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
                Val::IntList(items) => {
                    let list = items
                        .iter()
                        .map(|n| format!("{n}L"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    val {name}: List<Long> = listOf({list})\n"));
                }
                Val::StrList(items) => {
                    let list = items
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    val {name}: List<String> = listOf({list})\n"));
                }
            }
        }
    }
    for (section, keys) in ty {
        for (k, v) in keys {
            let name = swift_ident(section, k);
            match v {
                Val::Int(n) => {
                    out.push_str(&format!("    const val {name}: Long = {n}L\n"));
                }
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.1}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!("    const val {name}: Double = {s}\n"));
                }
                Val::Str(s) => {
                    out.push_str(&format!("    const val {name}: String = \"{s}\"\n"));
                }
                Val::StrList(items) => {
                    let list = items
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    val {name}: List<String> = listOf({list})\n"));
                }
                Val::IntList(items) => {
                    let list = items
                        .iter()
                        .map(|n| format!("{n}L"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("    val {name}: List<Long> = listOf({list})\n"));
                }
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
            }
        }
    }
    out.push_str("}\n");
    out
}

/// Export CSS depuis les MÊMES TOML (braise/surface inclus).
pub fn emit_css(color: &Doc, motion: &Doc, ty: &Doc) -> String {
    let mut out = String::from(
        "/* Généré par `cargo xtask tokens export-css` depuis tokens/*.toml. NE PAS ÉDITER. */\n\
         /* braise + surface : source unique tokens/color.toml. */\n\
         :root {\n",
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
            let var = css_var(section, k);
            match v {
                Val::Int(n) => {
                    let unit = if k == "stiffness"
                        || k == "damping"
                        || k == "mass"
                        || k == "count_full"
                        || k == "count_compact"
                    {
                        ""
                    } else if k.ends_with("_ms")
                        || section.contains("ms")
                        || section.contains("duration")
                        || section.contains("haptic")
                    {
                        "ms"
                    } else if k.ends_with("_px") {
                        "px"
                    } else {
                        ""
                    };
                    out.push_str(&format!("  {var}: {n}{unit};\n"));
                }
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.0}")
                    } else {
                        format!("{f}")
                    };
                    let unit = if k.ends_with("_px") {
                        "px"
                    } else if k.ends_with("_ms") {
                        "ms"
                    } else {
                        ""
                    };
                    out.push_str(&format!("  {var}: {s}{unit};\n"));
                }
                Val::Str(s) => {
                    out.push_str(&format!("  {var}: {s};\n"));
                }
                // Sons : `--hakille-motion-sound-eveil: 69 90ms, 76 90ms;`
                Val::IntPairList(pairs) => {
                    let list = pairs
                        .iter()
                        .map(|(a, b)| format!("{a} {b}ms"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("  {var}: {list};\n"));
                }
                Val::IntList(items) => {
                    let list = items
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push_str(&format!("  {var}: {list};\n"));
                }
                Val::StrList(items) => {
                    let list = items
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("  {var}: {list};\n"));
                }
            }
        }
    }
    for (section, keys) in ty {
        for (k, v) in keys {
            let var = css_var(section, k);
            match v {
                Val::Int(n) => {
                    let unit = if k == "weight" { "" } else { "px" };
                    out.push_str(&format!("  {var}: {n}{unit};\n"));
                }
                Val::Float(f) => {
                    let s = if f.fract() == 0.0 {
                        format!("{f:.0}")
                    } else {
                        format!("{f}")
                    };
                    out.push_str(&format!("  {var}: {s}px;\n"));
                }
                Val::Str(s) => {
                    let val_str = if section.starts_with("font") {
                        format!("\"{s}\"")
                    } else {
                        s.clone()
                    };
                    out.push_str(&format!("  {var}: {val_str};\n"));
                }
                Val::StrList(items) => {
                    let list = items
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("  {var}: {list};\n"));
                }
                Val::IntList(items) => {
                    let list = items
                        .iter()
                        .map(|n| format!("{n}px"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push_str(&format!("  {var}: {list};\n"));
                }
                Val::IntPairList(pairs) => {
                    let list = pairs
                        .iter()
                        .map(|(a, b)| format!("{a} {b}ms"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("  {var}: {list};\n"));
                }
            }
        }
    }
    if let (Some(Val::Str(fam)), Some(Val::StrList(chain))) = (
        ty.get("font.interface").and_then(|k| k.get("family")),
        ty.get("font.fallback").and_then(|k| k.get("chain")),
    ) {
        let mut stack = vec![format!("\"{fam}\"")];
        for c in chain {
            stack.push(format!("\"{c}\""));
        }
        stack.push("sans-serif".to_string());
        out.push_str(&format!(
            "  --hakille-font-interface-stack: {};\n",
            stack.join(", ")
        ));
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
        let mut motion: Doc = BTreeMap::new();
        motion.entry("motion.ease".to_string()).or_default().insert(
            "ease_in".to_string(),
            Val::Str("cubic-bezier(0.2, 0, 0, 1)".to_string()),
        );
        motion
            .entry("motion.spring_card".to_string())
            .or_default()
            .insert("stiffness".to_string(), Val::Int(380));
        let mut ty: Doc = BTreeMap::new();
        ty.entry("font.interface".to_string())
            .or_default()
            .insert("family".to_string(), Val::Str("Inter Variable".to_string()));
        ty.entry("font.fallback".to_string()).or_default().insert(
            "chain".to_string(),
            Val::StrList(vec!["Noto Sans".to_string()]),
        );
        ty.entry("type.display".to_string())
            .or_default()
            .insert("size".to_string(), Val::Int(40));
        ty.entry("type.display".to_string())
            .or_default()
            .insert("weight".to_string(), Val::Int(600));
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

    #[test]
    fn test_parse_scalar_floats_and_ints() {
        assert_eq!(parse_scalar("42"), Val::Int(42));
        assert_eq!(parse_scalar("2.0"), Val::Float(2.0));
        assert_eq!(parse_scalar("\"hello\""), Val::Str("hello".to_string()));
    }

    #[test]
    fn test_multi_target_exporters_curves_fonts_and_units() {
        let (color, motion, ty) = sample_docs();
        let swift = emit_swift(&color, &motion, &ty);
        let kotlin = emit_kotlin(&color, &motion, &ty);
        let css = emit_css(&color, &motion, &ty);

        // Swift assertions
        assert!(
            swift.contains("CubicBezier(0.2, 0.0, 0.0, 1.0, spec: \"cubic-bezier(0.2, 0, 0, 1)\")")
        );
        assert!(
            swift.contains("public static let fontInterfaceFamily: String = \"Inter Variable\"")
        );
        assert!(swift.contains("public static let colorBraiseDark = Color(hex: \"#FF7A2F\")"));
        assert!(swift.contains("public static let colorBraiseDarkRgb: UInt32 = 0xFF7A2F"));

        // Kotlin assertions
        assert!(kotlin.contains("CubicBezierEasing(0.2f, 0.0f, 0.0f, 1.0f)"));
        assert!(kotlin.contains("val colorBraiseDark: Color = Color(0xFFFF7A2FL)"));
        assert!(kotlin.contains("const val fontInterfaceFamily: String = \"Inter Variable\""));

        // CSS assertions
        assert!(css.contains("--hakille-motion-ease-ease-in: cubic-bezier(0.2, 0, 0, 1);"));
        assert!(css.contains("--hakille-motion-spring-card-stiffness: 380;"));
        assert!(css.contains("--hakille-type-display-size: 40px;"));
        assert!(css.contains("--hakille-type-display-weight: 600;"));
        assert!(css.contains("--hakille-font-interface-family: \"Inter Variable\";"));
        assert!(css.contains("--hakille-font-interface-stack:"));
    }
}
