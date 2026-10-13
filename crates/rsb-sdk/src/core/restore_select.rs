//! Selective restore: which manifest entries to restore (`rsb restore
//! --files`) and which snapshot to restore from (`rsb restore --date`).
//!
//! Used by [`crate::core::restore::restore_from_storage`] through
//! [`crate::RestoreOptions::include`] and
//! [`crate::RestoreOptions::at_or_before`]; the CLI and the desktop app only
//! parse user input with [`FileSelector::new`] and [`parse_restore_date`].
//!
//! # File patterns
//!
//! Patterns are matched against manifest keys: paths relative to the backup
//! source, components separated by `/` on every platform. A pattern is one
//! of:
//!
//! * an exact path: `docs/report.pdf`;
//! * a directory: `docs` or `docs/` restores everything under `docs/`
//!   (`docs` also matches a file named `docs`, `docs/` only a directory);
//! * a glob (`*`, `?`, `[...]`, `**`, see the `glob` crate): `docs/*.pdf`,
//!   `**/*.jpg`. Globs are anchored at the backup root, `*` and `?` never
//!   match `/` and `**` matches any number of directories, so `*.txt` only
//!   matches top-level files and `**/*.txt` matches them at any depth. A glob
//!   matching a directory selects everything under it (`photos/20*`).
//!
//! Matching is case sensitive. Unicode is compared after normalizing both
//! the pattern and the manifest key to NFC, so `café` typed in NFC (as on
//! Linux and Windows) selects a `café` stored in NFD (as macOS file systems
//! report it) and vice versa; the file is still restored under the name
//! stored in the manifest. `\` is read as `/`, `.` components and repeated `/`
//! are ignored. Absolute paths, drive letters and `..` components are
//! rejected. A pattern containing glob characters also matches the entry
//! spelled exactly like it, so `photo[1].jpg` restores a file of that name.
//!
//! # Dates
//!
//! [`parse_restore_date`] accepts RFC 3339 (`2026-10-01T12:00:00Z`, exact
//! instant) or a local time: `YYYY-MM-DD`, `YYYY-MM-DD HH:MM` and
//! `YYYY-MM-DD HH:MM:SS` (`T` may replace the space). A local time covers the
//! whole day, minute or second it names: `2026-10-01` means up to the end of
//! that day (23:59:59.999999999) in the local time zone of the machine, and
//! `2026-10-01 12:00` up to 12:00:59.999999999. The latest snapshot taken at
//! or before that instant is restored (see [`select_snapshot_at_or_before`]).

use super::manifest::{parse_snapshot_timestamp, sort_snapshots_newest_first};
use crate::config::Config;
use crate::error::RsbError;
use crate::storage::Storage;
use chrono::{
    DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc,
};
use glob::{MatchOptions, Pattern};
use std::path::PathBuf;
use tracing::warn;
use unicode_normalization::UnicodeNormalization;

const GLOB_CHARS: [char; 3] = ['*', '?', '['];

const GLOB_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// One validated `--files` pattern (see the [module docs](self)).
#[derive(Debug, Clone)]
pub struct FilePattern {
    raw: String,
    /// Normalized form: NFC, `/`-separated, no `.`/empty components.
    path: String,
    glob: Option<Pattern>,
    /// Written with a trailing `/`: only matches what is under it.
    dir_only: bool,
}

