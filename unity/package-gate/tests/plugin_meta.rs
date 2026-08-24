//! R-E21, over the native libraries the package ships.
//!
//! `docs/specification/07-embedding-and-distribution.md` R-E21 requires each
//! native library the package ships to carry a `.meta` declaring the platform
//! and CPU that
//! `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D3
//! assigns to its target, in that table's exact casing.
//!
//! **The `.meta` is the only mechanism, and its absence is silent.** D2 records
//! that Unity's path-inference table is rooted at `Assets/` in every row, so it
//! reaches no package at all; it has no Android row, and cannot express macOS
//! arm64. A native library matching no pattern takes the *Editor platform*
//! defaults, and inside a package that is every library — so a plugin shipped
//! without a correct `.meta` is Editor-only and absent from every player build.
//! Nothing in that build reports it.
//!
//! **Casing is the substance rather than pedantry.** Unity parses these values
//! through an enum converter and, on failure, substitutes the default with a
//! warning rather than an error; for Android its own documentation states it
//! does not validate the setting. So `arm64` where `ARM64` is required is not a
//! build error, it is a library that is quietly not there.
//!
//! **This is the textual half of R-E21's check, and it is not the whole of it.**
//! `DashsceneEditorCompat.CheckNativePlugins`, which `just unity-editor` runs,
//! reads the same values back through `PluginImporter` in a real editor — the
//! only place Unity's own parse of them can be observed. This half compares two
//! committed texts, so it needs neither an editor nor the .NET SDK and runs in
//! the sanity tier on every pull request. The specification names both.
//!
//! **D3's table is read out of the record rather than believed.**
//! [`the_transcribed_rows_are_d3s_table`] parses the matrix under D3's heading
//! and compares it against [`ROWS`], so the oracle this gate measures against is
//! the record R-E21 cites and not a transcription of it that can drift from it
//! in silence.
//!
//! # Two rules here are wider than R-E21's stated comparison
//!
//! Both are about the comparison's own validity rather than about a key D3
//! states, and both are stated where they are enforced — [`check_meta`] for the
//! first and [`ANY_PLATFORM`] for the second.
//!
//! - **The platforms a row names are enabled and every other entry is
//!   disabled.** Without the second half, the Android `.so` enabled for the
//!   editor as well, or an arm64 `.dylib` enabled for Windows, satisfies every
//!   key its row states while reaching a build D3 never assigned it to.
//! - **`Any` is refused.** A plugin compatible with any platform is included
//!   everywhere whatever the per-platform values below it say, so a wrong `CPU`
//!   under it has no observable consequence and this gate would be measuring
//!   nothing.
//!
//! # What this deliberately does not catch
//!
//! - **The library's own bytes, past the two properties D3's row states about
//!   them.** Each shipped file's header is read — the Mach-O magic and
//!   `cputype`, the ELF magic and `e_machine` — for its architecture, compared
//!   against the `CPU` the same D3 row states, and for its container, compared
//!   against the extension that row's `file` cell names. So an x86_64 `.dylib`
//!   under `macOS/`, the Android `.so` copied over it, and a zero-length file
//!   are each named. **That is a header match and nothing else.** A library of
//!   the right shape built from another commit passes here, as does one whose
//!   exported symbols, ABI version or row sizes disagree with this tree: nothing
//!   in this file is a freshness check.
//!
//!   D5 of that record states that nothing verifies a shipped binary against the
//!   declarations. Since story #1334 that is true of the binary's *freshness*
//!   rather than of the binary, from two directions: the header comparison here,
//!   and `just unity-render`, which no longer stages a library it built itself
//!   and so builds a player that loads the committed one — `DashsceneRuntime`'s
//!   constructor runs the `ds_abi_version` handshake against it (R-E16) and
//!   `AcquireFrame` compares every array's `DsSlice::stride` against the
//!   package's own row sizes (R-E17). That gate needs a Unity editor and runs
//!   outside CI, so on a pull request the header comparison here is the whole of
//!   what reads a shipped binary.
//! - **Whether a `.meta` exists at all, as a requirement of its own.** That is
//!   R-E2, checked over every imported path by
//!   `the_unity_package_meta_files_are_all_or_nothing` in
//!   `demo/tests/registry_consistency.rs`. A missing `.meta` fails here too,
//!   because there is then nothing to compare against, but the requirement it
//!   belongs to is that one.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the package's native libraries sit, under the package directory.
///
/// **This path is how the gate decides which of D3's rows describes a library,
/// and it is not how Unity decides anything.** D2 is explicit that the folder
/// name reaches no Unity path-inference rule, and R-E21 repeats it. The
/// directory is a statement of intent by whoever put the file there, and
/// comparing that intent against the `.meta` is the point — a `.dylib` under
/// `Android/` is a defect this arrangement can see and Unity cannot.
const PLUGINS_DIR: &str = "Runtime/Plugins";

/// What counts as a native library.
///
/// The four file kinds D3's table names, and the same list the editor half
/// carries as `NativeLibraryExtensions`. `.a` is iOS's, which ships nothing
/// today and is listed so a static library dropped in is checked rather than
/// walked past.
const LIBRARY_EXTENSIONS: [&str; 4] = ["dylib", "so", "dll", "a"];

/// The libraries this branch ships, relative to [`PLUGINS_DIR`], sorted.
///
/// **Named rather than counted, and that is the difference between a gate and a
/// formality.** A count is satisfied by deleting one library and duplicating
/// another, so the deleted platform ships nothing with the number unmoved. It is
/// also satisfied by an empty or missing `Runtime/Plugins/`, which every
/// per-library assertion below would report as a clean pass over nothing.
///
/// **It is a fixed list and the walk is not**, deliberately. The walk finds
/// whatever is there, so a library added at a path nobody thought about reaches
/// the comparison; this list is what turns the walk's result into a claim.
/// Shipping the Windows, Linux or iOS library D3 also describes means adding it
/// here — those rows have no consumer today, which is a scope decision recorded
/// with story #1334 rather than an omission, and D3's rows for them are
/// transcribed below so the library itself needs no change to this gate.
///
/// `ShippedPlugins()` in `unity/editor-compat/DashsceneEditorCompat.cs` is the
/// same list for the editor half.
const SHIPPED: [&str; 2] = [
    "Android/libdashscene_ffi.so",
    "macOS/libdashscene_ffi.dylib",
];

/// One row of D3's per-platform matrix.
#[derive(Debug)]
struct Row {
    /// The directory under [`PLUGINS_DIR`] that names this target.
    dir: &'static str,
    /// D3's own "target" column, quoted in failures so a message names the row
    /// a reader has to go and read.
    target: &'static str,
    /// D3's "file" column.
    file: &'static str,
    /// What D3's "`.meta` must set" column states, per platform.
    platforms: &'static [RowPlatform],
}

/// One platform inside a row, and the keys that row states for it.
#[derive(Debug)]
struct RowPlatform {
    /// How the `.meta` names this platform. **D3's own word first**, and after
    /// it any other spelling that denotes the same platform.
    ///
    /// More than one is needed because Unity has changed the serialization. In
    /// `serializedVersion: 2` a platform entry is a `first:` mapping of
    /// *group* to *build target*, and D3's word is the group — `Standalone:
    /// OSXUniversal`. In `serializedVersion: 3`, which Unity 6 writes and which
    /// the committed files carry, `platformData` is keyed by the build target
    /// alone and the group is gone — `OSXUniversal:`. D3 names neither
    /// serialization, so a row states the group and the targets it covers here.
    ///
    /// **A group is not enough to identify an entry, and matching on one alone
    /// was a hole.** `Standalone` covers `OSXUniversal`, `Win64` and `Linux64`
    /// together, so a `serializedVersion: 2` entry reading `Standalone: Win64`
    /// answered to the macOS row's `Standalone` — an enabled Windows entry
    /// carrying `CPU: ARM64` satisfied the row while the library reached no
    /// macOS build at all. [`PlatformData::is`] therefore requires the *target*
    /// to be one of these names as well wherever an entry states one.
    names: &'static [&'static str],
    /// **Only the keys the row states.** R-E21 requires the comparison to be
    /// over these rather than over a fixed pair, because `OS` is stated for the
    /// desktop rows and for neither Android nor iOS. A `.meta` carries other
    /// keys — Unity writes `DefaultValueInitialized` into every editor entry it
    /// touches, and `Is16KbAligned` into an Android one — and those are not this
    /// requirement's business.
    settings: &'static [(&'static str, &'static str)],
}

/// D3's table, transcribed.
///
/// **The transcription is checked against the record, not trusted.**
/// [`the_transcribed_rows_are_d3s_table`] parses the matrix out of
/// [`D3_RECORD`] and requires it to equal what is written here, row for row and
/// key for key. Without it this constant is a second source for R-E21's oracle:
/// a row deleted from it, or a `CPU` pair dropped out of one, leaves every
/// assertion below passing over a rule nothing states any more.
///
/// **What the record cannot supply stays local, and is named.** [`Row::dir`] is
/// D2's `Runtime/Plugins/<platform>/` and appears in no cell of the table; the
/// second and later entries of [`RowPlatform::names`] are the build-target
/// spellings the prose under the table gives for D3's groups. The crate-type
/// column is not transcribed at all — it is D1's subject, and no `.meta` states
/// it.
///
/// **The "`.meta` must set" column is what is transcribed, not the target
/// column.** D3's macOS row states settings for two platforms, `Editor` and
/// `Standalone`; its Windows and Linux rows say "editor + standalone" in the
/// target column and then state `Editor` keys only. R-E21 asks for "every key
/// that row states", so what is written below is what each row states and
/// nothing inferred from the prose beside it. Where the record is silent this is
/// silent too, which is why the iOS row compares no key and requires only that
/// the platform is declared and enabled.
const ROWS: &[Row] = &[
    Row {
        dir: "macOS",
        target: "macOS editor + standalone, arm64",
        file: "libdashscene_ffi.dylib",
        platforms: &[
            RowPlatform {
                names: &["Editor"],
                settings: &[("OS", "OSX"), ("CPU", "ARM64")],
            },
            RowPlatform {
                names: &["Standalone", "OSXUniversal"],
                settings: &[("CPU", "ARM64")],
            },
        ],
    },
    Row {
        dir: "Windows",
        target: "Windows editor + standalone, x64",
        file: "dashscene_ffi.dll",
        platforms: &[RowPlatform {
            names: &["Editor"],
            // `x86_64` in lower case here and `ARM64` in upper case above, both
            // out of the same table. That is not a transcription slip: D3 states
            // in terms that the casing differs between platforms, because the
            // enum Unity parses each value into differs.
            settings: &[("OS", "Windows"), ("CPU", "x86_64")],
        }],
    },
    Row {
        dir: "Linux",
        target: "Linux editor + standalone, x64",
        file: "libdashscene_ffi.so",
        platforms: &[RowPlatform {
            names: &["Editor"],
            settings: &[("OS", "Linux"), ("CPU", "x86_64")],
        }],
    },
    Row {
        dir: "Android",
        target: "Android player, arm64",
        file: "libdashscene_ffi.so",
        platforms: &[RowPlatform {
            names: &["Android"],
            settings: &[("CPU", "ARM64")],
        }],
    },
    Row {
        dir: "iOS",
        target: "iOS, v1",
        file: "libdashscene_ffi.a",
        // D3 states `iOS` and no key for this row, so this compares nothing and
        // requires only that the platform is declared and enabled. Written down
        // rather than left out: a `.a` appearing under `iOS/` is then held to
        // the file name and to that entry, which is more than nothing, and the
        // day the row gains a key it gains it here.
        platforms: &[RowPlatform {
            names: &["iOS"],
            settings: &[],
        }],
    },
];

