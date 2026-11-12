//! `skillwright <verb> [options]`: argv parsing and dispatch only.
//!
//! The verbs: `init` ([`crate::commands::init`]), `build` and `check`
//! ([`crate::commands::marketplace`], one path in two modes) and `bump`
//! ([`crate::commands::bump`]). Any other verb is clap's unknown-command
//! error, a usage error.
//!
//! The global flags are declared on each subcommand rather than once on the
//! root, so they parse after the verb, among its own flags.

use std::io::Write;
use std::path::PathBuf;

use clap::builder::PossibleValuesParser;
use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use tradecraft_core::Harness;

use crate::commands::bump::{BumpFlags, BumpLevel, run_bump};
use crate::commands::init::{InitFlags, run_init};
use crate::commands::marketplace::{ExportVerb, run_export};
use crate::ctx::CommandContext;
use crate::exit::{EXIT_OK, EXIT_USAGE};
use crate::resolve::RunFlags;

const PROG: &str = "skillwright";
const DESCRIPTION: &str = "Build a vault of markdown skill notes into plugin marketplaces for \
                           coding-agent harnesses. A thin CLI over the one Rust core.";

/// The verbs the usage line for a bare `skillwright` prints, in order.
const VERBS: [&str; 4] = ["init", "build", "check", "bump"];

/// The globals that mean something to a verb writing nothing.
///
/// A verb that writes has no separate copy: [`Globals`] flattens this one and
/// adds `--dry-run` and `--yes`, so these cannot drift between the two
/// surfaces.
///
/// Each help string is spelled out rather than taken from a doc comment,
/// because clap strips a doc comment's trailing period and [`root_help`]
/// reads these back as whole sentences.
#[derive(Args, Debug, Clone)]
struct ReadOnlyGlobals {
    #[arg(
        long,
        value_name = "path",
        help = "Vault root: the folder holding `skillwright.md` and `marketplaces/`. Overrides the walk up from the working directory for this run."
    )]
    vault: Option<String>,
    #[arg(long, help = "Emit the report on stdout as one JSON document.")]
    json: bool,
    #[arg(long, help = "Drop progress and the tally from stderr.")]
    quiet: bool,
}

impl ReadOnlyGlobals {
    /// The run's flags, with `--marketplace` and `--harness` and nothing
    /// written.
    fn run(&self, marketplace: Option<&String>, harness: &HarnessArg) -> RunFlags {
        RunFlags {
            vault: self.vault.clone(),
            json: self.json,
            quiet: self.quiet,
            dry_run: false,
            yes: false,
            marketplace: marketplace.cloned(),
            harnesses: harness.names.iter().cloned().collect(),
        }
    }
}

/// The globals a verb that writes declares.
///
/// `--dry-run` and `--yes` are here and not in [`ReadOnlyGlobals`] because a
/// verb that writes nothing has no plan to preview and nothing to accept, so
/// `check --yes` is an unknown flag.
#[derive(Args, Debug, Clone)]
struct Globals {
    #[command(flatten)]
    read_only: ReadOnlyGlobals,
    #[arg(long, help = "Classify and report; write nothing.")]
    dry_run: bool,
    #[arg(long, help = "Accept the plan without prompting.")]
    yes: bool,
}

impl Globals {
    /// [`ReadOnlyGlobals::run`], with `--dry-run` and `--yes`.
    fn run(&self, marketplace: Option<&String>, harness: &HarnessArg) -> RunFlags {
        RunFlags {
            dry_run: self.dry_run,
            yes: self.yes,
            ..self.read_only.run(marketplace, harness)
        }
    }
}

/// The globals in the order the root `--help` lists them.
const GLOBAL_OPTIONS: [&str; 5] = ["vault", "json", "dry-run", "yes", "quiet"];

/// The globals a verb that never writes does not declare.
const WRITE_ONLY_GLOBALS: [&str; 2] = ["dry-run", "yes"];