impl FilePattern {
    /// Parses and validates one user-supplied pattern.
    pub fn new(raw: &str) -> Result<Self, RsbError> {
        let invalid = |why: &str| {
            RsbError::InvalidInput(format!("Invalid --files pattern '{}': {}", raw, why))
        };
        if raw.trim().is_empty() {
            return Err(invalid("empty pattern"));
        }
        if raw.contains('\0') {
            return Err(invalid("contains a NUL character"));
        }
        let s: String = raw.replace('\\', "/").nfc().collect();
        let b = s.as_bytes();
        if s.starts_with('/') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':') {
            return Err(invalid(
                "absolute paths are not allowed; use a path relative to the backup source (e.g. docs/report.pdf)",
            ));
        }
        let mut parts = Vec::new();
        for comp in s.split('/') {
            match comp {
                "" | "." => {}
                ".." => return Err(invalid("'..' components are not allowed")),
                c => parts.push(c),
            }
        }
        if parts.is_empty() {
            return Err(invalid(
                "matches the whole snapshot; omit --files to restore everything",
            ));
        }
        let path = parts.join("/");
        let glob = if path.contains(GLOB_CHARS) {
            Some(Pattern::new(&path).map_err(|e| invalid(&e.to_string()))?)
        } else {
            None
        };
        Ok(Self {
            raw: raw.to_string(),
            path,
            glob,
            dir_only: s.ends_with('/'),
        })
    }

    /// The pattern as the user wrote it.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Whether the manifest key `key` (`/`-separated) is selected.
    pub fn matches(&self, key: &str) -> bool {
        self.matches_nfc(&nfc_key(key))
    }

    /// [`Self::matches`] for a key already passed through [`nfc_key`].
    fn matches_nfc(&self, key: &str) -> bool {
        // The entry itself, then each of its parent directories.
        let whole = (!self.dir_only).then_some(key);
        let parents = key.match_indices('/').map(|(i, _)| &key[..i]);
        whole
            .into_iter()
            .chain(parents)
            .any(|candidate| self.matches_one(candidate))
    }

    fn matches_one(&self, candidate: &str) -> bool {
        candidate == self.path
            || self
                .glob
                .as_ref()
                .is_some_and(|g| g.matches_with(candidate, GLOB_OPTIONS))
    }
}

/// `key` with `/` separators, normalized to NFC (the form patterns are
/// stored in), so canonically equivalent names compare equal.
fn nfc_key(key: &str) -> String {
    key.replace('\\', "/").nfc().collect()
}

/// A set of `--files` patterns; an empty selector selects every entry.
#[derive(Debug, Clone, Default)]
pub struct FileSelector {
    patterns: Vec<FilePattern>,
}

impl FileSelector {
    /// Validates every pattern (see [`FilePattern::new`]).
    pub fn new<I, S>(patterns: I) -> Result<Self, RsbError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let patterns = patterns
            .into_iter()
            .map(|p| FilePattern::new(p.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { patterns })
    }

    /// `true` when no pattern was given (the whole snapshot is selected).
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// The validated patterns.
    pub fn patterns(&self) -> &[FilePattern] {
        &self.patterns
    }

    /// Whether `key` is selected (always `true` for an empty selector).
    pub fn matches(&self, key: &str) -> bool {
        if self.is_empty() {
            return true;
        }
        let key = nfc_key(key);
        self.patterns.iter().any(|p| p.matches_nfc(&key))
    }

    /// Keeps the selected entries of `snapshot`.
    ///
    /// Errors with [`RsbError::NotFound`] when patterns were given and none
    /// of them matches an entry, so a selective restore never succeeds
    /// without restoring anything. Patterns that match nothing while others
    /// do are logged as warnings.
    pub fn select<T>(
        &self,
        entries: Vec<(PathBuf, T)>,
        snapshot: &str,
    ) -> Result<Vec<(PathBuf, T)>, RsbError> {
        if self.is_empty() {
            return Ok(entries);
        }
        let mut used = vec![false; self.patterns.len()];
        let selected: Vec<(PathBuf, T)> = entries
            .into_iter()
            .filter(|(path, _)| {
                // Matched in NFC; the entry keeps its stored name.
                let key = nfc_key(&path.to_string_lossy());
                let mut hit = false;
                for (p, used) in self.patterns.iter().zip(used.iter_mut()) {
                    if p.matches_nfc(&key) {
                        *used = true;
                        hit = true;
                    }
                }
                hit
            })
            .collect();
        let unmatched: Vec<&str> = self
            .patterns
            .iter()
            .zip(&used)
            .filter(|(_, used)| !**used)
            .map(|(p, _)| p.as_str())
            .collect();
        if selected.is_empty() {
            return Err(RsbError::NotFound(format!(
                "No file in snapshot {} matches --files {}; nothing was restored.\n\
                 Patterns are relative to the backup source and use '/' (e.g. docs/report.pdf, docs/, '**/*.txt').",
                snapshot,
                quoted(&unmatched)
            )));
        }
        for p in unmatched {
            warn!("--files pattern '{}' matched no file in {}", p, snapshot);
        }
        Ok(selected)
    }
}