// ---------------------------------------------------------------------------
// D3's table, read out of the record
//
// **R-E21's check is stated against D3's table, and until this section existed
// it ran against a transcription of it.** The difference is not academic: with
// only the constant above, deleting the `("CPU", "ARM64")` pair from the macOS
// standalone platform, or deleting the Windows, Linux and iOS rows outright,
// left every test in this file passing — and the first of those made a `.meta`
// saying `CPU: x86_64` for the standalone entry acceptable. What is parsed here
// is the oracle; `ROWS` is then a claim about it that one test settles.
// ---------------------------------------------------------------------------

/// The record R-E21 sends a reader to, relative to the repository root.
const D3_RECORD: &str = "docs/decisions/the-native-library-ships-inside-the-unity-package.md";

/// The line D3's matrix follows.
const D3_HEADING: &str = "**D3 — the per-platform matrix.**";

/// The columns D3's matrix carries, in the record's order.
///
/// Compared rather than assumed, so a column inserted or reordered stops this
/// parser instead of silently moving which cell it reads as the file name.
const D3_COLUMNS: [&str; 4] = ["target", "crate type", "file", "`.meta` must set"];

/// One platform D3 names in a "`.meta` must set" cell, and the `KEY=VALUE`
/// pairs it states for that platform.
type StatedPlatform = (String, Vec<(String, String)>);

/// One row of D3's table, as the record states it.
#[derive(Debug, PartialEq, Eq)]
struct RecordRow {
    /// The `target` cell.
    target: String,
    /// The `file` cell, with its backticks removed.
    file: String,
    /// The `` `.meta` must set `` cell, platform by platform.
    platforms: Vec<StatedPlatform>,
}

/// D3's table, parsed out of the record's Markdown.
///
/// **Every row is parsed or the parse fails; no row is skipped.** A parser that
/// walks past a cell it does not understand turns this comparison into an
/// agreement about a subset nobody chose — which is the same fail-open shape the
/// `.meta` reader above refuses, arrived at from the oracle's side instead.
fn d3_table(record: &str) -> Result<Vec<RecordRow>, String> {
    let lines: Vec<&str> = record.lines().collect();
    let heading = lines
        .iter()
        .position(|line| line.trim() == D3_HEADING)
        .ok_or_else(|| {
            format!(
                "carries no line reading `{D3_HEADING}`, so there is nothing \
                 here to find R-E21's table by."
            )
        })?;
    let first = lines[heading..]
        .iter()
        .position(|line| line.trim_start().starts_with('|'))
        .map(|at| heading + at)
        .ok_or_else(|| format!("has no Markdown table after `{D3_HEADING}`."))?;
    let table: Vec<&str> = lines[first..]
        .iter()
        .copied()
        .take_while(|line| line.trim_start().starts_with('|'))
        .collect();

    let header = table_cells(table[0])?;
    if header.iter().map(String::as_str).collect::<Vec<_>>() != D3_COLUMNS {
        return Err(format!(
            "states the columns {header:?} under `{D3_HEADING}`, and this parser \
             reads {D3_COLUMNS:?}. Which cell holds the file and which holds the \
             `.meta` keys is decided by position, so it will not read a table it \
             does not recognise."
        ));
    }
    let rule = table_cells(table.get(1).ok_or("carries a table header and no rows.")?)?;
    if !rule
        .iter()
        .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
    {
        return Err(format!(
            "has {rule:?} where the header rule of D3's table should be, so what \
             follows is not a row of the matrix."
        ));
    }

    table[2..]
        .iter()
        .map(|line| {
            let cells = table_cells(line)?;
            let target = cells[0].clone();
            if target.is_empty() {
                return Err(format!("has a table row `{line}` naming no target."));
            }
            let file = unbacktick(&cells[2]).ok_or_else(|| {
                format!(
                    "states the file `{}` for `{target}`, which is not \
                     backtick-quoted as every other file cell is.",
                    cells[2]
                )
            })?;
            let platforms = d3_platforms(&cells[3])
                .map_err(|why| format!("has a row for `{target}` that {why}"))?;
            Ok(RecordRow {
                target,
                file,
                platforms,
            })
        })
        .collect()
}

/// The cells of one Markdown table row.
fn table_cells(line: &str) -> Result<Vec<String>, String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('|')
        .and_then(|rest| rest.strip_suffix('|'))
        .ok_or_else(|| format!("has `{trimmed}` where a `|`-delimited row belongs."))?;
    let cells: Vec<String> = inner
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    if cells.len() != D3_COLUMNS.len() {
        return Err(format!(
            "has `{trimmed}`, which carries {} cells where D3's table carries {}.",
            cells.len(),
            D3_COLUMNS.len()
        ));
    }
    Ok(cells)
}

/// The `KEY=VALUE` pairs D3 states, per platform, in one "`.meta` must set" cell.
fn d3_platforms(cell: &str) -> Result<Vec<StatedPlatform>, String> {
    cell.split(';').map(d3_platform).collect()
}

/// One platform's clause of that cell.
///
/// The grammar is the one the record writes: a platform name, bare or
/// backtick-quoted, then zero or more backtick-quoted `KEY=VALUE` pairs.
/// Anything else refuses.
fn d3_platform(clause: &str) -> Result<StatedPlatform, String> {
    // **The iOS row ends in prose about the C# rather than about the `.meta`** —
    // "`iOS`, and the C# becomes `DllImport(\"__Internal\")`". What follows the
    // comma is read as that prose, which is only safe while it states no key, so
    // that is asserted rather than assumed: an `=` after the comma would be a
    // pair this parser silently dropped.
    let (head, tail) = match clause.split_once(',') {
        Some((head, tail)) => (head, Some(tail)),
        None => (clause, None),
    };
    if let Some(tail) = tail
        && tail.contains('=')
    {
        return Err(format!(
            "states `{}` after a comma. This parser reads what follows a comma \
             as prose about something other than the `.meta`, and it will not \
             do that to text carrying an `=`.",
            tail.trim()
        ));
    }

    let mut rest = head.trim();
    let name = if rest.starts_with('`') {
        let (token, remainder) = backtick_token(rest)?;
        rest = remainder;
        token
    } else {
        let end = rest.find('`').unwrap_or(rest.len());
        let (word, remainder) = rest.split_at(end);
        rest = remainder.trim_start();
        word.trim().to_string()
    };
    if name.split_whitespace().count() != 1 {
        return Err(format!(
            "names the platform `{name}`, which is not one word — so this parser \
             cannot say which platform the keys beside it belong to."
        ));
    }

    let mut settings = Vec::new();
    while !rest.is_empty() {
        let (token, remainder) = backtick_token(rest)?;
        rest = remainder;
        let Some((key, value)) = token.split_once('=') else {
            return Err(format!(
                "states `{token}` for `{name}`, which is not a `KEY=VALUE` pair."
            ));
        };
        if value.contains('=') {
            return Err(format!(
                "states `{token}` for `{name}`, which carries more than one `=`."
            ));
        }
        settings.push((key.trim().to_string(), value.trim().to_string()));
    }
    Ok((name, settings))
}

/// The first backtick-quoted span of `text`, and what follows it.
fn backtick_token(text: &str) -> Result<(String, &str), String> {
    let after = text.strip_prefix('`').ok_or_else(|| {
        format!(
            "carries `{text}` where a backtick-quoted item belongs. Every value \
             in that column is quoted, so unquoted text here is prose this \
             parser will not read past."
        )
    })?;
    let (token, remainder) = after
        .split_once('`')
        .ok_or_else(|| format!("carries an unbalanced backtick in `{text}`."))?;
    Ok((token.to_string(), remainder.trim_start()))
}

/// `text` with its surrounding backticks removed, if it has a pair and no more.
fn unbacktick(text: &str) -> Option<String> {
    let inner = text.strip_prefix('`')?.strip_suffix('`')?;
    if inner.contains('`') {
        return None;
    }
    Some(inner.to_string())
}

/// The platform that means "every platform".
///
/// **It must be off for the rest of this comparison to mean anything.** A plugin
/// compatible with any platform is included everywhere whatever its per-platform
/// values say, so a wrong `CPU` would not show up as a missing library and this
/// gate would not be measuring R-E21. The editor half refuses it through
/// `GetCompatibleWithAnyPlatform`, which is the same assertion against the
/// parsed importer rather than against the text.
const ANY_PLATFORM: &str = "Any";

/// One platform entry of a `PluginImporter`'s `platformData`.
#[derive(Debug)]
struct PlatformData {
    /// What the entry calls the platform: the key of the `first:` mapping in
    /// `serializedVersion: 2`, and the `platformData` key itself in 3.
    name: String,
    /// The build target inside the group, in `serializedVersion: 2` only —
    /// `OSXUniversal` for a macOS standalone. Empty in 3, where the key above
    /// already is the build target.
    ///
    /// **Compared wherever an entry states one.** D3's table names groups, and
    /// the prose under it gives the target each group means; [`PlatformData::is`]
    /// requires both, because a group covers several targets and the group alone
    /// cannot say which entry a row describes.
    target: String,
    enabled: bool,
    settings: BTreeMap<String, String>,
}

impl PlatformData {
    /// How a failure message names this entry.
    fn label(&self) -> String {
        if self.target.is_empty() {
            self.name.clone()
        } else {
            format!("{}: {}", self.name, self.target)
        }
    }

    /// Whether this entry is the one `platform` describes.
    ///
    /// **Both halves of a `serializedVersion: 2` entry have to answer to the
    /// row, not either half.** The key there is a *group*, and `Standalone`
    /// covers `OSXUniversal`, `Win64` and `Linux64` at once — so an entry whose
    /// group the row names and whose target it does not is a different
    /// platform's entry, however correct its settings look. Matching on the
    /// group alone let an enabled `Standalone: Win64` stand in for the macOS
    /// row's standalone entry.
    ///
    /// In `serializedVersion: 3` the key already is the build target and there
    /// is no group, so the name is the whole of the comparison.
    fn is(&self, platform: &RowPlatform) -> bool {
        if self.target.is_empty() {
            platform.names.contains(&self.name.as_str())
        } else {
            platform.names.contains(&self.name.as_str())
                && platform.names.contains(&self.target.as_str())
        }
    }
}

// ---------------------------------------------------------------------------
// The reader
// ---------------------------------------------------------------------------

/// The lines of the top-level `PluginImporter:` block, as (indent, content).
///
/// A textual read rather than a YAML parse, and this crate takes no dependency
/// for it. **It refuses what it does not understand** rather than skipping it:
/// every shape below either becomes an entry or becomes an error, because a
/// reader that quietly ignores a line it cannot classify turns a `.meta` it
/// mis-parses into an empty platform set — the same fail-open as an empty
/// directory, arrived at from inside the file.
fn plugin_importer_block(meta: &str) -> Result<Vec<(usize, String)>, String> {
    let all: Vec<&str> = meta.lines().collect();
    let start = all
        .iter()
        .position(|line| line.trim_end() == "PluginImporter:")
        .ok_or_else(|| {
            "declares no top-level `PluginImporter:` block. Unity gives a native \
             library that no PluginImporter describes the Editor-platform \
             defaults (D2), so it is absent from every player build with nothing \
             reporting it."
                .to_string()
        })?;

    let mut out = Vec::new();
    for line in &all[start + 1..] {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        // A line back at column zero is the next top-level key, so the block has
        // ended.
        if indent == 0 {
            break;
        }
        out.push((indent, line.trim().to_string()));
    }
    Ok(out)
}

