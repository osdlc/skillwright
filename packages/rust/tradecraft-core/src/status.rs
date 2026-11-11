//! The version-bump rule the marketplace export and `bump` share.

use serde::{Deserialize, Serialize};

/// A plugin whose members changed since it was last built, with its own
/// `version` unmoved since: a suggestion to run `bump`.
///
/// The version at the last build is the built `plugin.json`'s own
/// `"version"` field, read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionBumpSuggestion {
    /// The note's current `version`.
    pub current_version: String,
    /// The `version` the installed pack manifest carries. Equal to
    /// `current_version` by construction — a suggestion exists exactly
    /// because nothing moved to answer the members having changed.
    pub published_version: String,
}

/// The version-bump rule's tail: given that a plugin's members moved, does
/// the installed pack manifest still carry the version the vault currently
/// declares?
///
/// `None` for an unreadable or unparseable manifest, and for one whose
/// version has already moved: a bump that has happened is not a bump to
/// suggest.
pub(crate) fn version_bump_if_unmoved(
    installed_manifest: Option<&[u8]>,
    current_version: String,
) -> Option<VersionBumpSuggestion> {
    let published_version = serde_json::from_slice::<serde_json::Value>(installed_manifest?)
        .ok()?
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)?;
    (published_version == current_version).then_some(VersionBumpSuggestion {
        current_version,
        published_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unmoved_version_is_a_suggestion_and_a_moved_one_is_not() {
        let manifest = br#"{"name":"p","version":"1.0.0"}"#;
        assert_eq!(
            version_bump_if_unmoved(Some(manifest), "1.0.0".to_owned()),
            Some(VersionBumpSuggestion {
                current_version: "1.0.0".to_owned(),
                published_version: "1.0.0".to_owned(),
            })
        );
        assert_eq!(
            version_bump_if_unmoved(Some(manifest), "1.0.1".to_owned()),
            None
        );
        assert_eq!(
            version_bump_if_unmoved(Some(b"not json"), "1.0.0".to_owned()),
            None
        );
        assert_eq!(version_bump_if_unmoved(None, "1.0.0".to_owned()), None);
    }
}
