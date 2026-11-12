//! The report's format gate. A format change in `src/report.rs` that is not also made here must fail.

use skillwright::report::{render_export_tally, render_unit_line};
use tradecraft_core::{EntityKind, ExportMode, ExportUnit, Harness, StaleKind, UnitStatus};

fn plugin(status: UnitStatus) -> ExportUnit {
    ExportUnit::Plugin {
        root: Harness::Claude,
        name: "tools".to_string(),
        skills: 2,
        status,
        version_bump_suggestion: None,
    }
}

fn manifest(status: UnitStatus) -> ExportUnit {
    ExportUnit::Manifest {
        root: Harness::Claude,
        status,
    }
}

fn stale() -> ExportUnit {
    ExportUnit::Stale {
        root: Harness::Claude,
        path: ".claude-plugin/packs/gone".to_string(),
        kind: StaleKind::Plugin,
    }
}

fn stale_file() -> ExportUnit {
    ExportUnit::Stale {
        root: Harness::Claude,
        path: ".claude-plugin/README.md".to_string(),
        kind: StaleKind::File,
    }
}

fn not_carried() -> ExportUnit {
    ExportUnit::NotCarried {
        root: None,
        path: "marketplaces/house/plugins/tools/agents/triage.md".to_string(),
        kind: EntityKind::Agent,
    }
}

mod render_unit_line_tests {
    use super::*;

    #[test]
    fn a_not_carried_member_names_its_path_and_its_kind() {
        // A renderer that handles only three variants prints `undefined` for
        // the fourth.
        let line = render_unit_line(&not_carried(), ExportMode::Build);
        assert!(line.contains("marketplaces/house/plugins/tools/agents/triage.md"));
        assert!(line.contains("agent"));
        assert!(!line.contains("undefined"));
    }

    #[test]
    fn a_member_one_root_cannot_carry_names_that_root() {
        let unit = ExportUnit::NotCarried {
            root: Some(Harness::Codex),
            path: "marketplaces/house/plugins/tools/commands/ship.md".to_string(),
            kind: EntityKind::Command,
        };
        assert_eq!(
            render_unit_line(&unit, ExportMode::Build),
            "marketplaces/house/plugins/tools/commands/ship.md  not carried · command on codex"
        );
    }

    #[test]
    fn a_not_carried_member_reads_the_same_under_check_as_under_build() {
        // It is a fact about the vault's contents, not an outcome of the run,
        // so neither verb changes it — unlike a stale path, which check calls
        // drift and build removes.
        assert_eq!(
            render_unit_line(&not_carried(), ExportMode::Check),
            render_unit_line(&not_carried(), ExportMode::Build)
        );
    }

    #[test]
    fn a_not_carried_member_belongs_to_no_root() {
        // Its path is the vault's. Prefixing a root would file it under a
        // marketplace root it was precisely not written into.
        assert!(!render_unit_line(&not_carried(), ExportMode::Build).starts_with("claude/"));
    }

    #[test]
    fn the_three_original_variants_are_unchanged() {
        assert_eq!(
            render_unit_line(&plugin(UnitStatus::Differs), ExportMode::Build),
            "claude/tools  written · 2 skills"
        );
        assert_eq!(
            render_unit_line(&manifest(UnitStatus::Same), ExportMode::Check),
            "claude  ok · marketplace manifest"
        );
        assert_eq!(
            render_unit_line(&stale(), ExportMode::Check),
            ".claude-plugin/packs/gone  drift · stale plugin"
        );
        assert_eq!(
            render_unit_line(&stale(), ExportMode::Build),
            ".claude-plugin/packs/gone  removed · stale plugin"
        );
    }

    #[test]
    fn a_whole_pack_going_does_not_read_like_a_leftover_file() {
        // Both lines end in the same verb, and one of them is the
        // marketplace losing a pack. The kind is the only thing that says so.
        assert_eq!(
            render_unit_line(&stale_file(), ExportMode::Build),
            ".claude-plugin/README.md  removed · stale file"
        );
        assert_ne!(
            render_unit_line(&stale(), ExportMode::Build),
            render_unit_line(&stale_file(), ExportMode::Build)
                .replace(".claude-plugin/README.md", ".claude-plugin/packs/gone")
        );
    }
}

mod render_export_tally_tests {
    use super::*;

    #[test]
    fn every_unit_is_counted_exactly_once() {
        // The tally's own contract: its numbers sum to the units it was
        // given, and no unit is filed under a key its variant does not
        // carry. Keying on `unit.status` files a not-carried member under
        // `undefined`.
        let units = vec![
            plugin(UnitStatus::Differs),
            plugin(UnitStatus::Same),
            manifest(UnitStatus::Differs),
            stale(),
            not_carried(),
        ];
        let line = render_export_tally(&units, ExportMode::Build);
        let total: u32 = line
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u32>().unwrap())
            .sum();
        assert_eq!(total as usize, units.len());
        assert!(!line.contains("undefined"));
    }

    #[test]
    fn not_carried_is_counted_beside_stale_not_among_the_statuses() {
        assert_eq!(
            render_export_tally(
                &[plugin(UnitStatus::Differs), not_carried(), not_carried()],
                ExportMode::Build
            ),
            "1 written, 2 not carried"
        );
    }

    #[test]
    fn a_report_holding_only_not_carried_members_still_reports_them() {
        assert_eq!(
            render_export_tally(&[not_carried()], ExportMode::Build),
            "1 not carried"
        );
    }

    #[test]
    fn an_empty_report_says_so() {
        assert_eq!(
            render_export_tally(&[], ExportMode::Check),
            "nothing to report"
        );
    }

    #[test]
    fn the_mode_picks_the_status_words() {
        let units = [plugin(UnitStatus::Differs), manifest(UnitStatus::Same)];
        assert_eq!(
            render_export_tally(&units, ExportMode::Check),
            "1 drift, 1 ok"
        );
        assert_eq!(
            render_export_tally(&units, ExportMode::Build),
            "1 written, 1 unchanged"
        );
    }
}
