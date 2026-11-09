//! SemVer 2.0 version resolution and constraint matching.

use semver::{Version, VersionReq};

/// Resolves the highest version from the candidates satisfying an optional VersionReq.
///
/// If `req` is `Some`, selects the highest version that matches the requirement.
/// If `req` is `None`, selects the highest overall version available.
pub fn resolve_version<'a>(
    versions: impl IntoIterator<Item = &'a Version>,
    req: Option<&VersionReq>,
) -> Option<Version> {
    match req {
        Some(requirement) => versions
            .into_iter()
            .filter(|v| requirement.matches(v))
            .max()
            .cloned(),
        None => versions.into_iter().max().cloned(),
    }
}

/// Finds all versions satisfying a given requirement, returned in ascending order.
pub fn matching_versions<'a>(
    versions: impl IntoIterator<Item = &'a Version>,
    req: &VersionReq,
) -> Vec<Version> {
    let mut matches: Vec<Version> = versions
        .into_iter()
        .filter(|v| req.matches(v))
        .cloned()
        .collect();
    matches.sort();
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_latest_and_constrained() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.1.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();
        let list = vec![v1.clone(), v2.clone(), v3.clone()];

        // No constraint -> highest overall (2.0.0)
        assert_eq!(resolve_version(&list, None), Some(v3.clone()));

        // ^1.0 constraint -> 1.1.0
        let req_1 = VersionReq::parse("^1.0").unwrap();
        assert_eq!(resolve_version(&list, Some(&req_1)), Some(v2.clone()));

        // =1.0.0 constraint -> 1.0.0
        let req_exact = VersionReq::parse("=1.0.0").unwrap();
        assert_eq!(resolve_version(&list, Some(&req_exact)), Some(v1));

        // >=3.0 -> None
        let req_none = VersionReq::parse(">=3.0").unwrap();
        assert_eq!(resolve_version(&list, Some(&req_none)), None);
    }
}