/// Every platform entry the `PluginImporter` declares.
///
/// **Both serializations, because D3's table names neither and the committed
/// files have already changed shape once.** Unity 6 writes `serializedVersion:
/// 3`, where `platformData` is a mapping keyed by build target; an older editor
/// writes 2, where it is a sequence of `- first:`/`second:` entries. A `.meta`
/// is rewritten by whichever editor imports it, so which shape is on disk is not
/// this repository's choice to make.
fn plugin_platforms(meta: &str) -> Result<Vec<PlatformData>, String> {
    let block = plugin_importer_block(meta)?;
    let at = block
        .iter()
        .position(|(_, content)| content.starts_with("platformData:"))
        .ok_or_else(|| {
            "has a `PluginImporter:` block that declares no `platformData:`. \
             Every platform then takes the importer's default, which inside a \
             package is Editor-only (D2)."
                .to_string()
        })?;
    if block[at].1 != "platformData:" {
        return Err(format!(
            "declares `{}`, which is a platformData carrying no entry at all. \
             Every platform then takes the importer's default, which inside a \
             package is Editor-only (D2).",
            block[at].1
        ));
    }
    let list_indent = block[at].0;

    // Everything under `platformData:`, in either shape: its own children, plus
    // the sequence items of the older shape, which sit at the key's indentation
    // rather than past it. The first line that is neither ends the region.
    let body = block[at + 1..]
        .iter()
        .take_while(|(indent, content)| *indent > list_indent || content.starts_with("- "));

    let mut entries: Vec<Vec<(usize, String)>> = Vec::new();
    let mut sequence = false;
    let mut key_indent: Option<usize> = None;
    for (indent, content) in body {
        // `serializedVersion: 2` — a new entry. Its `first:` key sits two
        // columns past the dash, and recording it there rather than at the
        // dash's column lets the entry read as an ordinary mapping from here on.
        if let Some(rest) = content.strip_prefix("- ") {
            sequence = true;
            entries.push(vec![(*indent + 2, rest.to_string())]);
            continue;
        }
        // `serializedVersion: 3` — a new entry is a key at the depth the first
        // one appeared at. Anything deeper belongs to the entry already open.
        if !sequence && key_indent.is_none_or(|first| *indent == first) {
            key_indent = Some(*indent);
            entries.push(vec![(*indent, content.clone())]);
            continue;
        }
        let Some(current) = entries.last_mut() else {
            return Err(format!(
                "carries `{content}` under `platformData:` before any entry \
                 begins, which this reader does not understand."
            ));
        };
        current.push((*indent, content.clone()));
    }

    entries.iter().map(|entry| parse_entry(entry)).collect()
}

/// One platform entry, in either serialization.
fn parse_entry(lines: &[(usize, String)]) -> Result<PlatformData, String> {
    let key_indent = lines[0].0;
    let mut named: Option<(String, String)> = None;
    // In `serializedVersion: 3` the entry *is* what `second:` holds in 2, so it
    // starts already inside that mapping.
    let mut seen_second = lines[0].1 != "first:";
    if seen_second {
        let (name, value) = split_pair(&lines[0].1)?;
        if !value.is_empty() {
            return Err(format!(
                "carries a platformData entry written `{}` on one line. This \
                 reader parses the block form Unity writes and nothing else.",
                lines[0].1
            ));
        }
        named = Some((name, String::new()));
    }

    let mut enabled: Option<bool> = None;
    let mut settings: BTreeMap<String, String> = BTreeMap::new();
    // The indent of the `settings:` key while its own mapping is open. Cleared
    // when a line dedents back to it or past it, so a key written after the
    // settings block is read as a key of the entry and not as a setting.
    let mut settings_indent: Option<usize> = None;

    for (indent, content) in &lines[1..] {
        if *indent <= key_indent {
            if content == "second:" {
                seen_second = true;
                settings_indent = None;
                continue;
            }
            return Err(format!(
                "carries `{content}` beside `first:` and `second:` in a \
                 platformData entry, which this reader does not understand."
            ));
        }

        if !seen_second {
            if named.is_some() {
                return Err(format!(
                    "carries more than one line under a `first:` mapping, the \
                     second being `{content}`. One platform entry names one \
                     platform, so this reader does not know which is meant."
                ));
            }
            named = Some(split_pair(content)?);
            continue;
        }

        if let Some(open) = settings_indent {
            if *indent > open {
                let (key, value) = split_pair(content)?;
                if settings.insert(key.clone(), value).is_some() {
                    return Err(format!(
                        "sets `{key}` twice inside one settings mapping, so \
                         which value Unity reads is not knowable here."
                    ));
                }
                continue;
            }
            settings_indent = None;
        }

        let (key, value) = split_pair(content)?;
        match (key.as_str(), value.as_str()) {
            ("enabled", "1") => enabled = Some(true),
            ("enabled", "0") => enabled = Some(false),
            ("enabled", other) => {
                return Err(format!(
                    "carries `enabled: {other}` in a platformData entry. Unity \
                     writes 0 or 1, and this reader will not guess what a third \
                     value means."
                ));
            }
            ("settings", "") => settings_indent = Some(*indent),
            ("settings", "{}") => {}
            _ => {
                return Err(format!(
                    "carries `{content}` inside a platformData entry, which this \
                     reader does not understand."
                ));
            }
        }
    }

    let (name, target) = named.ok_or_else(|| {
        "carries a platformData entry whose `first:` mapping is empty, so it \
         names no platform."
            .to_string()
    })?;
    let enabled = enabled.ok_or_else(|| {
        format!(
            "carries a `{name}` platformData entry with no `enabled:` key, so \
             whether the library reaches that platform is not knowable from the \
             file."
        )
    })?;
    Ok(PlatformData {
        name,
        target,
        enabled,
        settings,
    })
}

/// `key: value`, with the scalar's surrounding spaces removed.
///
/// **Trimming is not a relaxation of the byte comparison.** A YAML scalar is
/// what follows `: `, so the space is punctuation rather than content; `arm64`
/// still fails against `ARM64`. What it trades away is a value Unity would have
/// to write with a trailing space, which it does not.
fn split_pair(content: &str) -> Result<(String, String), String> {
    let Some((key, value)) = content.split_once(':') else {
        return Err(format!(
            "carries `{content}`, which is not a `key: value` line. This reader \
             parses nothing else."
        ));
    };
    Ok((key.trim().to_string(), value.trim().to_string()))
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// D3's row for a library at `relative`, a path under [`PLUGINS_DIR`].
///
/// **Selected by directory rather than by file name**, because the file name
/// cannot answer it: D3's Linux row and its Android row both name
/// `libdashscene_ffi.so`. The file name is then compared against the row the
/// directory chose, so a library in the wrong directory and a library under the
/// wrong name are separate, named failures.
fn row_for(relative: &str) -> Result<&'static Row, String> {
    let Some((dir, _)) = relative.split_once('/') else {
        return Err(format!(
            "{relative} sits directly in {PLUGINS_DIR}/ rather than in a \
             per-target directory under it, so nothing says which row of D3's \
             table describes it. The rows are {:?}.",
            row_dirs()
        ));
    };
    ROWS.iter().find(|row| row.dir == dir).ok_or_else(|| {
        format!(
            "{relative} sits under {PLUGINS_DIR}/{dir}/, which names no row of \
             D3's table, so there is nothing to hold its `.meta` to. The rows \
             are {:?}.",
            row_dirs()
        )
    })
}

/// The directory of every row, for a failure that has to list them.
fn row_dirs() -> Vec<&'static str> {
    ROWS.iter().map(|row| row.dir).collect()
}

/// The one enabled entry for `platform`, or why there is not exactly one.
///
/// **Enabled, not merely present.** A disabled entry is the library absent from
/// that platform's build with every stated key perfectly correct — the outcome
/// R-E21's casing rule exists to prevent, reached by another route — so an entry
/// that is not enabled is not the entry the row describes.
///
/// **Exactly one, because one spelling can cover several targets.** In
/// `serializedVersion: 2` the `Standalone` group covers `OSXUniversal`, `Win64`
/// and `Linux64`, and a `.meta` commonly carries a disabled entry for each
/// target it does not serve. Those are skipped by the enabled test; two
/// *enabled* ones would leave D3's single line of settings describing two
/// entries, and this refuses rather than picking one.
fn enabled_entry<'a>(
    entries: &'a [PlatformData],
    platform: &RowPlatform,
    row: &Row,
) -> Result<&'a PlatformData, String> {
    let present: Vec<&PlatformData> = entries.iter().filter(|e| e.is(platform)).collect();
    let enabled: Vec<&&PlatformData> = present.iter().filter(|e| e.enabled).collect();
    let name = platform.names[0];
    match enabled.len() {
        1 => Ok(enabled[0]),
        0 if present.is_empty() => Err(format!(
            "declares no `{name}` platform entry, and D3's row for `{}` states \
             one. A platform the PluginImporter does not describe takes the \
             importer's default, which inside a package is Editor-only (D2). \
             The spellings that would have named it are {:?}, and the platforms \
             this file declares are {:?}.",
            row.target,
            platform.names,
            declared(entries)
        )),
        0 => Err(format!(
            "declares {} `{name}` platform {}, and none of them is enabled — so \
             the library reaches no `{name}` build however correct its settings \
             are. D3's row for `{}` states settings for `{name}`. The platforms \
             this file declares are {:?}.",
            present.len(),
            if present.len() == 1 {
                "entry"
            } else {
                "entries"
            },
            row.target,
            declared(entries)
        )),
        n => Err(format!(
            "declares {n} enabled `{name}` platform entries ({:?}). D3's row for \
             `{}` states one set of settings for `{name}` — this package ships \
             one file per target — so which entry that row describes is not \
             knowable here.",
            enabled.iter().map(|e| e.label()).collect::<Vec<_>>(),
            row.target
        )),
    }
}

/// Every platform a `.meta` declares, for a failure message.
fn declared(entries: &[PlatformData]) -> Vec<String> {
    entries.iter().map(PlatformData::label).collect()
}