fn quoted(patterns: &[&str]) -> String {
    patterns
        .iter()
        .map(|p| format!("'{}'", p))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parses a `--date` value in the local time zone (see the
/// [module docs](self) for the accepted formats).
pub fn parse_restore_date(input: &str) -> Result<DateTime<Utc>, RsbError> {
    parse_restore_date_in(input, &Local)
}

/// Like [`parse_restore_date`], with local times read in `tz`.
pub fn parse_restore_date_in<Tz: TimeZone>(
    input: &str,
    tz: &Tz,
) -> Result<DateTime<Utc>, RsbError> {
    let s = input.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Ok(t.with_timezone(&Utc));
    }
    let end_of = |naive: NaiveDateTime, span: Duration| naive + span - Duration::nanoseconds(1);
    let naive = if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        end_of(d.and_time(chrono::NaiveTime::MIN), Duration::days(1))
    } else if let Some(n) = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
    {
        end_of(n, Duration::seconds(1))
    } else if let Some(n) = ["%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
    {
        end_of(n, Duration::minutes(1))
    } else {
        return Err(RsbError::InvalidInput(format!(
            "Invalid --date '{}': expected RFC 3339 (2026-10-01T12:00:00Z), \
             YYYY-MM-DD (end of that day, local time) or YYYY-MM-DD HH:MM (local time)",
            input
        )));
    };
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(t) => Ok(t.with_timezone(&Utc)),
        // Repeated hour (DST end): the later instant, so the range is inclusive.
        LocalResult::Ambiguous(_, later) => Ok(later.with_timezone(&Utc)),
        LocalResult::None => Err(RsbError::InvalidInput(format!(
            "Invalid --date '{}': this local time does not exist (daylight saving change); \
             use RFC 3339 with an explicit offset",
            input
        ))),
    }
}

/// Bare snapshot id of a snapshot path (`snapshots/<id>.toml` → `<id>`).
pub fn snapshot_id_of(path: &str) -> &str {
    let name = path.strip_prefix("snapshots/").unwrap_or(path);
    name.strip_suffix(".toml").unwrap_or(name)
}

fn display_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Picks, among snapshot paths/names, the latest one taken at or before
/// `when` (a snapshot taken exactly at `when` qualifies). Snapshots taken
/// in the same instant are ordered by their collision suffix, as in
/// [`sort_snapshots_newest_first`]. Names without a timestamp are ignored.
///
/// Errors with [`RsbError::NotFound`], naming the oldest snapshot, when every
/// snapshot is newer than `when`.
pub fn select_snapshot_at_or_before(
    snapshots: &[String],
    when: DateTime<Utc>,
) -> Result<String, RsbError> {
    let mut dated: Vec<String> = snapshots
        .iter()
        .filter(|s| parse_snapshot_timestamp(s).is_some())
        .cloned()
        .collect();
    if dated.is_empty() {
        return Err(RsbError::NotFound("No snapshots found".to_string()));
    }
    sort_snapshots_newest_first(&mut dated);
    if let Some(hit) = dated
        .iter()
        .find(|s| parse_snapshot_timestamp(s).is_some_and(|t| t <= when))
    {
        return Ok(hit.clone());
    }
    let oldest = dated.last().expect("non-empty");
    let oldest_time = parse_snapshot_timestamp(oldest).expect("dated");
    Err(RsbError::NotFound(format!(
        "No snapshot taken at or before {}: the oldest available snapshot is {} (taken {})",
        display_time(when),
        snapshot_id_of(oldest),
        display_time(oldest_time)
    )))
}

