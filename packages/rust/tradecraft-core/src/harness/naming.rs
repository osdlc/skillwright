//! The name a skill ships under, and the call a runtime writes for it.

use agent_adapt::RuntimeId;

use super::SkillNaming;
use crate::discover::name_refusal;
use crate::error::CoreError;
use crate::refusal;

/// The name a skill publishes under: its directory, its frontmatter `name`,
/// and the name a call to it renders. One function, so the three agree.
///
/// # Errors
///
/// `refused: published-name-invalid` — a prefixed name the Agent Skills spec does
/// not admit. Never truncated: a shortened name would not match its calls.
pub fn published_skill_name(
    naming: SkillNaming,
    plugin: &str,
    skill: &str,
) -> Result<String, CoreError> {
    match naming {
        SkillNaming::HostNamespaced | SkillNaming::Bare => Ok(skill.to_owned()),
        SkillNaming::PluginPrefixed => {
            let name = format!("{plugin}-{skill}");
            match name_refusal(&name, "published skill name") {
                None => Ok(name),
                Some(why) => Err(CoreError::refused(
                    refusal::PUBLISHED_NAME_INVALID,
                    format!("skill `{skill}` of plugin `{plugin}` cannot publish as {why}"),
                )),
            }
        }
    }
}

/// The call one runtime writes for skill `skill` of plugin `plugin`.
pub fn rendered_skill_call(
    runtime: RuntimeId,
    naming: SkillNaming,
    plugin: &str,
    skill: &str,
) -> Result<String, CoreError> {
    let name = published_skill_name(naming, plugin, skill)?;
    match naming {
        SkillNaming::HostNamespaced => checked_skill_call(runtime, Some(plugin), &name),
        SkillNaming::Bare | SkillNaming::PluginPrefixed => checked_skill_call(runtime, None, &name),
    }
}

/// `RuntimeId::skill_call`, with its error as a `skill-call-invalid` refusal.
fn checked_skill_call(
    runtime: RuntimeId,
    namespace: Option<&str>,
    name: &str,
) -> Result<String, CoreError> {
    runtime
        .skill_call(namespace, name)
        .map_err(|err| CoreError::refused(refusal::SKILL_CALL_INVALID, err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefixed_name_is_the_directory_and_the_call() {
        assert_eq!(
            published_skill_name(SkillNaming::PluginPrefixed, "tools", "b").unwrap(),
            "tools-b"
        );
        assert_eq!(
            rendered_skill_call(RuntimeId::Pi, SkillNaming::PluginPrefixed, "tools", "b").unwrap(),
            "/skill:tools-b"
        );
        for naming in [SkillNaming::HostNamespaced, SkillNaming::Bare] {
            assert_eq!(published_skill_name(naming, "tools", "b").unwrap(), "b");
        }
        assert_eq!(
            rendered_skill_call(
                RuntimeId::ClaudeCode,
                SkillNaming::HostNamespaced,
                "tools",
                "b"
            )
            .unwrap(),
            "/tools:b"
        );
        assert_eq!(
            rendered_skill_call(RuntimeId::Cursor, SkillNaming::Bare, "tools", "b").unwrap(),
            "/b"
        );

        // 33 + 1 + 31 = 65 characters: refused, never truncated.
        let plugin = "p".repeat(33);
        let skill = "s".repeat(31);
        let err = published_skill_name(SkillNaming::PluginPrefixed, &plugin, &skill).unwrap_err();
        assert!(err.to_string().contains("published-name-invalid"), "{err}");
        let err = rendered_skill_call(RuntimeId::Pi, SkillNaming::PluginPrefixed, &plugin, &skill)
            .unwrap_err();
        assert!(err.to_string().contains("published-name-invalid"), "{err}");
    }
}