/// What the root `--help` adds under the verb list: the globals, and the
/// nouns the verbs use.
///
/// The globals are declared per verb, not on the root — see the module note —
/// and that is a parsing constraint, not a documentation one. A user reading
/// `skillwright --help` has to be able to learn `--vault` exists without
/// guessing which verb to ask. The wording is read back off [`Globals`]'s own
/// declarations, so it is the same sentence the verb prints, and
/// [`WRITE_ONLY_GLOBALS`] are marked because a read-only verb does not
/// declare them.
fn root_help() -> String {
    let declared = Globals::augment_args(clap::Command::new(PROG));
    let rows: Vec<(String, String)> = GLOBAL_OPTIONS
        .iter()
        .map(|long| {
            let arg = declared
                .get_arguments()
                .find(|a| a.get_long() == Some(long))
                .expect("every global is declared on Globals");
            // A boolean global takes no value, and clap still hands back a
            // placeholder value name for one — so the action decides the
            // spelling, not the presence of a name.
            let flag_only = matches!(
                arg.get_action(),
                clap::ArgAction::SetTrue | clap::ArgAction::SetFalse
            );
            let spelling = match arg.get_value_names() {
                Some([name]) if !flag_only => format!("--{long} <{name}>"),
                _ => format!("--{long}"),
            };
            let mut describe = arg.get_help().map(ToString::to_string).unwrap_or_default();
            if WRITE_ONLY_GLOBALS.contains(long) {
                describe.push_str(" Only on a verb that writes.");
            }
            (spelling, describe)
        })
        .collect();
    let width = rows.iter().map(|(f, _)| f.len()).max().unwrap_or(0);
    let globals: Vec<String> = rows
        .iter()
        .map(|(flags, describe)| format!("  {flags:width$}  {describe}"))
        .collect();
    [
        "",
        "Global options (accepted after the verb, e.g. `build --vault ~/notes`):",
        &globals.join("\n"),
        "",
        "The three nouns:",
        "  vault        The folder holding `skillwright.md` and `marketplaces/`.",
        "  marketplace  A folder under `marketplaces/`. `build` writes each to `dist/<m>/`.",
        "  harness      A tool each marketplace is built for: claude, codex, cursor or pi.",
        "",
    ]
    .join("\n")
}

#[derive(Parser, Debug)]
#[command(name = PROG, version, about = DESCRIPTION, after_help = root_help(), disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand, Debug)]
enum Verb {
    /// Write a new vault in the current folder: the root file, one marketplace and one plugin.
    Init(InitArgs),
    /// Build every marketplace into `dist/<m>/`.
    Build(BuildArgs),
    /// Report drift between the vault and `dist/<m>/` and exit 1 on drift. Writes nothing.
    Check(CheckArgs),
    /// Raise a plugin's or a marketplace's `version`: `--major`, `--minor`, or `--patch` (default).
    Bump(BumpArgs),
}

/// The inputs `build` and `check` share: one render, compared once, two
/// things to do with the comparison.
#[derive(Args, Debug)]
struct ExportArgs {
    /// The one marketplace under `marketplaces/` to build or check. Absent means every one.
    #[arg(long, value_name = "name")]
    marketplace: Option<String>,
    #[command(flatten)]
    harness: HarnessArg,
}

/// `--harness`, which `build`, `check` and `bump` share.
#[derive(Args, Debug)]
struct HarnessArg {
    /// A harness to build: claude, codex, cursor or pi. Repeat it for several. It replaces the root file's `build.harnesses` for this run.
    #[arg(long = "harness", value_name = "name", value_parser = PossibleValuesParser::new(Harness::ALL.map(Harness::as_str)))]
    names: Vec<String>,
}

/// `build`'s surface: the shared inputs and every global.
#[derive(Args, Debug)]
struct BuildArgs {
    #[command(flatten)]
    export: ExportArgs,
    #[command(flatten)]
    globals: Globals,
}

/// `check`'s surface: the shared inputs and the globals of a verb that
/// writes nothing.
#[derive(Args, Debug)]
struct CheckArgs {
    #[command(flatten)]
    export: ExportArgs,
    #[command(flatten)]
    globals: ReadOnlyGlobals,
}

/// `init`'s two names. It reads no configuration, so it takes none of the
/// globals.
#[derive(Args, Debug)]
struct InitArgs {
    /// The marketplace's name.
    #[arg(value_name = "marketplace")]
    marketplace: String,
    /// The plugin's name.
    #[arg(value_name = "plugin")]
    plugin: String,
}

