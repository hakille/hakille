//! Honest macOS host queries backed by the system tools `sysctl` and
//! `sw_vers`.
//!
//! The `parse_*` helpers are pure and unit-tested on every platform. The
//! command runners are compiled only under `cfg(target_os = "macos")` and
//! report explicit failures (tool absent, non-zero exit, non-UTF-8 stdout).
//! Off macOS every `query_*` fails explicitly — this crate never presents a
//! simulated value as a real host reading.

use serde::{Deserialize, Serialize};

/// (1, 5, 15)-minute load averages as reported by `sysctl vm.loadavg`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

/// Explicit "not available on this OS" error for macOS-only host queries.
pub fn macos_only_error(tool: &str) -> String {
    format!(
        "{tool}: macOS-only host query (this build targets {})",
        std::env::consts::OS
    )
}

/// Extracts the trimmed value of `key` from `sysctl` output.
///
/// Expects the `sysctl <key>` form (`"hw.memsize: 17179869184"`). A missing
/// or empty field is an explicit error, never a default value.
pub fn parse_sysctl_value<'a>(output: &'a str, key: &str) -> Result<&'a str, String> {
    for line in output.lines() {
        let line = line.trim();
        if let Some((field, value)) = line.split_once(':')
            && field.trim() == key
        {
            let value = value.trim();
            if value.is_empty() {
                return Err(format!("sysctl {key}: field present but empty"));
            }
            return Ok(value);
        }
    }
    Err(format!("sysctl {key}: missing field in output"))
}

/// Parses an unsigned integer `sysctl` field (e.g. `hw.memsize`, `hw.ncpu`).
///
/// Trailing tokens (e.g. a unit suffix) are rejected explicitly: `sysctl`
/// reports raw integers, so refusing to guess a unit is the honest option.
pub fn parse_sysctl_u64(output: &str, key: &str) -> Result<u64, String> {
    let raw = parse_sysctl_value(output, key)?;
    let mut tokens = raw.split_whitespace();
    let number = tokens
        .next()
        .ok_or_else(|| format!("sysctl {key}: field present but empty"))?;
    let value = number
        .parse::<u64>()
        .map_err(|_| format!("sysctl {key}: non-numeric value \"{raw}\""))?;
    if let Some(unit) = tokens.next() {
        return Err(format!(
            "sysctl {key}: unexpected unit or trailing token \"{unit}\" in \"{raw}\""
        ));
    }
    Ok(value)
}

/// Parses a textual `sysctl` field (e.g. `hw.model`), trimming whitespace.
pub fn parse_sysctl_string(output: &str, key: &str) -> Result<String, String> {
    parse_sysctl_value(output, key).map(str::to_owned)
}

/// Parses the macOS product version from `sw_vers` output.
///
/// Accepts both the full `sw_vers` report (`"ProductVersion:\t15.1"`) and the
/// bare `sw_vers -productVersion` output (`"15.1"`).
pub fn parse_sw_vers_product_version(output: &str) -> Result<String, String> {
    for line in output.lines() {
        let line = line.trim();
        if let Some((field, value)) = line.split_once(':')
            && field.trim() == "ProductVersion"
        {
            let value = value.trim();
            if value.is_empty() {
                return Err("sw_vers: ProductVersion field present but empty".to_string());
            }
            return Ok(value.to_string());
        }
    }
    // `sw_vers -productVersion` prints the bare version, no field name.
    let mut lines = output.lines().map(str::trim).filter(|l| !l.is_empty());
    if let (Some(only), None) = (lines.next(), lines.next())
        && looks_like_version(only)
    {
        return Ok(only.to_string());
    }
    Err("sw_vers: missing field \"ProductVersion\" in output".to_string())
}

