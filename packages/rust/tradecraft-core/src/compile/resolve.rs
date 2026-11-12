//! Where a link resolves (spec section 4): a name or a path, in the scope of
//! the linking file's plugin, and where the file it reaches lands in a pack.

use std::path::{Path, PathBuf};

use super::render::Renderer;
use super::scan::WikiToken;
use crate::error::CoreError;
use crate::frontmatter::EntityKind;
use crate::layout::{LayoutFile, MARKETPLACES_DIR, PLUGINS_DIR, Place, Unreachable, reach};
use crate::model::LinkedComponent;
use crate::refusal;
use crate::vfs::normalize;

/// Where a component's note lands in a pack laid out as Claude Code's.
pub(super) fn component_note_path(kind: EntityKind, name: &str) -> String {
    match kind {
        EntityKind::Skill => format!("skills/{name}/SKILL.md"),
        EntityKind::Command => format!("commands/{name}.md"),
        EntityKind::Agent => format!("agents/{name}.md"),
        _ => format!("{name}.md"),
    }
}

/// Where a linked file lands, and the component whose folder holds it.
pub(super) struct PackTarget {
    /// Its path in a pack laid out as Claude Code's.
    pub(super) path: String,
    pub(super) component: Option<LinkedComponent>,
    /// It is the component's note.
    pub(super) note: bool,
}