/// Id of the latest snapshot of `storage` taken at or before `when` (see
/// [`select_snapshot_at_or_before`]).
pub async fn find_snapshot_at_or_before(
    storage: &dyn Storage,
    when: DateTime<Utc>,
) -> Result<String, RsbError> {
    let mut snapshots = storage.list("snapshots").await?;
    snapshots.retain(|s| s.ends_with(".toml"));
    let path = select_snapshot_at_or_before(&snapshots, when)?;
    Ok(snapshot_id_of(&path).to_string())
}

/// Like [`find_snapshot_at_or_before`] on the repository of `config` (or on
/// `storage` when given).
pub async fn find_snapshot_at_or_before_in(
    config: &Config,
    storage: Option<std::sync::Arc<dyn Storage>>,
    when: DateTime<Utc>,
) -> Result<String, RsbError> {
    let storage = super::storage_backend::storage_or_from_config(storage, config).await?;
    find_snapshot_at_or_before(&*storage, when).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn sel(patterns: &[&str]) -> FileSelector {
        FileSelector::new(patterns).unwrap()
    }

    #[test]
    fn exact_path() {
        let s = sel(&["docs/a.txt"]);
        assert!(s.matches("docs/a.txt"));
        assert!(!s.matches("docs/a.txt2"));
        assert!(!s.matches("docs/b.txt"));
        assert!(!s.matches("other/docs/a.txt"));
        assert!(sel(&["./docs//a.txt"]).matches("docs/a.txt"));
    }

    const CAFE_NFC: &str = "fotos/ver\u{e3}o/caf\u{e9}.txt";
    const CAFE_NFD: &str = "fotos/vera\u{303}o/cafe\u{301}.txt";

    #[test]
    fn nfc_and_nfd_match_each_other() {
        assert_ne!(CAFE_NFC, CAFE_NFD);
        for (pattern, key) in [(CAFE_NFC, CAFE_NFD), (CAFE_NFD, CAFE_NFC)] {
            assert!(sel(&[pattern]).matches(key), "{pattern:?} vs {key:?}");
            let dir = pattern.rsplit_once('/').unwrap().0;
            assert!(sel(&[dir]).matches(key), "dir {dir:?}");
            assert!(sel(&[&format!("{dir}/")]).matches(key), "dir/ {dir:?}");
            assert!(sel(&[&format!("{dir}/*.txt")]).matches(key), "glob");
            assert!(sel(&["**/caf\u{e9}.*"]).matches(key), "** glob");
            assert!(sel(&["**/cafe\u{301}.*"]).matches(key), "** glob nfd");
        }
        // Normalization does not fold different letters or case.
        assert!(!sel(&[CAFE_NFC]).matches("fotos/verao/cafe.txt"));
        assert!(!sel(&["fotos/VER\u{c3}O"]).matches(CAFE_NFD));
    }

    #[test]
    fn select_keeps_the_stored_name() {
        let entries = vec![
            (PathBuf::from(CAFE_NFD), 1),
            (PathBuf::from("other.txt"), 2),
        ];
        let got = sel(&[CAFE_NFC]).select(entries, "snap").unwrap();
        assert_eq!(got, vec![(PathBuf::from(CAFE_NFD), 1)]);
        // The pattern is reported as typed.
        assert_eq!(sel(&[CAFE_NFD]).patterns()[0].as_str(), CAFE_NFD);
    }

    #[test]
    fn directory_prefix() {
        for p in ["docs", "docs/", "docs\\"] {
            let s = sel(&[p]);
            assert!(s.matches("docs/a.txt"), "{p}");
            assert!(s.matches("docs/sub/deep/b.bin"), "{p}");
            assert!(!s.matches("docs2/a.txt"), "{p}");
            assert!(!s.matches("documents/a.txt"), "{p}");
            assert!(!s.matches("x/docs/a.txt"), "{p}");
        }
        // Without the slash a file of that name matches too; with it, not.
        assert!(sel(&["docs"]).matches("docs"));
        assert!(!sel(&["docs/"]).matches("docs"));
    }

    #[test]
    fn glob_patterns() {
        let s = sel(&["docs/*.pdf"]);
        assert!(s.matches("docs/a.pdf"));
        assert!(!s.matches("docs/sub/a.pdf"), "* must not cross '/'");
        assert!(!s.matches("docs/a.txt"));

        let top = sel(&["*.txt"]);
        assert!(top.matches("a.txt"));
        assert!(!top.matches("sub/a.txt"));

        let any = sel(&["**/*.txt"]);
        assert!(any.matches("a.txt"));
        assert!(any.matches("sub/deep/a.txt"));
        assert!(!any.matches("sub/a.bin"));

        // A glob matching a directory selects what is under it.
        let dirs = sel(&["photos/20?5"]);
        assert!(dirs.matches("photos/2025/img.jpg"));
        assert!(!dirs.matches("photos/2024/img.jpg"));
        assert!(sel(&["photos/202[0-5]/"]).matches("photos/2023/x"));

        // Literal names containing glob characters still match.
        assert!(sel(&["photo[1].jpg"]).matches("photo[1].jpg"));
        assert!(sel(&["photo[1].jpg"]).matches("photo1.jpg"));

        // Case sensitive, hidden files included.
        assert!(!sel(&["*.TXT"]).matches("a.txt"));
        assert!(sel(&["*"]).matches(".hidden"));
    }

    #[test]
    fn unicode_paths() {
        assert!(sel(&["fotos/verão/praia.jpg"]).matches("fotos/verão/praia.jpg"));
        assert!(sel(&["fotos/verão"]).matches("fotos/verão/praia.jpg"));
        assert!(sel(&["fotos/ver?o/*"]).matches("fotos/verão/praia.jpg"));
        assert!(sel(&["日本/*.txt"]).matches("日本/ファイル.txt"));
        assert!(!sel(&["fotos/verao"]).matches("fotos/verão/praia.jpg"));
    }

    #[test]
    fn rejects_unsafe_or_empty_patterns() {
        for p in [
            "",
            "  ",
            "/etc/passwd",
            "\\etc",
            "C:/Users",
            "c:\\x",
            "..",
            "../x",
            "docs/../../x",
            "docs\\..\\x",
            ".",
            "./",
            "a\0b",
        ] {
            let err = FileSelector::new([p]).unwrap_err();
            assert!(matches!(err, RsbError::InvalidInput(_)), "{p:?}: {err:?}");
        }
        // `..` inside a name is not a traversal.
        assert!(sel(&["a..b"]).matches("a..b"));
        assert!(FileSelector::new(["[unclosed"]).is_err());
    }

    #[test]
    fn select_errors_when_nothing_matches() {
        let entries = vec![(PathBuf::from("a.txt"), 1), (PathBuf::from("b/c.txt"), 2)];
        let err = sel(&["nope", "zz/*"])
            .select(entries.clone(), "snap-1")
            .unwrap_err();
        let RsbError::NotFound(msg) = err else {
            panic!("{err:?}")
        };
        assert!(msg.contains("'nope'") && msg.contains("'zz/*'") && msg.contains("snap-1"));
        assert!(msg.contains("nothing was restored"));

        let some = sel(&["b", "nope"]).select(entries.clone(), "s").unwrap();
        assert_eq!(some, vec![(PathBuf::from("b/c.txt"), 2)]);
        assert_eq!(
            FileSelector::default().select(entries, "s").unwrap().len(),
            2
        );
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn parses_dates() {
        let plus2 = FixedOffset::east_opt(2 * 3600).unwrap();
        let p = |s| parse_restore_date_in(s, &plus2).unwrap();
        assert_eq!(p("2026-10-01T12:00:00Z"), utc("2026-10-01T12:00:00Z"));
        assert_eq!(p("2026-10-01T12:00:00+05:00"), utc("2026-10-01T07:00:00Z"));
        // A date is the end of that day in the given zone.
        assert_eq!(p("2026-10-01"), utc("2026-10-01T21:59:59.999999999Z"));
        assert_eq!(p(" 2026-10-01 "), p("2026-10-01"));
        // HH:MM is the end of that minute, HH:MM:SS of that second.
        assert_eq!(p("2026-10-01 12:00"), utc("2026-10-01T10:00:59.999999999Z"));
        assert_eq!(p("2026-10-01T12:00"), p("2026-10-01 12:00"));
        assert_eq!(
            p("2026-10-01 12:00:30"),
            utc("2026-10-01T10:00:30.999999999Z")
        );
        for bad in [
            "",
            "yesterday",
            "2026-13-01",
            "2026-10-01 25:00",
            "01/10/2026",
        ] {
            let err = parse_restore_date_in(bad, &plus2).unwrap_err();
            assert!(matches!(err, RsbError::InvalidInput(_)), "{bad:?}");
        }
        // The local variant accepts the same syntax.
        assert!(parse_restore_date("2026-10-01").is_ok());
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| format!("snapshots/{s}.toml")).collect()
    }

    #[test]
    fn selects_latest_at_or_before() {
        let snaps = names(&[
            "2026-10-01T080000.000Z",
            "2026-10-01T120000.000Z",
            "2026-10-01T120000.000Z-1",
            "2026-10-01T180000.500Z",
            "2026-10-03T090000Z",
            "not-a-snapshot",
        ]);
        let pick = |w: &str| {
            select_snapshot_at_or_before(&snaps, utc(w)).map(|p| snapshot_id_of(&p).to_string())
        };
        // Exactly equal timestamp qualifies; same-instant collision → highest suffix.
        assert_eq!(
            pick("2026-10-01T12:00:00Z").unwrap(),
            "2026-10-01T120000.000Z-1"
        );
        assert_eq!(
            pick("2026-10-01T11:59:59.999Z").unwrap(),
            "2026-10-01T080000.000Z"
        );
        // Several the same day: the latest of the day.
        assert_eq!(
            pick("2026-10-01T23:59:59.999999999Z").unwrap(),
            "2026-10-01T180000.500Z"
        );
        assert_eq!(
            pick("2026-10-01T18:00:00.499Z").unwrap(),
            "2026-10-01T120000.000Z-1"
        );
        assert_eq!(pick("2030-01-01T00:00:00Z").unwrap(), "2026-10-03T090000Z");
        // Before the oldest: error naming it.
        let err = pick("2026-10-01T07:59:59Z").unwrap_err();
        let RsbError::NotFound(msg) = err else {
            panic!("{err:?}")
        };
        assert!(msg.contains("2026-10-01T080000.000Z"), "{msg}");
        assert!(msg.contains("2026-10-01T07:59:59Z"), "{msg}");

        assert!(matches!(
            select_snapshot_at_or_before(&[], utc("2026-10-01T00:00:00Z")),
            Err(RsbError::NotFound(_))
        ));
    }

    #[test]
    fn snapshot_ids() {
        assert_eq!(
            snapshot_id_of("snapshots/2026-10-01T080000Z.toml"),
            "2026-10-01T080000Z"
        );
        assert_eq!(snapshot_id_of("2026-10-01T080000Z"), "2026-10-01T080000Z");
    }
}