/// A bare product version is numeric with dots (`"15.1"`, `"14.6.1"`).
fn looks_like_version(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.as_bytes()[0].is_ascii_digit()
        && candidate.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

/// Parses `sysctl vm.loadavg` output.
///
/// Accepts `"vm.loadavg: { 1.23 1.45 1.67 }"`, the bare brace form
/// `"{ 1.23 1.45 1.67 }"` and the space-separated form without braces.
/// Exactly three finite, non-negative values are required.
pub fn parse_vm_loadavg(output: &str) -> Result<LoadAverage, String> {
    let raw = output.trim();
    let raw = raw
        .strip_prefix("vm.loadavg:")
        .map(str::trim)
        .unwrap_or(raw);
    let body = if let Some(open) = raw.find('{') {
        let close = raw
            .rfind('}')
            .filter(|close| *close > open)
            .ok_or_else(|| format!("vm.loadavg: unbalanced braces in \"{raw}\""))?;
        if !raw[close + 1..].trim().is_empty() {
            return Err(format!(
                "vm.loadavg: trailing text after braces in \"{raw}\""
            ));
        }
        &raw[open + 1..close]
    } else {
        raw
    };
    let tokens: Vec<&str> = body.split_whitespace().collect();
    if tokens.len() != 3 {
        return Err(format!(
            "vm.loadavg: expected 3 values (1/5/15 min), got {} in \"{raw}\"",
            tokens.len()
        ));
    }
    let mut values = [0.0_f64; 3];
    for (slot, token) in values.iter_mut().zip(&tokens) {
        let parsed = token
            .parse::<f64>()
            .map_err(|_| format!("vm.loadavg: non-numeric value \"{token}\""))?;
        if !parsed.is_finite() || parsed < 0.0 {
            return Err(format!("vm.loadavg: invalid value \"{token}\""));
        }
        *slot = parsed;
    }
    Ok(LoadAverage {
        one: values[0],
        five: values[1],
        fifteen: values[2],
    })
}

/// Runs a host tool with `args` and returns its raw stdout as strict UTF-8.
///
/// Explicit failures: tool absent (`NotFound`), non-zero exit status and
/// non-UTF-8 stdout — a lossy conversion would silently replace bytes.
#[cfg(target_os = "macos")]
pub(crate) fn run_host_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                format!("{program}: command not found on this macOS host")
            }
            _ => format!("{program}: cannot run host command: {err}"),
        })?;
    if !output.status.success() {
        return Err(format!(
            "{program}: host command exited with status {}",
            output.status
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|err| format!("{program}: stdout is not valid UTF-8: {err}"))
}

/// Real total physical RAM (`sysctl hw.memsize`, bytes). macOS only.
#[cfg(target_os = "macos")]
pub fn query_hw_memsize() -> Result<u64, String> {
    parse_sysctl_u64(&run_host_command("sysctl", &["hw.memsize"])?, "hw.memsize")
}

/// Off macOS there is no `sysctl`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_hw_memsize() -> Result<u64, String> {
    Err(macos_only_error("sysctl hw.memsize"))
}

/// Real logical CPU count (`sysctl hw.ncpu`). macOS only.
#[cfg(target_os = "macos")]
pub fn query_hw_ncpu() -> Result<u32, String> {
    let count = parse_sysctl_u64(&run_host_command("sysctl", &["hw.ncpu"])?, "hw.ncpu")?;
    u32::try_from(count).map_err(|_| format!("sysctl hw.ncpu: value {count} exceeds u32 range"))
}

/// Off macOS there is no `sysctl`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_hw_ncpu() -> Result<u32, String> {
    Err(macos_only_error("sysctl hw.ncpu"))
}

/// Real hardware model identifier (`sysctl hw.model`), e.g. `MacBookPro18,3`.
#[cfg(target_os = "macos")]
pub fn query_hw_model() -> Result<String, String> {
    parse_sysctl_string(&run_host_command("sysctl", &["hw.model"])?, "hw.model")
}

/// Off macOS there is no `sysctl`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_hw_model() -> Result<String, String> {
    Err(macos_only_error("sysctl hw.model"))
}

/// Real VM page size (`sysctl hw.pagesize`, bytes). macOS only.
#[cfg(target_os = "macos")]
pub fn query_hw_pagesize() -> Result<u64, String> {
    parse_sysctl_u64(
        &run_host_command("sysctl", &["hw.pagesize"])?,
        "hw.pagesize",
    )
}