impl Renderer<'_> {
    /// A wikilink's file, and where it lands. `[[#Heading]]` is the file it
    /// is in; a target with a `/` is a path from the linking file; a name is
    /// looked up in the two steps of section 4 rule 3.
    pub(super) fn resolve_wiki(
        &self,
        token: &WikiToken<'_>,
        written: &str,
        from_file: &Path,
    ) -> Result<(PathBuf, PackTarget), CoreError> {
        let decoded = markdown_link::percent_decode(&token.target);
        let target = decoded.as_str();
        let path = if target.is_empty() {
            from_file.to_path_buf()
        } else if target.starts_with('/') {
            return Err(self.link_error(
                refusal::UNSAFE_PATH,
                from_file,
                written,
                "names a path outside the vault",
                "section 4, the link table",
            ));
        } else if target.contains('/') {
            self.resolve_path(from_file, target, written, true)?
        } else {
            self.resolve_name(from_file, target, written)?
        };
        let pack = self.pack_target(from_file, &path, written)?;
        Ok((path, pack))
    }

    /// `relative` from the folder of `from_file`. It must stay inside the
    /// marketplace (containment rule 1) and name a file the layout read; a
    /// wikilink's path with no extension may leave out `.md`. A path below a
    /// symlink in a never-ship folder, which the layout does not follow,
    /// resolves to that symlink, so the link is refused as one that never ships.
    pub(super) fn resolve_path(
        &self,
        from_file: &Path,
        relative: &str,
        written: &str,
        wiki: bool,
    ) -> Result<PathBuf, CoreError> {
        let folder = from_file.parent().unwrap_or(from_file);
        let joined = normalize(&folder.join(relative));
        let market = self.layout.root.join(MARKETPLACES_DIR).join(&self.market);
        if !joined.starts_with(&market) {
            return Err(Unreachable::outside(&self.market)
                .refuse(&self.relative_or_verbatim(from_file), written));
        }
        let with_md = PathBuf::from(format!("{}.md", joined.display()));
        let mut candidates = vec![joined.clone()];
        if wiki && !names_extension(relative) {
            candidates.push(with_md);
        }
        candidates
            .into_iter()
            .find(|path| self.layout.file(path).is_some())
            .or_else(|| {
                self.layout
                    .files_of(&self.market)
                    .find(|file| {
                        matches!(file.place, Place::NeverShip { .. })
                            && joined.starts_with(&file.path)
                    })
                    .map(|file| file.path.clone())
            })
            .ok_or_else(|| self.missing(from_file, written))
    }

    /// A name-only link: the linking file's component folder at any depth,
    /// then the plugin's shared folders and its components by name. Case is
    /// ignored, with a warning when it differs; a name with no extension may
    /// leave out `.md` (section 4 rule 3).
    fn resolve_name(
        &self,
        from_file: &Path,
        name: &str,
        written: &str,
    ) -> Result<PathBuf, CoreError> {
        let found = self.find_name(from_file, name, written)?;
        self.warn_case(&found, name, written, from_file);
        Ok(found)
    }

    /// Warns when `found` matched `name` only by ignoring case.
    fn warn_case(&self, found: &Path, name: &str, written: &str, from_file: &Path) {
        let spelled = found
            .file_name()
            .map(|file| file.to_string_lossy())
            .is_some_and(|file| file == name || file == format!("{name}.md"));
        if spelled {
            return;
        }
        self.call_context.warn(
            self.relative_or_verbatim(from_file),
            "link-case",
            format!(
                "`{written}` matches {} only by ignoring case; spell the name as the file \
                 does (section 4 rule 3)",
                self.relative_or_verbatim(found)
            ),
        );
    }

    /// The file a name-only link names, by the two steps of rule 3.
    fn find_name(&self, from_file: &Path, name: &str, written: &str) -> Result<PathBuf, CoreError> {
        let wanted = name.to_lowercase();
        let wanted_md = (!names_extension(name)).then(|| format!("{wanted}.md"));
        let named = |file: &LayoutFile| {
            file.path
                .file_name()
                .map(|found| found.to_string_lossy().to_lowercase())
                .is_some_and(|found| found == wanted || wanted_md.as_ref() == Some(&found))
        };
        let folder = self.component_folder(from_file);
        let never_ship = |file: &LayoutFile| matches!(file.place, Place::NeverShip { .. });
        if let Some(folder) = &folder {
            let hits: Vec<&LayoutFile> = self
                .layout
                .files_of(&self.market)
                .filter(|file| file.path.starts_with(folder) && !never_ship(file) && named(file))
                .collect();
            if let Some(found) = self.one_match(&hits, from_file, written)? {
                return Ok(found);
            }
        }
        let hits: Vec<&LayoutFile> = self
            .layout
            .files_of(&self.market)
            .filter(|file| file.plugin == self.plugin)
            .filter(|file| match &file.place {
                Place::Shared { skill: None, .. } => named(file),
                Place::ComponentNote { name: found, .. } => found.eq_ignore_ascii_case(name),
                _ => false,
            })
            .collect();
        if let Some(found) = self.one_match(&hits, from_file, written)? {
            return Ok(found);
        }
        // Neither step searches a never-ship folder. A name only the
        // component's or the plugin's own such folder holds returns that
        // file, which rule 10 then refuses.
        let plugin_rel = format!(
            "{MARKETPLACES_DIR}/{}/{PLUGINS_DIR}/{}",
            self.market,
            self.plugin.as_deref().unwrap_or_default()
        );
        self.layout
            .files_of(&self.market)
            .filter(|file| file.plugin == self.plugin && named(file))
            .find(|file| match &file.place {
                Place::NeverShip { folder: dir } => {
                    folder
                        .as_ref()
                        .is_some_and(|folder| file.path.starts_with(folder))
                        || Path::new(dir).parent() == Some(Path::new(&plugin_rel))
                }
                _ => false,
            })
            .map(|file| file.path.clone())
            .ok_or_else(|| self.missing(from_file, written))
    }

    /// The one file of `hits`, `None` for none, or the error naming each.
    fn one_match(
        &self,
        hits: &[&LayoutFile],
        from_file: &Path,
        written: &str,
    ) -> Result<Option<PathBuf>, CoreError> {
        match hits {
            [] => Ok(None),
            [one] => Ok(Some(one.path.clone())),
            _ => {
                let named: Vec<&str> = hits.iter().map(|file| file.rel.as_str()).collect();
                Err(self.link_error(
                    refusal::LINK_AMBIGUOUS,
                    from_file,
                    written,
                    format!("matches {}", named.join(" and ")),
                    "section 4 rule 3",
                ))
            }
        }
    }

    /// The folder of the component `file` belongs to, if any: a component
    /// note's folder, or the skill folder holding one of its files.
    pub(super) fn component_folder(&self, file: &Path) -> Option<PathBuf> {
        let found = self.layout.file(file)?;
        match &found.place {
            Place::ComponentNote { .. } => found.path.parent().map(Path::to_path_buf),
            Place::Shared {
                skill: Some(skill), ..
            } => Some(
                self.layout
                    .root
                    .join(MARKETPLACES_DIR)
                    .join(&found.marketplace)
                    .join(PLUGINS_DIR)
                    .join(found.plugin.as_deref()?)
                    .join("skills")
                    .join(skill),
            ),
            _ => None,
        }
    }

    /// Where the file `to`, linked from `from_file`, lands: it must be in the
    /// linking file's plugin, in a folder that ships, and a file a link may
    /// reach (section 4, scope of a link).
    pub(super) fn pack_target(
        &self,
        from_file: &Path,
        to: &Path,
        written: &str,
    ) -> Result<PackTarget, CoreError> {
        let Some(file) = self.layout.file(to) else {
            return Err(self.missing(from_file, written));
        };
        // The layout has refused a symlink whose real path this would refuse.
        if let Some(why) = reach(&self.market, self.plugin.as_deref(), file) {
            return Err(why.refuse(&self.relative_or_verbatim(from_file), written));
        }
        let component = |kind: EntityKind, name: &str| {
            Some(LinkedComponent {
                kind,
                name: name.to_owned(),
            })
        };
        match &file.place {
            Place::ComponentNote { kind, name } => Ok(PackTarget {
                path: component_note_path(*kind, name),
                component: component(*kind, name),
                note: true,
            }),
            Place::Shared {
                skill: Some(skill),
                path,
                ..
            } => Ok(PackTarget {
                path: format!("skills/{skill}/{path}"),
                component: component(EntityKind::Skill, skill),
                note: false,
            }),
            Place::Shared { path, .. } => Ok(PackTarget {
                path: path.clone(),
                component: None,
                note: false,
            }),
            _ => Err(Unreachable::no_link_may_reach(&file.rel)
                .refuse(&self.relative_or_verbatim(from_file), written)),
        }
    }
}