/// `bump`'s surface: the plugin to raise, the marketplace with no plugin
/// named, or `--changed-since` for every plugin that changed, and which
/// semver level — mutually exclusive, `--patch` the default when none is
/// given.
#[derive(Args, Debug)]
struct BumpArgs {
    /// The plugin to bump, by its folder name.
    #[arg(
        value_name = "plugin",
        required_unless_present_any = ["changed_since", "marketplace"]
    )]
    plugin: Option<String>,
    /// Bump every plugin whose rendered members differ from this baseline:
    /// a built marketplace directory, or a git ref checked out and built in
    /// a temporary folder.
    #[arg(long, value_name = "ref|dir", conflicts_with = "plugin")]
    changed_since: Option<String>,
    /// The marketplace to work in. With no plugin named, bump the marketplace itself.
    #[arg(long, value_name = "name")]
    marketplace: Option<String>,
    /// Bump the major version.
    #[arg(long, conflicts_with_all = ["minor", "patch"])]
    major: bool,
    /// Bump the minor version.
    #[arg(long, conflicts_with_all = ["major", "patch"])]
    minor: bool,
    /// Bump the patch version. The default when none of the three is given.
    #[arg(long, conflicts_with_all = ["major", "minor"])]
    patch: bool,
    #[command(flatten)]
    harness: HarnessArg,
    #[command(flatten)]
    globals: Globals,
}

impl BumpArgs {
    fn flags(&self) -> BumpFlags {
        BumpFlags {
            plugin: self.plugin.clone(),
            changed_since: self.changed_since.clone(),
            level: if self.major {
                BumpLevel::Major
            } else if self.minor {
                BumpLevel::Minor
            } else {
                BumpLevel::Patch
            },
            run: self.globals.run(self.marketplace.as_ref(), &self.harness),
        }
    }
}

/// Parse `argv` (as in the process arguments after the program name) and run
/// the selected verb, returning the exit code.
///
/// `out` is the report and `err` is everything else; the working directory is
/// the process's own. [`run_with`] is the same entry with the context
/// supplied, which is how the tests drive a verb without a subprocess.
pub fn run(argv: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut ctx = CommandContext::new(cwd, out, err);
    run_with(argv, &mut ctx)
}

/// [`run`], with the context supplied.
pub fn run_with(argv: &[String], ctx: &mut CommandContext<'_>) -> i32 {
    if argv.is_empty() {
        let _ = writeln!(ctx.err, "usage: {PROG} <{}> [options]", VERBS.join("|"));
        return EXIT_USAGE;
    }

    let from = std::iter::once(PROG.to_string()).chain(argv.iter().cloned());
    let cli = match Cli::try_parse_from(from) {
        Ok(cli) => cli,
        Err(error) => return report_parse_error(&error, ctx),
    };

    match &cli.verb {
        Verb::Init(args) => run_init(
            &InitFlags {
                marketplace: args.marketplace.clone(),
                plugin: args.plugin.clone(),
            },
            ctx,
        ),
        Verb::Build(args) => {
            let export = &args.export;
            let flags = args
                .globals
                .run(export.marketplace.as_ref(), &export.harness);
            run_export(ExportVerb::Build, &flags, ctx)
        }
        Verb::Check(args) => {
            let export = &args.export;
            let flags = args
                .globals
                .run(export.marketplace.as_ref(), &export.harness);
            run_export(ExportVerb::Check, &flags, ctx)
        }
        Verb::Bump(args) => run_bump(&args.flags(), ctx),
    }
}

/// clap's own error taxonomy, projected onto the CLI spec's exit codes: a help or
/// version request is the run completing, and everything else is a usage
/// error. Help goes to the report sink because it is what the user asked
/// for; an error does not.
fn report_parse_error(error: &clap::Error, ctx: &mut CommandContext<'_>) -> i32 {
    let rendered = error.render().to_string();
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            let _ = writeln!(ctx.out, "{rendered}");
            EXIT_OK
        }
        _ => {
            let _ = writeln!(ctx.err, "{rendered}");
            EXIT_USAGE
        }
    }
}