/// Off macOS there is no `sysctl`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_hw_pagesize() -> Result<u64, String> {
    Err(macos_only_error("sysctl hw.pagesize"))
}

/// Real macOS product version (`sw_vers -productVersion`), e.g. `15.1`.
#[cfg(target_os = "macos")]
pub fn query_sw_vers_product_version() -> Result<String, String> {
    parse_sw_vers_product_version(&run_host_command("sw_vers", &["-productVersion"])?)
}

/// Off macOS there is no `sw_vers`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_sw_vers_product_version() -> Result<String, String> {
    Err(macos_only_error("sw_vers"))
}

/// Real (1, 5, 15)-minute load averages (`sysctl vm.loadavg`). macOS only.
#[cfg(target_os = "macos")]
pub fn query_host_load_average() -> Result<LoadAverage, String> {
    parse_vm_loadavg(&run_host_command("sysctl", &["vm.loadavg"])?)
}

/// Off macOS there is no `sysctl`: explicit error, never a simulated value.
#[cfg(not(target_os = "macos"))]
pub fn query_host_load_average() -> Result<LoadAverage, String> {
    Err(macos_only_error("sysctl vm.loadavg"))
}

/// OS version string for `PlatformManifest::os_version`.
///
/// macOS: real `sw_vers -productVersion`; on query failure an explicitly
/// labelled `unknown (...)` string is returned (never an invented release).
#[cfg(target_os = "macos")]
pub fn read_macos_os_version() -> String {
    match query_sw_vers_product_version() {
        Ok(version) => version,
        Err(err) => {
            tracing::warn!("sw_vers query failed: {err}");
            format!("unknown (sw_vers failed: {err})")
        }
    }
}