/// R-E21's comparison, for one library's `.meta`.
///
/// `meta_name` is the path the failure names — relative to [`PLUGINS_DIR`], so a
/// message reads the same whether the walk was over the package or over a
/// fixture.
fn check_meta(meta_name: &str, meta: &str, row: &Row) -> Result<(), String> {
    let entries = plugin_platforms(meta).map_err(|why| format!("{meta_name} {why}"))?;

    if let Some(any) = entries
        .iter()
        .find(|e| e.enabled && (e.name == ANY_PLATFORM || e.target == ANY_PLATFORM))
    {
        return Err(format!(
            "{meta_name}: the `{}` platform entry is enabled, so the library is \
             included everywhere whatever the per-platform settings below it \
             say. A wrong `CPU` would then not show up as a missing library, and \
             this comparison would be measuring nothing.",
            any.label()
        ));
    }

    for platform in row.platforms {
        let entry =
            enabled_entry(&entries, platform, row).map_err(|why| format!("{meta_name} {why}"))?;

        for (key, want) in platform.settings {
            match entry.settings.get(*key) {
                None => {
                    return Err(format!(
                        "{meta_name}: the `{}` platform entry states no `{key}`, \
                         and D3's row for `{}` states `{key}={want}` there. A key \
                         Unity does not find takes the importer's default, which \
                         inside a package is Editor-only (D2). The keys that \
                         entry does state are {:?}.",
                        entry.label(),
                        row.target,
                        entry.settings.keys().collect::<Vec<_>>()
                    ));
                }
                Some(found) if found != want => {
                    return Err(format!(
                        "{meta_name}: the `{}` platform entry sets `{key}: \
                         {found}`, and D3's row for `{}` states `{key}={want}`. \
                         The comparison is byte for byte because casing is what \
                         Unity's enum converter fails on: it substitutes the \
                         default with a warning rather than an error, and for \
                         Android it does not validate the value at all — so the \
                         library is silently absent from the build rather than \
                         reported.",
                        entry.label(),
                        row.target
                    ));
                }
                Some(_) => {}
            }
        }
    }

    // **The row's platform set is exclusive, and this is the half that says so.**
    // The loop above asks whether every platform the row names is enabled and
    // carries the row's keys; on its own that is satisfied by a `.meta` that
    // also enables platforms the row says nothing about — the Android `.so`
    // enabled for the editor, or an arm64 `.dylib` enabled for Windows, which is
    // a library reaching a build D3 never assigned it to and which no key
    // comparison can see. R-E21 is stated over "the platform and CPU D3 assigns
    // to its target", so a platform the row does not name is a declaration the
    // record does not make.
    //
    // Stated after the loop rather than before it so a `.meta` missing an entry
    // the row requires still fails on the missing entry, which is the more
    // useful message of the two.
    if let Some(extra) = entries
        .iter()
        .find(|e| e.enabled && !row.platforms.iter().any(|platform| e.is(platform)))
    {
        return Err(format!(
            "{meta_name}: the `{}` platform entry is enabled, and D3's row for \
             `{}` names {:?} and nothing else. A library enabled for a platform \
             its row does not state reaches a build the record never assigned it \
             to, with every key that row does state perfectly correct.",
            extra.label(),
            row.target,
            row.platforms
                .iter()
                .map(|platform| platform.names[0])
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The shipped binary
//
// **A `.meta` can be byte-perfect over the wrong file.** Replacing the macOS
// `.dylib` with an x86_64 build, with the Android `.so`, or with a zero-length
// file left every check in this repository green, because nothing anywhere
// opened one. What follows reads each file's header for its container and its
// architecture and compares both against the same D3 row its `.meta` is compared
// against — no toolchain, no dependency, the first twenty bytes.
//
// **It is a header comparison and not a freshness check**, which the module note
// states in full: a library of the right shape built from another commit passes
// here, and `DsSlice::stride` in a running host is still what observes that one.
// ---------------------------------------------------------------------------

/// The architectures D3's `CPU` values name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arch {
    Arm64,
    X86_64,
}

/// The container formats D3's file extensions name.
///
/// **Compared as well as the architecture, and the reason is a real mutation.**
/// The Android `.so` copied over the macOS `.dylib` is `arm64` on both sides, so
/// an architecture comparison alone accepts it — and Unity then ships an ELF to
/// a macOS build. The row's `file` cell names the extension, and the extension
/// names the container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    MachO,
    Elf,
}

impl Format {
    /// How a failure names a container.
    fn label(self) -> &'static str {
        match self {
            Format::MachO => "Mach-O",
            Format::Elf => "ELF",
        }
    }
}

impl Arch {
    /// How a failure names an architecture — the header's word rather than
    /// D3's, because D3's casing differs per platform and the message is about
    /// what the file is.
    fn label(self) -> &'static str {
        match self {
            Arch::Arm64 => "arm64",
            Arch::X86_64 => "x86_64",
        }
    }
}

/// `MH_MAGIC_64` as a little-endian file writes it.
const MACH_O_64: [u8; 4] = [0xcf, 0xfa, 0xed, 0xfe];
/// `FAT_MAGIC` and `FAT_MAGIC_64`, which a universal binary carries big-endian.
const MACH_O_UNIVERSAL: [[u8; 4]; 2] = [[0xca, 0xfe, 0xba, 0xbe], [0xca, 0xfe, 0xba, 0xbf]];
/// `\x7fELF`.
const ELF: [u8; 4] = [0x7f, b'E', b'L', b'F'];
/// `CPU_TYPE_ARM | CPU_ARCH_ABI64`, from `<mach/machine.h>`.
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
/// `CPU_TYPE_X86 | CPU_ARCH_ABI64`.
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
/// `EM_AARCH64`, from the ELF psABI.
const EM_AARCH64: u16 = 183;
/// `EM_X86_64`.
const EM_X86_64: u16 = 62;

/// How many bytes of a library are read.
///
/// ELF puts `e_machine` at offset 0x12, the further of the two fields, so twenty
/// bytes answer both formats. The file is opened and this much is read rather
/// than loaded whole: the Android library is 6.5 MB and this runs in the sanity
/// tier on every commit.
const HEADER_BYTES: usize = 20;

/// The architecture D3's row states, with the spelling it states it in.
///
/// **Derived from the row's own `CPU` keys**, so the bytes and the `.meta` are
/// held to one cell of one table and a row whose CPU changes moves both checks
/// together. `None` where the row states no `CPU` at all — D3's iOS row states
/// no key, so there is nothing to hold a `.a` to, and this keeps the same
/// silence the settings comparison keeps over that row.
fn row_arch(row: &Row) -> Result<Option<(Arch, &'static str)>, String> {
    let mut found: Option<(Arch, &'static str)> = None;
    for platform in row.platforms {
        for (key, value) in platform.settings {
            if *key != "CPU" {
                continue;
            }
            // D3 writes `ARM64` for macOS and Android and `x86_64` for the
            // Windows and Linux editor rows, and its prose gives `X86_64` as
            // Android's other value. A spelling that is none of these is refused
            // rather than guessed at: this value decides which binary the gate
            // demands, so reading it wrong is worse than not reading it.
            let arch = match *value {
                "ARM64" => Arch::Arm64,
                "x86_64" | "X86_64" => Arch::X86_64,
                other => {
                    return Err(format!(
                        "D3's row for `{}` states `CPU={other}`, and this gate \
                         knows no binary architecture by that name — so it \
                         cannot say what a library under that row should be.",
                        row.target
                    ));
                }
            };
            match found {
                Some((seen, seen_value)) if seen != arch => {
                    return Err(format!(
                        "D3's row for `{}` states `CPU={seen_value}` for one \
                         platform and `CPU={value}` for another, so the row \
                         names no single architecture for one file.",
                        row.target
                    ));
                }
                _ => found = Some((arch, value)),
            }
        }
    }
    Ok(found)
}

/// The container D3's `file` cell names, by its extension.
///
/// **An extension this gate cannot read is an error rather than a skip, where
/// it is reached at all.** A Windows `.dll` is PE and an iOS `.a` is an `ar`
/// archive; neither is read here, and no row ships one today. The `.dll` reaches
/// this refusal because D3's Windows row states a `CPU`. **The `.a` does not**:
/// D3's iOS row states none, so [`check_binary`] returns before asking for a
/// container at all, and an iOS library would be skipped rather than refused —
/// which is the one hole in this function's own rule. The day a row ships one,
/// this refusal names what that story has to add — where returning "nothing to check" would leave the
/// binary half silently absent for that platform.
fn row_format(row: &Row) -> Result<Format, String> {
    match row.file.rsplit('.').next().unwrap_or("") {
        "dylib" => Ok(Format::MachO),
        "so" => Ok(Format::Elf),
        other => Err(format!(
            "D3's row for `{}` names the file `{}`, and this gate reads Mach-O \
             and ELF only — a `.{other}` is neither.",
            row.target, row.file
        )),
    }
}

/// The container and architecture a library's first bytes declare.
///
/// **Two formats, and everything else refuses.** Mach-O and ELF are what D3's
/// rows that ship carry. A file whose first bytes are neither is refused rather
/// than passed, because a reader that shrugs at a header it does not know is a
/// check that reports nothing about the one class of defect it exists for.
fn read_header(bytes: &[u8]) -> Result<(Format, Arch), String> {
    if bytes.len() < HEADER_BYTES {
        return Err(format!(
            "is {} bytes long, and neither header this gate reads fits in fewer \
             than {HEADER_BYTES}. A truncated or empty library is a package that \
             cannot load with every `.meta` in the tree still perfect.",
            bytes.len()
        ));
    }
    if MACH_O_UNIVERSAL.contains(&[bytes[0], bytes[1], bytes[2], bytes[3]]) {
        return Err(
            "is a universal Mach-O, which carries more than one architecture. \
             D3's row states one `CPU` and Unity's setting names one, so this \
             gate reads a single-architecture library rather than deciding \
             which slice of a fat one a row meant."
                .to_string(),
        );
    }
    if bytes[..4] == MACH_O_64 {
        let cputype = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        return match cputype {
            CPU_TYPE_ARM64 => Ok((Format::MachO, Arch::Arm64)),
            CPU_TYPE_X86_64 => Ok((Format::MachO, Arch::X86_64)),
            other => Err(format!(
                "is a 64-bit Mach-O declaring cputype 0x{other:08x}, which is \
                 neither arm64 (0x{CPU_TYPE_ARM64:08x}) nor x86_64 \
                 (0x{CPU_TYPE_X86_64:08x})."
            )),
        };
    }
    if bytes[..4] == ELF {
        // Refused rather than read at a guess: `e_machine`'s offset and byte
        // order are what the class and data bytes decide, so reading it out of a
        // 32-bit or big-endian file would be reading two other bytes entirely.
        if bytes[4] != 2 || bytes[5] != 1 {
            return Err(format!(
                "is an ELF whose class byte is {} and data byte {}, and this \
                 gate reads 64-bit little-endian ELF (2, 1). D3 assigns no \
                 32-bit or big-endian target.",
                bytes[4], bytes[5]
            ));
        }
        let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
        return match machine {
            EM_AARCH64 => Ok((Format::Elf, Arch::Arm64)),
            EM_X86_64 => Ok((Format::Elf, Arch::X86_64)),
            other => Err(format!(
                "is a 64-bit ELF declaring e_machine {other}, which is neither \
                 aarch64 ({EM_AARCH64}) nor x86-64 ({EM_X86_64})."
            )),
        };
    }
    Err(format!(
        "begins {:02x?}, which is neither a 64-bit Mach-O ({MACH_O_64:02x?}) nor \
         an ELF ({ELF:02x?}). A file this gate cannot recognise is refused \
         rather than passed: it is the shape a library replaced by something \
         else takes, which is exactly what this check is here to see.",
        &bytes[..4]
    ))
}

/// D3's row, against the bytes of the library under it.
///
/// **The row is the same one its `.meta` is compared against**, which is what
/// makes this a check on the pair rather than two opinions. A row that states no
/// `CPU` — D3's iOS row — has nothing here to compare, so this returns without
/// reading the file at all.
fn check_binary(relative: &str, bytes: &[u8], row: &Row) -> Result<(), String> {
    let Some((want_arch, cpu)) = row_arch(row)? else {
        return Ok(());
    };
    let want_format = row_format(row)?;
    let (format, arch) = read_header(bytes).map_err(|why| format!("{relative} {why}"))?;
    if format != want_format {
        return Err(format!(
            "{relative} is {}, and D3's row for `{}` names the file `{}`, which \
             is {}. A library of the right architecture in the wrong container \
             reaches no build on that platform, and every key in the \
             declaration beside it can still be correct.",
            format.label(),
            row.target,
            row.file,
            want_format.label()
        ));
    }
    if arch != want_arch {
        return Err(format!(
            "{relative} is a library for {}, and D3's row for `{}` states \
             `CPU={cpu}`, which is {}. The declaration beside it can carry that \
             CPU perfectly and Unity will still ship these bytes, which is why \
             the two are compared against one row.",
            arch.label(),
            row.target,
            want_arch.label()
        ));
    }
    Ok(())
}

/// The first [`HEADER_BYTES`] of `path`, or fewer if the file is shorter.
fn header_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|why| format!("cannot be opened ({why})."))?;
    let mut buffer = [0u8; HEADER_BYTES];
    let mut filled = 0;
    // `read` is allowed to return fewer bytes than asked for without being at
    // the end, so the short read is looped rather than treated as a short file —
    // which would report a correct library as truncated.
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(why) => return Err(format!("cannot be read ({why}).")),
        }
    }
    Ok(buffer[..filled].to_vec())
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// Every native library under `plugins`, as paths relative to it, sorted.
///
/// Recursive, because Unity's own packages nest — D2 quotes
/// `Runtime/Plugins/Android/<name>/arm64-v8a/` — and a library one directory
/// deeper than this gate expected is exactly the one nobody would notice.
fn shipped_libraries(plugins: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect_libraries(plugins, plugins, &mut out);
    out.sort();
    out
}