// --- Destinations, extensions, labels ---------------------------------------

/// The extension of a link target as written, lowercased with its dot —
/// `None` when the basename has no dot or only a leading one. This decides
/// whether a target is addressed as a file or as a note.
///
fn extension_of_target(target: &str) -> Option<String> {
    let base = basename(target);
    match base.rfind('.') {
        Some(0) | None => None,
        Some(at) => Some(base[at..].to_lowercase()),
    }
}

/// Whether a wikilink's target ends in a file extension: a dot, then letters
/// and digits with at least one letter, such as `.sh`. Such a target names
/// that file only; any other may leave out `.md`, so `[[v1.2 notes]]` finds
/// `v1.2 notes.md` (section 4 rule 3).
fn names_extension(target: &str) -> bool {
    let base = basename(target);
    base.rfind('.').filter(|at| *at > 0).is_some_and(|at| {
        let ext = &base[at + 1..];
        !ext.is_empty()
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

/// The extension of a resolved path, as [`extension_of_target`] reads it.
pub(super) fn extension_of_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(extension_of_target)
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_is_a_dot_then_letters_and_digits() {
        for target in ["run.sh", "a/b.mp3", "x.7z", "notes.MD"] {
            assert!(names_extension(target), "{target}");
        }
        for target in ["guide", "v1.2", "v1.2 notes", ".hidden", "a.b/c", "x."] {
            assert!(!names_extension(target), "{target}");
        }
    }
}