/// Off macOS there is no `sw_vers`: an explicitly labelled placeholder is
/// kept so the manifest stays valid — never a fabricated macOS release.
#[cfg(not(target_os = "macos"))]
pub fn read_macos_os_version() -> String {
    format!(
        "unknown (not queried: sw_vers is macOS-only; this build targets {})",
        std::env::consts::OS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sysctl_u64_typical_hw_memsize() {
        let output = "hw.memsize: 17179869184\n";
        assert_eq!(
            parse_sysctl_u64(output, "hw.memsize").unwrap(),
            17_179_869_184
        );
        assert_eq!(parse_sysctl_u64("hw.ncpu: 10", "hw.ncpu").unwrap(), 10);
        assert_eq!(
            parse_sysctl_u64("  hw.memsize:   34359738368  \n", "hw.memsize").unwrap(),
            34_359_738_368
        );
    }

    #[test]
    fn parse_sysctl_u64_picks_the_requested_key_among_many() {
        let output = "hw.ncpu: 10\nhw.memsize: 17179869184\nhw.pagesize: 16384\n";
        assert_eq!(parse_sysctl_u64(output, "hw.pagesize").unwrap(), 16_384);
        assert_eq!(parse_sysctl_u64(output, "hw.ncpu").unwrap(), 10);
    }

    #[test]
    fn parse_sysctl_u64_rejects_missing_field() {
        let err = parse_sysctl_u64("hw.ncpu: 10\n", "hw.memsize").unwrap_err();
        assert!(err.contains("missing field"), "got: {err}");
        assert!(err.contains("hw.memsize"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_u64_rejects_empty_output() {
        let err = parse_sysctl_u64("", "hw.memsize").unwrap_err();
        assert!(err.contains("missing field"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_u64_rejects_non_numeric() {
        let err = parse_sysctl_u64("hw.memsize: sixteen-gigabytes", "hw.memsize").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
        assert!(err.contains("sixteen-gigabytes"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_u64_rejects_negative_value() {
        let err = parse_sysctl_u64("hw.memsize: -1", "hw.memsize").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_u64_rejects_unit_suffix_instead_of_guessing() {
        let err = parse_sysctl_u64("hw.memsize: 17179869184 B", "hw.memsize").unwrap_err();
        assert!(err.contains("unexpected unit"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_u64_rejects_empty_field() {
        let err = parse_sysctl_u64("hw.memsize:   ", "hw.memsize").unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_string_hw_model() {
        let output = "hw.model: MacBookPro18,3\n";
        assert_eq!(
            parse_sysctl_string(output, "hw.model").unwrap(),
            "MacBookPro18,3"
        );
        assert_eq!(
            parse_sysctl_string("hw.model:   VirtualMac2,1  ", "hw.model").unwrap(),
            "VirtualMac2,1"
        );
    }

    #[test]
    fn parse_sysctl_string_rejects_missing_field() {
        let err = parse_sysctl_string("hw.ncpu: 10", "hw.model").unwrap_err();
        assert!(err.contains("missing field"), "got: {err}");
    }

    #[test]
    fn parse_sw_vers_full_report() {
        let output = "ProductName:\t\tmacOS\nProductVersion:\t\t15.1\nBuildVersion:\t\t24B83\n";
        assert_eq!(parse_sw_vers_product_version(output).unwrap(), "15.1");
    }

    #[test]
    fn parse_sw_vers_bare_product_version() {
        // `sw_vers -productVersion` prints the bare version, no field name.
        assert_eq!(parse_sw_vers_product_version("15.1\n").unwrap(), "15.1");
        assert_eq!(parse_sw_vers_product_version("14.6.1").unwrap(), "14.6.1");
    }

    #[test]
    fn parse_sw_vers_rejects_missing_field() {
        let err = parse_sw_vers_product_version("ProductName:\tmacOS\n").unwrap_err();
        assert!(err.contains("ProductVersion"), "got: {err}");
    }

    #[test]
    fn parse_sw_vers_rejects_empty_field() {
        let err = parse_sw_vers_product_version("ProductVersion:   \n").unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn parse_sw_vers_rejects_non_version_bare_output() {
        let err = parse_sw_vers_product_version("not-a-version\n").unwrap_err();
        assert!(err.contains("ProductVersion"), "got: {err}");
        let err = parse_sw_vers_product_version("15.1 extra\n").unwrap_err();
        assert!(err.contains("ProductVersion"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_typical() {
        let parsed = parse_vm_loadavg("vm.loadavg: { 1.23 1.45 1.67 }\n").unwrap();
        assert_eq!(
            parsed,
            LoadAverage {
                one: 1.23,
                five: 1.45,
                fifteen: 1.67
            }
        );
    }

    #[test]
    fn parse_vm_loadavg_bare_braces() {
        let parsed = parse_vm_loadavg("{ 2.02 1.98 1.76 }").unwrap();
        assert_eq!(parsed.one, 2.02);
        assert_eq!(parsed.five, 1.98);
        assert_eq!(parsed.fifteen, 1.76);
    }

    #[test]
    fn parse_vm_loadavg_without_braces_or_integers() {
        let parsed = parse_vm_loadavg("0 1 2").unwrap();
        assert_eq!(parsed.one, 0.0);
        assert_eq!(parsed.five, 1.0);
        assert_eq!(parsed.fifteen, 2.0);
    }

    #[test]
    fn parse_vm_loadavg_rejects_wrong_count() {
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 2.0 }").unwrap_err();
        assert!(err.contains("expected 3"), "got: {err}");
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 2.0 3.0 4.0 }").unwrap_err();
        assert!(err.contains("expected 3"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_rejects_non_numeric() {
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 abc 3.0 }").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
        assert!(err.contains("abc"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_rejects_non_finite_and_negative() {
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 inf 3.0 }").unwrap_err();
        assert!(err.contains("invalid value"), "got: {err}");
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 NaN 3.0 }").unwrap_err();
        assert!(err.contains("invalid value"), "got: {err}");
        let err = parse_vm_loadavg("vm.loadavg: { -1.0 2.0 3.0 }").unwrap_err();
        assert!(err.contains("invalid value"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_rejects_unbalanced_braces() {
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 2.0 3.0").unwrap_err();
        assert!(err.contains("unbalanced braces"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_rejects_trailing_text() {
        let err = parse_vm_loadavg("vm.loadavg: { 1.0 2.0 3.0 } junk").unwrap_err();
        assert!(err.contains("trailing text"), "got: {err}");
    }

    #[test]
    fn macos_only_error_names_the_tool_and_the_platform() {
        let msg = macos_only_error("sysctl hw.memsize");
        assert!(msg.contains("sysctl hw.memsize"), "got: {msg}");
        assert!(msg.contains("macOS-only"), "got: {msg}");
        assert!(msg.contains(std::env::consts::OS), "got: {msg}");
    }

    #[test]
    fn parse_sysctl_u64_accepts_crlf_fixture() {
        let output = "hw.memsize: 17179869184\r\nhw.ncpu: 10\r\n";
        assert_eq!(
            parse_sysctl_u64(output, "hw.memsize").unwrap(),
            17_179_869_184
        );
        assert_eq!(parse_sysctl_u64(output, "hw.ncpu").unwrap(), 10);
    }

    #[test]
    fn parse_sysctl_u64_rejects_float_and_hex_instead_of_guessing() {
        let err = parse_sysctl_u64("hw.memsize: 17.18e9", "hw.memsize").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
        let err = parse_sysctl_u64("hw.memsize: 0x400000000", "hw.memsize").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
    }

    #[test]
    fn parse_sysctl_string_accepts_crlf_and_rejects_empty() {
        assert_eq!(
            parse_sysctl_string("hw.model: Mac15,3\r\n", "hw.model").unwrap(),
            "Mac15,3"
        );
        let err = parse_sysctl_string("hw.model:\r\n", "hw.model").unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn parse_sw_vers_accepts_crlf_full_report() {
        let output =
            "ProductName:\t\tmacOS\r\nProductVersion:\t\t15.1\r\nBuildVersion:\t\t24B83\r\n";
        assert_eq!(parse_sw_vers_product_version(output).unwrap(), "15.1");
    }

    #[test]
    fn parse_sw_vers_rejects_localized_field_names_instead_of_guessing() {
        // `sw_vers` field names are stable English; a localized report must
        // not be silently mapped to a fabricated version.
        let output = "Nom du produit:\tmacOS\nVersion du produit:\t15.1\n";
        let err = parse_sw_vers_product_version(output).unwrap_err();
        assert!(err.contains("ProductVersion"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_accepts_extra_whitespace_and_crlf() {
        let parsed = parse_vm_loadavg("vm.loadavg:   {   2.02\t1.98   1.76 }\r\n").unwrap();
        assert_eq!(parsed.one, 2.02);
        assert_eq!(parsed.five, 1.98);
        assert_eq!(parsed.fifteen, 1.76);
    }

    #[test]
    fn parse_vm_loadavg_rejects_comma_decimal_separator() {
        // A localized decimal comma must be an explicit error, not a guess.
        let err = parse_vm_loadavg("vm.loadavg: { 1,23 1,45 1,67 }").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
    }

    #[test]
    fn parse_vm_loadavg_rejects_empty_output() {
        let err = parse_vm_loadavg("").unwrap_err();
        assert!(err.contains("expected 3"), "got: {err}");
        let err = parse_vm_loadavg("   \r\n").unwrap_err();
        assert!(err.contains("expected 3"), "got: {err}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn run_host_command_reports_missing_tool() {
        let err = run_host_command("hakille-no-such-tool-7c1e", &[]).unwrap_err();
        assert!(err.contains("command not found"), "got: {err}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn run_host_command_reports_non_zero_exit() {
        let err = run_host_command("false", &[]).unwrap_err();
        assert!(err.contains("exited with status"), "got: {err}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn run_host_command_rejects_non_utf8_stdout() {
        // BSD printf emits the raw byte 0xFF for the octal escape `\377`.
        let err = run_host_command("printf", &["\\377"]).unwrap_err();
        assert!(err.contains("not valid UTF-8"), "got: {err}");
    }
}