fn collect_libraries(plugins: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            collect_libraries(plugins, &path, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !LIBRARY_EXTENSIONS.contains(&ext) {
            continue;
        }
        out.push(
            path.strip_prefix(plugins)
                .expect("a path under the plugins directory")
                .to_string_lossy()
                .into_owned(),
        );
    }
}

/// Every failure R-E21 has over the tree rooted at `plugins`.
///
/// Every library rather than the first, so one run names every file to fix.
fn check_tree(plugins: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    for relative in shipped_libraries(plugins) {
        let row = match row_for(&relative) {
            Ok(row) => row,
            Err(why) => {
                failures.push(why);
                continue;
            }
        };

        let name = relative.rsplit('/').next().unwrap_or(&relative);
        if name != row.file {
            failures.push(format!(
                "{relative} is named `{name}`, and D3's row for `{}` names the \
                 file `{}`. A `[DllImport]` resolves a library by name, so this \
                 is either a library in the wrong directory or one the C# will \
                 not find.",
                row.target, row.file
            ));
        }

        // **The bytes and the declaration are checked independently**, so one
        // run names both when both are wrong. A library of the wrong
        // architecture is invisible to every comparison over the `.meta`, and a
        // `.meta` is what decides whether Unity ships the file at all.
        match header_bytes(&plugins.join(&relative)) {
            Ok(bytes) => {
                if let Err(why) = check_binary(&relative, &bytes, row) {
                    failures.push(why);
                }
            }
            Err(why) => failures.push(format!("{relative} {why}")),
        }

        let meta_name = format!("{relative}.meta");
        let meta = match std::fs::read_to_string(plugins.join(&meta_name)) {
            Ok(meta) => meta,
            Err(why) => {
                failures.push(format!(
                    "{meta_name} cannot be read ({why}). A native library with \
                     no `.meta` beside it takes the Editor-platform defaults \
                     (D2), so it is absent from every player build; R-E2 is what \
                     requires the file to exist at all."
                ));
                continue;
            }
        };

        if let Err(why) = check_meta(&meta_name, &meta, row) {
            failures.push(why);
        }
    }
    failures
}

/// The package's plugin directory.
fn plugins_dir() -> PathBuf {
    package_gate::root()
        .join(package_gate::PACKAGE_PATH)
        .join(PLUGINS_DIR)
}

// ---------------------------------------------------------------------------
// The tests over what the package ships
// ---------------------------------------------------------------------------

/// The package ships exactly the libraries this branch says it does.
///
/// **This is what stops the check below passing over nothing.** Every other
/// assertion here is stated per library found, so an empty or absent
/// `Runtime/Plugins/` satisfies all of them by having nothing to disagree with.
#[test]
fn the_package_ships_exactly_the_native_libraries_this_branch_expects() {
    let dir = plugins_dir();
    assert!(
        dir.is_dir(),
        "{} is not a directory. R-E21 is stated over the libraries the package \
         ships, and this branch ships {:?} — a missing directory is a gate whose \
         input moved, not an empty set.",
        dir.display(),
        SHIPPED
    );

    let found = shipped_libraries(&dir);
    let found: Vec<&str> = found.iter().map(String::as_str).collect();
    assert_eq!(
        found,
        SHIPPED,
        "the native libraries under {} are not the ones this branch ships. A \
         library that is gone is a package that cannot load on that platform; a \
         library that is new is one no row of D3's table was written for.",
        dir.display()
    );
}

/// R-E21: each library's `.meta` declares what D3's row for its target states,
/// and each library's own header carries the architecture that row states.
#[test]
fn every_native_library_the_package_ships_matches_d3s_row() {
    let dir = plugins_dir();
    assert!(
        dir.is_dir(),
        "{} is not a directory, so this check would hold R-E21 over an empty \
         set. The test above is what states which libraries are expected.",
        dir.display()
    );

    let failures = check_tree(&dir);
    assert!(
        failures.is_empty(),
        "R-E21 is not met by the libraries under {}:\n{}",
        dir.display(),
        failures.join("\n")
    );
}

/// [`ROWS`] is D3's table and not a story about it.
///
/// **R-E21's check is "read each `PluginImporter` block against D3's table".**
/// Every other test in this file reads it against [`ROWS`], so without this one
/// the requirement's oracle is a hand copy that nothing compares to the record:
/// deleting the macOS standalone platform's `("CPU", "ARM64")` pair, or the
/// Windows, Linux and iOS rows outright, left the whole file green, and the
/// first of those made a standalone entry saying `CPU: x86_64` acceptable.
///
/// **The comparison is positional**, because the transcription is written in the
/// record's order and a row that moved would otherwise be matched to another
/// row's keys. The settings of one platform are compared as a set: their order
/// inside a cell says nothing, and failing on it would be red for no defect.
#[test]
fn the_transcribed_rows_are_d3s_table() {
    let path = package_gate::root().join(D3_RECORD);
    let record = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("cannot read {}: {why}", path.display()));
    let table = d3_table(&record).unwrap_or_else(|why| panic!("{D3_RECORD} {why}"));

    assert_eq!(
        table.len(),
        ROWS.len(),
        "D3's table states {} rows and this file transcribes {}. A row that is \
         here and not there is a rule nothing states; a row that is there and \
         not here is a target this gate would let through unchecked.",
        table.len(),
        ROWS.len()
    );

    for (found, row) in table.iter().zip(ROWS) {
        assert_eq!(
            found.target, row.target,
            "D3's rows and this file's are compared in order, and they disagree \
             at this position."
        );
        assert_eq!(
            found.file, row.file,
            "D3's row for `{}` names a different file from the one transcribed \
             here.",
            row.target
        );
        assert_eq!(
            found.platforms.len(),
            row.platforms.len(),
            "D3's row for `{}` states {} platforms and this file transcribes \
             {}: {:?} against {:?}.",
            row.target,
            found.platforms.len(),
            row.platforms.len(),
            found
                .platforms
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            row.platforms
                .iter()
                .map(|platform| platform.names[0])
                .collect::<Vec<_>>()
        );

        for ((name, stated), platform) in found.platforms.iter().zip(row.platforms) {
            assert_eq!(
                name.as_str(),
                platform.names[0],
                "D3's row for `{}` names the platform `{name}` where this file \
                 puts `{}` first. The first spelling is D3's own word; the rest \
                 are the build targets the prose under the table gives for it, \
                 and no cell states those.",
                row.target,
                platform.names[0]
            );

            let mut found_settings = stated.clone();
            found_settings.sort();
            let mut want: Vec<(String, String)> = platform
                .settings
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect();
            want.sort();
            assert_eq!(
                found_settings, want,
                "D3's row for `{}` states {found_settings:?} for `{name}` and \
                 this file transcribes {want:?}. The comparison is byte for \
                 byte, because the casing is the requirement.",
                row.target
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The tests over fixtures
//
// The comparison is a function over `&str`, so every shape below is checked
// without a library and without a Unity editor. The tests above are what tie it
// to the files the package actually ships and to the record it measures against.
// ---------------------------------------------------------------------------

/// A macOS `.meta` of the shape Unity 6 writes.
///
/// `serializedVersion: 3`: `platformData` is a mapping keyed by build target,
/// and `OSXUniversal` is what D3 calls `Standalone`. Unity writes a trailing
/// space after the empty values at the end, which is trimmed out of this fixture
/// and which the reader accepts either way.
const MACOS_META: &str = r"fileFormatVersion: 2
guid: 23d0b9332928e403189524b8a89d5298
PluginImporter:
  externalObjects: {}
  serializedVersion: 3
  iconMap: {}
  executionOrder: {}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 1
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
    Any:
      enabled: 0
      settings: {}
    Editor:
      enabled: 1
      settings:
        CPU: ARM64
        DefaultValueInitialized: true
        OS: OSX
    OSXUniversal:
      enabled: 1
      settings:
        CPU: ARM64
  userData:
  assetBundleName:
  assetBundleVariant:
";

/// An Android `.meta` of the shape Unity 6 writes.
///
/// Its row states `CPU` and no `OS`, which is the asymmetry R-E21 names.
const ANDROID_META: &str = r"fileFormatVersion: 2
guid: 8efe1d8edc9e349598cd70b8db0b9f47
PluginImporter:
  externalObjects: {}
  serializedVersion: 3
  iconMap: {}
  executionOrder: {}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 1
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
    Android:
      enabled: 1
      settings:
        CPU: ARM64
        Is16KbAligned: true
    Any:
      enabled: 0
      settings: {}
    Editor:
      enabled: 0
      settings:
        DefaultValueInitialized: true
  userData:
  assetBundleName:
  assetBundleVariant:
";

/// The same macOS row, in the serialization an older editor writes.
///
/// `serializedVersion: 2`: a sequence of `first:`/`second:` entries, where the
/// platform is a group and a build target rather than a target alone, and where
/// `Standalone` — D3's own word — is a spelling that appears on disk. The two
/// pseudo-entries at the top are Unity's own and are carried here because a
/// reader that trips over them reads no real file of this vintage: `: Any` names
/// no group at all, and `Any:` names one with no target. The disabled
/// `Standalone: Win64` is the shape an entry takes when a target has been looked
/// at in the inspector and turned off.
const MACOS_META_SERIALIZED_VERSION_2: &str = r"fileFormatVersion: 2
guid: 23d0b9332928e403189524b8a89d5298
PluginImporter:
  externalObjects: {}
  serializedVersion: 2
  iconMap: {}
  executionOrder: {}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 1
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
  - first:
      : Any
    second:
      enabled: 0
      settings:
        Exclude Android: 1
        Exclude Editor: 0
        Exclude Linux64: 1
        Exclude OSXUniversal: 0
        Exclude Win: 1
        Exclude Win64: 1
  - first:
      Any:
    second:
      enabled: 0
      settings: {}
  - first:
      Editor: Editor
    second:
      enabled: 1
      settings:
        CPU: ARM64
        DefaultValueInitialized: true
        OS: OSX
  - first:
      Standalone: Win64
    second:
      enabled: 0
      settings: {}
  - first:
      Standalone: OSXUniversal
    second:
      enabled: 1
      settings:
        CPU: ARM64
  userData:
  assetBundleName:
  assetBundleVariant:
";

/// A `.meta` Unity writes for a file it has no importer for.
const DEFAULT_IMPORTER_META: &str = r"fileFormatVersion: 2
guid: 4d1a0e3c9b7f4a2e8c5d6f7033333333
DefaultImporter:
  externalObjects: {}
  userData:
  assetBundleName:
  assetBundleVariant:
";

/// `fixture` with its one occurrence of `from` replaced by `to`.
///
/// **The occurrence count is asserted because `str::replace` does not raise.** A
/// fixture edited so that an anchor no longer appears leaves the mutation
/// silently undone, and a test that mutates nothing passes while proving
/// nothing.
fn mutated(fixture: &str, from: &str, to: &str) -> String {
    let count = fixture.matches(from).count();
    assert_eq!(
        count, 1,
        "the fixture carries {count} occurrences of this test's anchor, and a \
         mutation is written against exactly one:\n{from}"
    );
    fixture.replace(from, to)
}

/// The row for a directory, or a panic naming it. Every fixture test starts here
/// so a row deleted from the table fails loudly rather than skipping a test.
fn row(dir: &str) -> &'static Row {
    ROWS.iter()
        .find(|row| row.dir == dir)
        .unwrap_or_else(|| panic!("D3's table, as transcribed here, carries no `{dir}` row"))
}

/// A pass is possible: the macOS row over a macOS `.meta`.
///
/// Both of its platforms are compared — the `Editor` entry for `OS` and `CPU`,
/// and the standalone entry, which this serialization names `OSXUniversal`, for
/// `CPU`.
#[test]
fn a_correct_macos_meta_matches_d3s_row() {
    if let Err(why) = check_meta(
        "macOS/libdashscene_ffi.dylib.meta",
        MACOS_META,
        row("macOS"),
    ) {
        panic!("a correct macOS .meta was rejected: {why}");
    }
}

/// A pass is possible for the row that states one key: the Android row.
#[test]
fn a_correct_android_meta_matches_d3s_row() {
    if let Err(why) = check_meta(
        "Android/libdashscene_ffi.so.meta",
        ANDROID_META,
        row("Android"),
    ) {
        panic!("a correct Android .meta was rejected: {why}");
    }
}

/// The older serialization is read, and reaches the same verdict.
///
/// **Which shape is on disk is not this repository's choice.** A `.meta` is
/// rewritten by whichever editor imports the package, so a reader that
/// understood only the shape Unity 6 writes would refuse a correct file the day
/// someone opened the package in an older editor — and a refusal from a gate
/// that cannot read the file is indistinguishable, at the failure, from the
/// defect it exists to find.
#[test]
fn the_older_serialization_of_the_same_row_is_read_the_same_way() {
    if let Err(why) = check_meta(
        "macOS/libdashscene_ffi.dylib.meta",
        MACOS_META_SERIALIZED_VERSION_2,
        row("macOS"),
    ) {
        panic!("a correct serializedVersion 2 .meta was rejected: {why}");
    }

    // And it is genuinely the other shape rather than a copy of the fixture
    // above, which is what a careless edit would leave behind.
    assert!(
        MACOS_META_SERIALIZED_VERSION_2.contains("  - first:\n      Standalone: OSXUniversal\n"),
        "the fixture no longer carries a `first:`/`second:` sequence entry, so \
         this test no longer reads the older serialization at all."
    );
}

/// The casing is the requirement.
///
/// `arm64` for `ARM64` is the failure R-E21 is written around: Unity's enum
/// converter fails on it, substitutes the default with a warning, and for
/// Android does not validate the value at all — so nothing else in this
/// repository, and nothing in a player build, would report it.
#[test]
fn a_cpu_value_in_the_wrong_case_is_a_failure() {
    const FROM: &str = "        CPU: ARM64\n";
    assert_eq!(
        ANDROID_META.matches(FROM).count(),
        1,
        "the fixture no longer carries exactly one line this test can mutate, so \
         the mutation below would be silent — `str::replace` does not raise."
    );
    let meta = ANDROID_META.replace(FROM, "        CPU: arm64\n");

    let why = check_meta("Android/libdashscene_ffi.so.meta", &meta, row("Android"))
        .expect_err("`arm64` was accepted where D3 states `ARM64`");
    for part in [
        "Android/libdashscene_ffi.so.meta",
        "CPU",
        "arm64",
        "ARM64",
        "Android player, arm64",
    ] {
        assert!(
            why.contains(part),
            "the failure does not name `{part}`, so it does not say what to fix: {why}"
        );
    }
}

/// A key the row states and the `.meta` does not.
#[test]
fn a_key_the_row_states_and_the_meta_omits_is_a_failure() {
    const FROM: &str = "        OS: OSX\n";
    assert_eq!(
        MACOS_META.matches(FROM).count(),
        1,
        "the fixture no longer carries exactly one line this test can mutate."
    );
    let meta = MACOS_META.replace(FROM, "");

    let why = check_meta("macOS/libdashscene_ffi.dylib.meta", &meta, row("macOS"))
        .expect_err("an Editor entry with no `OS` was accepted");
    for part in ["macOS/libdashscene_ffi.dylib.meta", "Editor", "OS", "OSX"] {
        assert!(
            why.contains(part),
            "the failure does not name `{part}`: {why}"
        );
    }
}

/// A key the row does not state is not compared, in either direction.
///
/// This is R-E21's own wording — "the comparison is over the keys the row
/// carries rather than over a fixed pair" — and it has two halves. A key no row
/// states (`DefaultValueInitialized` and `Is16KbAligned`, which Unity writes
/// itself) must not be rejected; and `OS`, which the desktop rows state and the
/// Android row does not, must not be demanded of an Android entry or compared
/// when one carries it anyway.
#[test]
fn a_setting_the_row_does_not_state_is_not_compared() {
    for (fixture, name) in [
        (MACOS_META, "        DefaultValueInitialized: true\n"),
        (ANDROID_META, "        Is16KbAligned: true\n"),
    ] {
        assert!(
            fixture.contains(name),
            "a fixture no longer carries `{name}`, a setting outside D3's rows, \
             so the passing tests above no longer demonstrate that such a key is \
             ignored."
        );
    }

    // The other half: the Android row states `CPU` alone, so an `OS` in that
    // entry — which means nothing to Unity on Android — changes no verdict.
    const FROM: &str = "        CPU: ARM64\n";
    assert_eq!(ANDROID_META.matches(FROM).count(), 1, "one line to mutate");
    let meta = ANDROID_META.replace(FROM, "        CPU: ARM64\n        OS: Windows\n");
    assert_ne!(meta, ANDROID_META, "the mutation changed nothing");

    if let Err(why) = check_meta("Android/libdashscene_ffi.so.meta", &meta, row("Android")) {
        panic!("a key outside D3's Android row was compared: {why}");
    }
}

/// A `.meta` with no `PluginImporter` at all.
///
/// The state a library committed without importing it into an editor is in, and
/// the one D2 names: no PluginImporter, Editor-platform defaults, absent from
/// every player build.
#[test]
fn a_meta_with_no_plugin_importer_block_is_a_failure() {
    let why = check_meta(
        "macOS/libdashscene_ffi.dylib.meta",
        DEFAULT_IMPORTER_META,
        row("macOS"),
    )
    .expect_err("a .meta with no PluginImporter was accepted");
    for part in ["macOS/libdashscene_ffi.dylib.meta", "PluginImporter"] {
        assert!(
            why.contains(part),
            "the failure does not name `{part}`: {why}"
        );
    }
}

/// A `.meta` describing another platform.
///
/// Copying the Android `.meta` onto the `.dylib` leaves a file whose every
/// stated value is correct for the row it was copied from. What it is compared
/// against is the row for the directory the library sits in, so the failure is a
/// missing `Editor` entry rather than a wrong value.
#[test]
fn a_meta_for_another_platform_is_a_failure() {
    let why = check_meta(
        "macOS/libdashscene_ffi.dylib.meta",
        ANDROID_META,
        row("macOS"),
    )
    .expect_err("an Android .meta was accepted for the macOS row");
    for part in ["macOS/libdashscene_ffi.dylib.meta", "Editor"] {
        assert!(
            why.contains(part),
            "the failure does not name `{part}`: {why}"
        );
    }
}

/// A platform entry that is present, correct and turned off.
///
/// Its settings pass the byte comparison and the library still reaches no build,
/// so the entry a row describes is the enabled one or there is none.
#[test]
fn a_disabled_platform_entry_is_a_failure() {
    const FROM: &str = "    Android:\n      enabled: 1\n";
    assert_eq!(
        ANDROID_META.matches(FROM).count(),
        1,
        "the fixture no longer carries the block this test can mutate."
    );
    let meta = ANDROID_META.replace(FROM, "    Android:\n      enabled: 0\n");

    let why = check_meta("Android/libdashscene_ffi.so.meta", &meta, row("Android"))
        .expect_err("a disabled Android entry was accepted");
    assert!(
        why.contains("none of them is enabled"),
        "the failure does not say the entry is disabled: {why}"
    );
}

/// The `Any` platform turned on.
///
/// **Not a key D3 states, and refused anyway.** It is the one addition to
/// R-E21's stated comparison here, because it is about that comparison's own
/// validity rather than about the library: a plugin compatible with any platform
/// is included everywhere, so a wrong `CPU` beneath it has no observable
/// consequence and this gate would pass on a `.meta` whose values decide
/// nothing. The editor half refuses the same state through
/// `GetCompatibleWithAnyPlatform`.
#[test]
fn the_any_platform_turned_on_is_a_failure() {
    const FROM: &str = "    Any:\n      enabled: 0\n";
    assert_eq!(
        MACOS_META.matches(FROM).count(),
        1,
        "the fixture no longer carries the block this test can mutate."
    );
    let meta = MACOS_META.replace(FROM, "    Any:\n      enabled: 1\n");

    let why = check_meta("macOS/libdashscene_ffi.dylib.meta", &meta, row("macOS"))
        .expect_err("an enabled `Any` platform was accepted");
    // `included everywhere` is the `Any` refusal's own words, and the reason to
    // assert on them is that the exclusivity rule below would also reject this
    // file and its message also names `Any`. Without a phrase only one branch
    // writes, deleting the `Any` check would leave this test green.
    for part in [
        "macOS/libdashscene_ffi.dylib.meta",
        "Any",
        "included everywhere",
    ] {
        assert!(
            why.contains(part),
            "the failure does not name `{part}`: {why}"
        );
    }
}

/// `Any` named as a build target rather than as a group.
///
/// **The other half of the `Any` test**, and the branch a reviewer deleted
/// without a test noticing. In `serializedVersion: 2` Unity writes a
/// pseudo-entry whose group is empty and whose target is `Any`, so a reader
/// checking only the group name walks past it; the file is then compatible with
/// every platform and every value below it decides nothing.
#[test]
fn the_any_platform_named_as_a_build_target_is_a_failure() {
    let meta = mutated(
        MACOS_META_SERIALIZED_VERSION_2,
        "      : Any\n    second:\n      enabled: 0\n",
        "      : Any\n    second:\n      enabled: 1\n",
    );

    let why = check_meta("macOS/libdashscene_ffi.dylib.meta", &meta, row("macOS"))
        .expect_err("an enabled `: Any` entry was accepted");
    assert!(
        why.contains("included everywhere"),
        "the failure is not the `Any` refusal, so the target half of that test \
         is pinned by nothing: {why}"
    );
}

/// A group entry the row names, for a build target it does not.
///
/// **`Standalone` covers three targets and D3's macOS row means one of them.**
/// A `serializedVersion: 2` `.meta` with `Standalone: Win64` enabled and
/// carrying `CPU: ARM64`, beside a disabled `Standalone: OSXUniversal`, states
/// every key the row states — and ships an arm64 library to Windows and nothing
/// at all to macOS. Matching an entry on its group alone accepted it.
#[test]
fn a_group_entry_for_a_target_the_row_does_not_name_is_not_the_rows_entry() {
    let meta = mutated(
        MACOS_META_SERIALIZED_VERSION_2,
        "      Standalone: Win64\n    second:\n      enabled: 0\n      settings: {}\n",
        "      Standalone: Win64\n    second:\n      enabled: 1\n      settings:\n        CPU: ARM64\n",
    );
    let meta = mutated(
        &meta,
        "      Standalone: OSXUniversal\n    second:\n      enabled: 1\n      settings:\n        CPU: ARM64\n",
        "      Standalone: OSXUniversal\n    second:\n      enabled: 0\n      settings: {}\n",
    );

    let why = check_meta("macOS/libdashscene_ffi.dylib.meta", &meta, row("macOS"))
        .expect_err("an enabled `Standalone: Win64` stood in for the macOS standalone entry");
    for part in ["Standalone", "none of them is enabled"] {
        assert!(
            why.contains(part),
            "the failure does not name `{part}`, so it does not say that the \
             macOS standalone entry is the one that is off: {why}"
        );
    }
}

/// A platform the row does not name, enabled.
///
/// **The row's platform set is exclusive.** Both cases here pass every key
/// comparison in the file and put the library into a build D3 never assigned it
/// to: the Android `.so` enabled for the editor, and the arm64 `.dylib` enabled
/// for a Windows standalone. Neither is visible to any check over the keys a row
/// states, because neither changes one.
#[test]
fn a_platform_the_row_does_not_name_may_not_be_enabled() {
    let editor = mutated(
        ANDROID_META,
        "    Editor:\n      enabled: 0\n",
        "    Editor:\n      enabled: 1\n",
    );
    let why = check_meta("Android/libdashscene_ffi.so.meta", &editor, row("Android"))
        .expect_err("the Android library enabled for the editor was accepted");
    assert!(
        why.contains("Editor") && why.contains("enabled"),
        "the failure does not name the editor entry it should refuse: {why}"
    );

    let windows = mutated(
        MACOS_META,
        "    OSXUniversal:\n      enabled: 1\n      settings:\n        CPU: ARM64\n",
        "    OSXUniversal:\n      enabled: 1\n      settings:\n        CPU: ARM64\n\
         \n    Win64:\n      enabled: 1\n      settings:\n        CPU: ARM64\n",
    );
    let why = check_meta("macOS/libdashscene_ffi.dylib.meta", &windows, row("macOS"))
        .expect_err("the macOS library enabled for a Windows standalone was accepted");
    assert!(
        why.contains("Win64"),
        "the failure does not name the Windows entry it should refuse: {why}"
    );
}

/// Two enabled entries where D3's row states one set of settings.
///
/// A row states one line of keys per platform, and this package ships one file
/// per target, so two enabled entries answering to one row platform leave no
/// answer to which one the row describes. Picking either would be a guess this
/// gate has no basis for.
#[test]
fn two_enabled_entries_for_one_row_platform_are_refused() {
    let meta = mutated(
        MACOS_META,
        "    OSXUniversal:\n      enabled: 1\n      settings:\n        CPU: ARM64\n",
        "    OSXUniversal:\n      enabled: 1\n      settings:\n        CPU: ARM64\n\
         \n    Standalone:\n      enabled: 1\n      settings:\n        CPU: ARM64\n",
    );

    let why = check_meta("macOS/libdashscene_ffi.dylib.meta", &meta, row("macOS"))
        .expect_err("two enabled entries for D3's `Standalone` platform were accepted");
    assert!(
        why.contains("2 enabled `Standalone` platform entries"),
        "the failure does not say that two entries answer to one row platform: \
         {why}"
    );
}

/// Every shape the reader refuses, refused.
///
/// **"It refuses what it does not understand" was one tested branch.** Each
/// of the mutations below turned a refusal into an acceptance — a plain insert
/// where a duplicate key is caught, `_ => {}` where an unrecognised line is,
/// `(content, "")` where [`split_pair`] errors — and every one of them left this
/// file green. A reader that quietly skips a line it cannot classify reports an
/// empty platform set, which is the same fail-open as an empty directory reached
/// from inside the file, so each branch gets a fixture that dies without it.
///
/// One error in [`plugin_platforms`] is deliberately absent from this table, and
/// the reason is written out so a reader can check it rather than take it: the
/// "before any entry begins" arm needs an empty `entries` at a line that opens
/// no entry, and both branches that can leave that state — the one that sets
/// `sequence` and the one that sets `key_indent` — push an entry in the same
/// step. So the first line under `platformData:` always opens one, and no input
/// reaches that arm. It stays in place as a refusal, and no fixture is claimed
/// for it.
#[test]
fn every_shape_the_reader_refuses_is_refused() {
    let cases: Vec<(&str, String, &str, &'static Row)> = vec![
        (
            "a settings key stated twice",
            mutated(
                ANDROID_META,
                "        CPU: ARM64\n",
                "        CPU: ARM64\n        CPU: X86_64\n",
            ),
            "sets `CPU` twice",
            row("Android"),
        ),
        (
            "a key inside an entry that is neither `enabled` nor `settings`",
            mutated(
                ANDROID_META,
                "    Android:\n      enabled: 1\n",
                "    Android:\n      enabled: 1\n      isPreloaded: 0\n",
            ),
            "carries `isPreloaded: 0` inside a platformData entry",
            row("Android"),
        ),
        (
            "a line inside an entry that is not `key: value`",
            mutated(
                ANDROID_META,
                "    Android:\n      enabled: 1\n",
                "    Android:\n      enabled: 1\n      nonsense\n",
            ),
            "is not a `key: value` line",
            row("Android"),
        ),
        (
            "a third value for `enabled`",
            mutated(ANDROID_META, "      enabled: 1\n", "      enabled: 2\n"),
            "will not guess what a third value means",
            row("Android"),
        ),
        (
            "an entry that states no `enabled`",
            mutated(
                ANDROID_META,
                "    Android:\n      enabled: 1\n",
                "    Android:\n",
            ),
            "with no `enabled:` key",
            row("Android"),
        ),
        (
            "an entry written on one line",
            mutated(ANDROID_META, "    Android:\n", "    Android: Editor\n"),
            "on one line",
            row("Android"),
        ),
        (
            "a `platformData` holding no entry",
            mutated(MACOS_META, "  platformData:\n", "  platformData: {}\n"),
            "carrying no entry at all",
            row("macOS"),
        ),
        (
            "a `PluginImporter` with no `platformData` at all",
            mutated(MACOS_META, "  platformData:\n", ""),
            "declares no `platformData:`",
            row("macOS"),
        ),
        (
            "a key beside `first:` and `second:`",
            mutated(
                MACOS_META_SERIALIZED_VERSION_2,
                "      Editor: Editor\n    second:\n",
                "      Editor: Editor\n    third:\n    second:\n",
            ),
            "beside `first:` and `second:`",
            row("macOS"),
        ),
        (
            "two platforms under one `first:` mapping",
            mutated(
                MACOS_META_SERIALIZED_VERSION_2,
                "      Editor: Editor\n",
                "      Editor: Editor\n      Standalone: OSXUniversal\n",
            ),
            "more than one line under a `first:` mapping",
            row("macOS"),
        ),
        (
            "a `first:` mapping naming no platform",
            mutated(
                MACOS_META_SERIALIZED_VERSION_2,
                "      Editor: Editor\n",
                "",
            ),
            "`first:` mapping is empty",
            row("macOS"),
        ),
    ];

    for (shape, meta, needle, row) in cases {
        let name = format!("{}/{}.meta", row.dir, row.file);
        let Err(why) = check_meta(&name, &meta, row) else {
            panic!(
                "{shape}: this shape was accepted, so the reader's refusal of it \
                 is pinned by nothing."
            );
        };
        assert!(
            why.contains(needle),
            "{shape}: the failure does not say `{needle}`, so a different branch \
             answered and this one is still unpinned: {why}"
        );
    }
}

/// A library carrying another row's file name.
///
/// A `[DllImport]` resolves a library by name, so a file under the right
/// directory with the wrong name is a package whose C# finds nothing. The check
/// is one line and was disabled by a reviewer with `if false &&` without a test
/// noticing.
#[test]
fn a_library_named_for_another_rows_file_is_a_failure() {
    let dir = temp_root();
    std::fs::create_dir_all(dir.join("macOS")).expect("the fixture's macOS directory");
    write(&dir, "macOS/dashscene_ffi.dylib", mach_o(CPU_TYPE_ARM64));
    write(&dir, "macOS/dashscene_ffi.dylib.meta", MACOS_META);

    let failures = check_tree(&dir);
    std::fs::remove_dir_all(&dir).expect("the fixture is removable");

    assert_eq!(
        failures.len(),
        1,
        "one library was misnamed and {} failures were reported: {}",
        failures.len(),
        failures.join("\n")
    );
    for part in [
        "macOS/dashscene_ffi.dylib",
        "libdashscene_ffi.dylib",
        "macOS editor + standalone, arm64",
    ] {
        assert!(
            failures[0].contains(part),
            "the failure does not name `{part}`: {}",
            failures[0]
        );
    }
}

/// A library of the shape its row states passes.
///
/// The positive control for the header comparison: without it, a check that
/// refused everything would look identical to a check that works.
#[test]
fn a_library_of_the_shape_its_row_states_passes() {
    for (dir, bytes) in [
        ("macOS", mach_o(CPU_TYPE_ARM64)),
        ("Android", elf(EM_AARCH64)),
    ] {
        let row = row(dir);
        let name = format!("{dir}/{}", row.file);
        if let Err(why) = check_binary(&name, &bytes, row) {
            panic!("a correct {dir} library was rejected: {why}");
        }
    }
}

/// A library whose header does not match the row its `.meta` is compared to.
///
/// **Every one of these survived the whole suite before the header was read.**
/// The `.meta` beside such a file can be byte-perfect: it is a statement about a
/// path, and nothing in it is a statement about the bytes at that path.
#[test]
fn a_library_whose_header_contradicts_its_row_is_a_failure() {
    for (shape, dir, bytes, needle) in [
        (
            "an x86_64 build under the macOS row",
            "macOS",
            mach_o(CPU_TYPE_X86_64),
            "x86_64",
        ),
        (
            "the Android library copied over the macOS one",
            "macOS",
            elf(EM_AARCH64),
            "ELF",
        ),
        (
            "an x86-64 build under the Android row",
            "Android",
            elf(EM_X86_64),
            "x86_64",
        ),
        (
            "the macOS library copied over the Android one",
            "Android",
            mach_o(CPU_TYPE_ARM64),
            "Mach-O",
        ),
        ("a zero-length file", "macOS", Vec::new(), "0 bytes long"),
        (
            "a universal Mach-O",
            "macOS",
            vec![
                0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            "universal Mach-O",
        ),
        (
            "a file that is no library at all",
            "macOS",
            b"not a library at all, just text".to_vec(),
            "neither a 64-bit Mach-O",
        ),
    ] {
        let row = row(dir);
        let name = format!("{dir}/{}", row.file);
        let Err(why) = check_binary(&name, &bytes, row) else {
            panic!("{shape}: accepted, so the header comparison does not see it");
        };
        assert!(
            why.contains(needle) && why.contains(&name),
            "{shape}: the failure does not say `{needle}` about `{name}`: {why}"
        );
    }
}

/// A library the table has no row for.
///
/// Both shapes: a directory D3 does not name, and no directory at all. Neither
/// may be skipped — a library the gate cannot classify is the one nobody wrote a
/// `.meta` rule for, which is D2's silent Editor-only default.
#[test]
fn a_library_the_table_has_no_row_for_is_a_failure() {
    let why = row_for("Frobnicate/libdashscene_ffi.so")
        .expect_err("a library under an unknown directory was accepted");
    assert!(why.contains("Frobnicate"), "{why}");

    let why = row_for("libdashscene_ffi.so")
        .expect_err("a library in no per-target directory was accepted");
    assert!(why.contains("per-target directory"), "{why}");
}

/// The walk and the comparison, end to end over files.
///
/// **The two tests over the package exercise only the passing case**, and only
/// for as long as the package keeps passing. This builds the tree the gate
/// expects, checks it clean, then corrupts it two ways and requires each to be
/// named — so the walk, the sibling `.meta` lookup and the comparison are known
/// to be connected to each other rather than merely present.
///
/// The libraries themselves are the first bytes of a header and nothing more,
/// which is all [`check_binary`] reads. A third corruption swaps one of them for
/// the other architecture, so the byte half of the walk is connected here too.
#[test]
fn the_walk_and_the_comparison_run_end_to_end_over_files() {
    let dir = temp_root();
    std::fs::create_dir_all(dir.join("macOS")).expect("the fixture's macOS directory");
    std::fs::create_dir_all(dir.join("Android")).expect("the fixture's Android directory");
    write(&dir, "macOS/libdashscene_ffi.dylib", mach_o(CPU_TYPE_ARM64));
    write(&dir, "macOS/libdashscene_ffi.dylib.meta", MACOS_META);
    write(&dir, "Android/libdashscene_ffi.so", elf(EM_AARCH64));
    write(&dir, "Android/libdashscene_ffi.so.meta", ANDROID_META);

    let found = shipped_libraries(&dir);
    let clean = check_tree(&dir);

    write(
        &dir,
        "Android/libdashscene_ffi.so.meta",
        ANDROID_META.replace("CPU: ARM64", "CPU: arm64"),
    );
    let corrupted = check_tree(&dir);

    write(&dir, "Android/libdashscene_ffi.so.meta", ANDROID_META);
    write(
        &dir,
        "macOS/libdashscene_ffi.dylib",
        mach_o(CPU_TYPE_X86_64),
    );
    let wrong_architecture = check_tree(&dir);

    write(&dir, "macOS/libdashscene_ffi.dylib", mach_o(CPU_TYPE_ARM64));
    std::fs::remove_file(dir.join("macOS/libdashscene_ffi.dylib.meta"))
        .expect("the .meta is there to remove");
    let orphaned = check_tree(&dir);

    // **Removed before the assertions, not after them.** A failing assertion
    // leaves through a panic, so a cleanup written below one runs on every pass
    // and on none of the failures — which are the runs someone repeats.
    std::fs::remove_dir_all(&dir).expect("the fixture is removable");

    let found: Vec<&str> = found.iter().map(String::as_str).collect();
    assert_eq!(
        found, SHIPPED,
        "the walk did not find the two libraries the fixture wrote, so the set \
         comparison the package tests make is exercised by nothing."
    );
    assert!(
        clean.is_empty(),
        "a correct tree was rejected: {}",
        clean.join("\n")
    );

    assert_eq!(
        corrupted.len(),
        1,
        "one `.meta` was corrupted and {} failures were reported: {}",
        corrupted.len(),
        corrupted.join("\n")
    );
    for part in ["Android/libdashscene_ffi.so.meta", "CPU", "arm64", "ARM64"] {
        assert!(
            corrupted[0].contains(part),
            "the failure does not name `{part}`: {}",
            corrupted[0]
        );
    }

    assert_eq!(
        wrong_architecture.len(),
        1,
        "one library was replaced with an x86_64 build and {} failures were \
         reported: {}",
        wrong_architecture.len(),
        wrong_architecture.join("\n")
    );
    for part in [
        "macOS/libdashscene_ffi.dylib",
        "x86_64",
        "arm64",
        "macOS editor + standalone, arm64",
    ] {
        assert!(
            wrong_architecture[0].contains(part),
            "the failure does not name `{part}`: {}",
            wrong_architecture[0]
        );
    }
    assert!(
        !wrong_architecture[0].contains("libdashscene_ffi.dylib.meta"),
        "the failure names the `.meta` rather than the library, so the wrong \
         half of the walk answered: {}",
        wrong_architecture[0]
    );

    assert_eq!(
        orphaned.len(),
        1,
        "one `.meta` was deleted and {} failures were reported: {}",
        orphaned.len(),
        orphaned.join("\n")
    );
    assert!(
        orphaned[0].contains("macOS/libdashscene_ffi.dylib.meta"),
        "the failure does not name the library whose `.meta` is gone: {}",
        orphaned[0]
    );
}

/// A record carrying D3's heading and one row of its table.
///
/// The shape of the real record rather than a copy of it: the row is the macOS
/// one, which is the only row that states two platforms and the only one whose
/// two `CPU` values have to agree.
const D3_FIXTURE: &str = "Some prose above the decision.

**D3 — the per-platform matrix.**

| target | crate type | file | `.meta` must set |
| ------ | ---------- | ---- | ---------------- |
| macOS editor + standalone, arm64 | `cdylib` | `libdashscene_ffi.dylib` | \
Editor `OS=OSX` `CPU=ARM64`; Standalone `CPU=ARM64` |

Some prose below it.
";

/// The oracle's own parser reads a table it recognises.
#[test]
fn the_d3_parser_reads_a_row_of_the_shape_the_record_writes() {
    let table = d3_table(D3_FIXTURE).unwrap_or_else(|why| panic!("the fixture was refused: {why}"));
    assert_eq!(
        table,
        vec![RecordRow {
            target: "macOS editor + standalone, arm64".to_string(),
            file: "libdashscene_ffi.dylib".to_string(),
            platforms: vec![
                (
                    "Editor".to_string(),
                    vec![
                        ("OS".to_string(), "OSX".to_string()),
                        ("CPU".to_string(), "ARM64".to_string()),
                    ],
                ),
                (
                    "Standalone".to_string(),
                    vec![("CPU".to_string(), "ARM64".to_string())],
                ),
            ],
        }]
    );
}

/// Every shape the oracle's parser refuses, refused.
///
/// **A parser that skips what it does not understand compares nothing, in
/// silence.** This one is the source of R-E21's oracle, so a row it walked past
/// would take the rule that row states with it — and the comparison against
/// [`ROWS`] would still pass, over the subset that happened to parse. Each shape
/// below therefore has to reach an error, and [`the_transcribed_rows_are_d3s_table`]
/// panics on one.
///
/// **The name says every shape and the cases are not every branch.** Nine are
/// covered here. Six are not: a missing table, a header with no rows, the
/// header-rule line, a row naming no target, a cell outside a `|`-delimited
/// row, and a clause carrying more than one `=`. Each of those is reached by
/// a malformed record rather than by a wrong one, so they fail loudly on the
/// first run against such a file; they are named here so the gap is a
/// decision rather than an oversight.
#[test]
fn every_shape_the_d3_parser_refuses_is_refused() {
    let cases = [
        (
            "no D3 heading at all",
            D3_FIXTURE.replace(
                "**D3 — the per-platform matrix.**",
                "**D3 — something else.**",
            ),
            "carries no line reading",
        ),
        (
            "a column this parser does not know",
            D3_FIXTURE.replace("| crate type |", "| crate kind |"),
            "states the columns",
        ),
        (
            "a row with a cell missing",
            D3_FIXTURE.replace(
                "| `cdylib` | `libdashscene_ffi.dylib` |",
                "| `libdashscene_ffi.dylib` |",
            ),
            "cells where D3's table carries 4",
        ),
        (
            "a file cell that is not quoted",
            D3_FIXTURE.replace("| `libdashscene_ffi.dylib` |", "| libdashscene_ffi.dylib |"),
            "not backtick-quoted",
        ),
        (
            "prose between two keys",
            D3_FIXTURE.replace("`OS=OSX` `CPU=ARM64`", "`OS=OSX` and perhaps `CPU=ARM64`"),
            "where a backtick-quoted item belongs",
        ),
        (
            "prose after a comma carrying a key",
            D3_FIXTURE.replace(
                "Standalone `CPU=ARM64` |",
                "Standalone `CPU=ARM64`, and also `OS=OSX` |",
            ),
            "after a comma",
        ),
        (
            "a platform named in more than one word",
            D3_FIXTURE.replace("Editor `OS=OSX`", "Editor and Standalone `OS=OSX`"),
            "which is not one word",
        ),
        (
            "a quoted item that states no value",
            D3_FIXTURE.replace("Editor `OS=OSX`", "Editor `OS`"),
            "not a `KEY=VALUE` pair",
        ),
        (
            "an unbalanced backtick",
            D3_FIXTURE.replace("`CPU=ARM64`; Standalone", "`CPU=ARM64; Standalone"),
            "unbalanced backtick",
        ),
    ];

    for (shape, record, needle) in cases {
        assert_ne!(
            record, D3_FIXTURE,
            "{shape}: the fixture no longer carries what this case replaces, so \
             the case is parsing the unmutated table."
        );
        let Err(why) = d3_table(&record) else {
            panic!(
                "{shape}: parsed, so a table of this shape would be compared as if it were D3's"
            );
        };
        assert!(
            why.contains(needle),
            "{shape}: the refusal does not say `{needle}`: {why}"
        );
    }
}

/// A directory of this run's own, under the system temporary directory.
///
/// The process id is not enough on its own: two runs of this test can overlap,
/// and a shared directory would have one run deleting the other's fixture
/// halfway through.
fn temp_root() -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dashscene-plugin-meta-{}-{now}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    dir
}

/// Takes bytes rather than text because the libraries a tree fixture writes are
/// headers, and the `.meta` beside them is a string.
fn write(dir: &Path, relative: &str, content: impl AsRef<[u8]>) {
    let path = dir.join(relative);
    std::fs::write(&path, content)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}

/// The first bytes of a 64-bit little-endian Mach-O declaring `cputype`.
///
/// **A header and not a library.** Nothing in this file loads what it reads, so
/// a fixture needs exactly the fields [`read_header`] looks at; building a
/// loadable `.dylib` would need a toolchain, which is the dependency this check
/// was written to avoid.
fn mach_o(cputype: u32) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_BYTES + 12];
    out[..4].copy_from_slice(&MACH_O_64);
    out[4..8].copy_from_slice(&cputype.to_le_bytes());
    out
}

/// The first bytes of a 64-bit little-endian ELF declaring `e_machine`.
fn elf(machine: u16) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_BYTES + 12];
    out[..4].copy_from_slice(&ELF);
    out[4] = 2; // ELFCLASS64
    out[5] = 1; // ELFDATA2LSB
    out[18..20].copy_from_slice(&machine.to_le_bytes());
    out
}
