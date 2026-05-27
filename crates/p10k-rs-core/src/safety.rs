//! Render-path safety primitives.
//!
//! Anything that flows from the outside world (git metadata, the working
//! directory, environment values) into the rendered prompt must pass through
//! [`sanitize_for_terminal`] before reaching a segment's output. The prompt is
//! ultimately written to a TTY and assigned to the shell's `PROMPT` variable;
//! both interpret bytes that the producer never intended (terminal escape
//! sequences, zsh `%` prompt expansions). Stripping control bytes at the
//! boundary prevents an attacker who controls a branch name or directory name
//! from steering the terminal or the shell's prompt-expansion engine.
//!
//! Per-shell escapes (zsh's `%` → `%%`) are applied later in
//! [`crate::render_prompt`]'s `wrap_for_shell` pass, after segments have
//! assembled their output.
//!
//! ## Path-open primitives
//!
//! [`open_owned_safely`] and [`open_owned_mode_safely`] are the lift of the
//! `open_fifo_safely` pattern from `p10k-rs-git` (slice 9, T1.14): open with
//! `O_NOFOLLOW`, then `fstat` on the resulting fd to re-verify file type and
//! ownership against the inode we actually hold. This closes the
//! lstat→open TOCTOU window for any attacker-influenced regular file
//! (config files, instant-prompt artefacts, located binaries).

use std::borrow::Cow;
#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
use thiserror::Error;
use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};
use unicode_segmentation::UnicodeSegmentation;

/// Default cap (in grapheme clusters) applied by
/// [`SafeText::from_untrusted_with_cap`] when callers don't specify their
/// own. 256 graphemes is well above any sane branch name / cwd component
/// / kube context, and well below "this will visibly blow up the prompt
/// line." If you want unbounded text, use [`SafeText::from_untrusted`].
pub const DEFAULT_SAFE_TEXT_CAP: usize = 256;

/// `true` if `c` is a C0/C1 control or DEL that must not reach the TTY.
///
/// Mirrors the C0/C1 half of the [`sanitize_for_terminal`] predicate so the
/// borrowed fast path and the owned slow path stay in lock-step.
#[inline]
fn is_unsafe(c: char) -> bool {
    if c == '\t' {
        return false;
    }
    c.is_control() || c == '\u{007F}'
}

/// `true` if `c` is a Unicode codepoint we strip on attacker-controlled
/// input for display-safety reasons (T1.10, slice α).
///
/// Four classes, all of which render invisibly or near-invisibly while
/// changing how text is interpreted by the terminal, by a downstream
/// equality check, or by a human reading the prompt:
///
/// - **`BiDi` format characters** (`U+200E`, `U+200F`, `U+202A..=U+202E`,
///   `U+2066..=U+2069`). These reorder display direction; the
///   "Trojan Source" class of attacks (Boucher & Anderson 2021) uses
///   them to hide malicious code from human review.
/// - **Default-ignorable** marks (`U+200B`/`U+200C`/`U+200D` zero-width
///   space/non-joiner/joiner, `U+FEFF` BOM, `U+034F` combining grapheme
///   joiner, `U+180E` Mongolian vowel separator). Invisible bytes that
///   change clipboard contents, copy/paste behaviour, or identifier
///   comparison.
/// - **Tag chars** (`U+E0000..=U+E007F`). The "tag character" range was
///   originally proposed for language tagging; in practice it's an
///   arbitrary-data-in-glyph exfiltration channel rendered invisibly by
///   most terminals.
/// - **Variation selectors** (`U+FE00..=U+FE0F` and `U+E0100..=U+E01EF`).
///   These select alternate glyph forms; bare use in branch / path names
///   is suspect and routinely abused to spoof file extensions.
///
/// This is **not** a comprehensive Unicode-spoof defence (homoglyphs,
/// mixed-script attacks, normalisation tricks beyond NFC are out of
/// scope for this slice). It's the targeted minimum that closes the
/// loud "Trojan Source / invisible-bytes" attacks at the render
/// boundary.
#[inline]
fn is_unicode_unsafe(c: char) -> bool {
    let n = c as u32;
    matches!(
        n,
        // BiDi format
        0x200E | 0x200F
        | 0x202A..=0x202E
        | 0x2066..=0x2069
        // Default-ignorable invisibles
        | 0x200B | 0x200C | 0x200D
        | 0xFEFF | 0x034F | 0x180E
        // Variation selectors (BMP + supplementary)
        | 0xFE00..=0xFE0F
        | 0xE0100..=0xE01EF
        // Tag characters
        | 0xE0000..=0xE007F
    )
}

/// Replace bytes in `s` that are unsafe to emit to a terminal, then
/// NFC-normalise the result.
///
/// Three predicates fire, in this order:
///
/// 1. **C0/C1 controls + DEL** (`is_unsafe`) — `\x1b`, `\r`, `\b`,
///    `\x07`, OSC introducers, the unicode C1 controls in
///    `U+0080..=U+009F`, plus DEL. Tab is preserved (terminals render
///    it as visible whitespace).
/// 2. **Unicode display-safety class** (`is_unicode_unsafe`) — `BiDi`
///    format chars (the "Trojan Source" class), zero-width invisibles,
///    BOM, tag chars, variation selectors. These render invisibly or
///    reorder display while changing how the text is interpreted.
/// 3. **NFC normalisation** — collapses decomposed sequences (e.g.
///    `e` + combining acute `\u{0301}` → precomposed `é`). Without
///    this, two visually-identical branch names could compare unequal
///    or evade prompt-side equality checks.
///
/// Returns `Cow<'_, str>`:
/// - **Borrowed** when the input is already free of unsafe codepoints
///   *and* already NFC. Zero allocation for the common case (ASCII
///   branch names, simple cwd components, version strings).
/// - **Owned** when filtering or normalisation is needed.
///
/// # Examples
///
/// ```
/// use p10k_rs_core::safety::sanitize_for_terminal;
///
/// assert_eq!(&*sanitize_for_terminal("main"), "main");
/// assert_eq!(&*sanitize_for_terminal("foo\tbar"), "foo\tbar");
/// assert_eq!(&*sanitize_for_terminal("foo\x1b]0;evil\x07bar"), "foo]0;evilbar");
/// assert_eq!(&*sanitize_for_terminal("a\rb"), "ab");
/// ```
#[must_use]
pub fn sanitize_for_terminal(s: &str) -> Cow<'_, str> {
    // Fast path: no unsafe codepoints AND already NFC → borrow.
    // Branch names, cwd components, version strings hit this case.
    let needs_filter = s.chars().any(|c| is_unsafe(c) || is_unicode_unsafe(c));
    if !needs_filter && is_nfc_quick(s.chars()) == IsNormalized::Yes {
        return Cow::Borrowed(s);
    }
    // Slow path: filter unsafe codepoints, then NFC-normalise the
    // result. The two-pass design keeps the filter predicate trivial
    // and lets the NFC pass operate on a known-clean iterator.
    let filtered: String = if needs_filter {
        s.chars()
            .filter(|&c| !(is_unsafe(c) || is_unicode_unsafe(c)))
            .collect()
    } else {
        s.to_string()
    };
    Cow::Owned(filtered.nfc().collect())
}

/// Truncate `s` to at most `cap` grapheme clusters, appending `…` when
/// truncation occurred. The ellipsis counts toward `cap`.
///
/// Grapheme-cluster boundaries mean combining accents, ZWJ emoji
/// families, and other multi-codepoint clusters are never cut mid-way.
fn truncate_to_graphemes(s: &str, cap: usize) -> String {
    let total = s.graphemes(true).count();
    if total <= cap {
        return s.to_string();
    }
    // Reserve 1 grapheme for the ellipsis (which is itself 1 grapheme).
    let take = cap.saturating_sub(1);
    let mut out = String::with_capacity(s.len());
    for g in s.graphemes(true).take(take) {
        out.push_str(g);
    }
    out.push('…');
    out
}

/// A string whose every codepoint has passed [`sanitize_for_terminal`].
///
/// Producer-side guarantee: no control byte (except `\t`), no DEL. The
/// inner string is private so the only way to put bytes into a `SafeText`
/// is through a sanitising constructor — there's no `assume_safe` escape
/// hatch by design. Literals like `"HEAD"` go through sanitisation too;
/// the cost is one no-op pass over a tiny string.
///
/// Implements `AsRef<str>` and `Display` so segments can format it
/// transparently:
///
/// ```
/// use p10k_rs_core::safety::SafeText;
/// let branch = SafeText::from_untrusted("main\rEVIL");
/// assert_eq!(branch.as_str(), "mainEVIL");
/// assert_eq!(format!("on {branch}"), "on mainEVIL");
/// ```
///
/// `From<&str>` is provided as ergonomic sugar for the same constructor;
/// the implicit form is what makes test fixtures readable. Either form
/// upholds the invariant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SafeText(String);

impl SafeText {
    /// Construct a `SafeText` from a `&str`, stripping unsafe bytes.
    ///
    /// Idempotent on already-safe strings; uses [`sanitize_for_terminal`]'s
    /// borrow-when-clean fast path and pays the allocation only when at
    /// least one character actually needs stripping. Already-clean inputs
    /// still incur one allocation here because `SafeText`'s inner type
    /// is owned `String`.
    #[must_use]
    pub fn from_untrusted(s: &str) -> Self {
        Self(sanitize_for_terminal(s).into_owned())
    }

    /// Construct a `SafeText` from a `&str`, sanitising and then
    /// truncating to at most `cap` grapheme clusters.
    ///
    /// All [`sanitize_for_terminal`] guarantees apply (C0/C1 stripped,
    /// Unicode-unsafe codepoints stripped, NFC-normalised). On top of
    /// that, the result has at most `cap` grapheme clusters. If
    /// truncation occurred, the result ends in `…` (which itself
    /// counts as one grapheme toward `cap`).
    ///
    /// Use this constructor for any text source that could be
    /// arbitrarily long under attacker influence (branch names, jj
    /// descriptions, kube context names, env-var values). See
    /// [`DEFAULT_SAFE_TEXT_CAP`] for the recommended default.
    ///
    /// # Examples
    ///
    /// ```
    /// use p10k_rs_core::safety::SafeText;
    /// let long = "a".repeat(500);
    /// let t = SafeText::from_untrusted_with_cap(&long, 10);
    /// assert!(t.as_str().ends_with('…'));
    /// ```
    #[must_use]
    pub fn from_untrusted_with_cap(s: &str, cap: usize) -> Self {
        let sanitised = sanitize_for_terminal(s);
        Self(truncate_to_graphemes(&sanitised, cap))
    }

    /// Construct a `SafeText` from raw bytes that may not be valid UTF-8.
    ///
    /// Non-UTF-8 sequences are replaced with `U+FFFD` (the Unicode
    /// replacement character) before sanitisation, mirroring
    /// `String::from_utf8_lossy`. This is the right constructor for
    /// data coming off the `gitstatusd` wire or any other byte-stream
    /// boundary where encoding can't be assumed.
    #[must_use]
    pub fn from_untrusted_bytes(b: &[u8]) -> Self {
        Self(sanitize_for_terminal(&String::from_utf8_lossy(b)).into_owned())
    }

    /// Borrow the inner string. Always-safe `&str` for pushing into
    /// rendered output.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` if the safe string is the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Length in bytes (matches `str::len`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl AsRef<str> for SafeText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SafeText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SafeText {
    fn from(s: &str) -> Self {
        Self::from_untrusted(s)
    }
}

// String-equality conveniences: `assert_eq!(safe_text, "main")` and
// `assert_eq!(safe_text, String::from("main"))` work without an explicit
// `.as_str()` on the LHS. The reverse direction (`"main" == safe_text`)
// is intentionally not provided — the cost is one `.as_str()` at the
// call site, in exchange for keeping the foreign-impl footprint small.
impl PartialEq<str> for SafeText {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for SafeText {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for SafeText {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

/// Errors produced by [`open_owned_safely`] and [`open_owned_mode_safely`].
///
/// Returned as a typed enum so callers can distinguish "didn't exist /
/// couldn't open" (transient — fall through to a default) from "exists
/// but failed a safety invariant" (loud — warn the user, refuse to use).
#[cfg(unix)]
#[derive(Debug, Error)]
pub enum SafetyError {
    /// The open syscall failed (path missing, permission denied, `ELOOP`
    /// from `O_NOFOLLOW` on a symlink, …). Wraps the underlying
    /// [`std::io::Error`] so callers can match on
    /// [`std::io::ErrorKind::NotFound`] and quietly fall back.
    #[error("open failed: {0}")]
    Open(#[from] std::io::Error),

    /// `fstat` on the just-opened fd failed. Vanishingly rare in practice
    /// (the kernel just handed us this fd); kept as a distinct variant so
    /// the caller can log it instead of treating it as a security failure.
    /// Unix-only — Windows has no `fstat`-based open-and-check pattern.
    #[cfg(unix)]
    #[error("fstat failed: {0}")]
    Stat(rustix::io::Errno),

    /// The opened inode is not a regular file (it was a directory, FIFO,
    /// socket, or device). `O_NOFOLLOW` already rejects symlinks at open
    /// time, so this variant fires on the cases the kernel cannot rule
    /// out from the open flags alone.
    #[error("not a regular file: {path}")]
    NotRegularFile {
        /// The path the caller asked us to open.
        path: std::path::PathBuf,
    },

    /// The file's owner uid is neither the current effective uid nor
    /// root. Refusing foreign-owned files is the load-bearing TOCTOU
    /// defence — the inode behind the fd cannot change uid mid-flight.
    #[error("foreign owner: file at {path} is owned by uid {uid}, not us or root")]
    ForeignOwner {
        /// The path the caller asked us to open.
        path: std::path::PathBuf,
        /// The uid the kernel reports owning the file.
        uid: u32,
    },

    /// The file's mode bits include world or group write permission
    /// (`mode & max_mode_mask != 0`). Co-tenants who can write to a config
    /// file we trust would otherwise influence the rendered prompt.
    #[error(
        "insecure permissions: file at {path} has mode {mode:#o}, which sets bits \
         outside the allowed mask {max_mask:#o}"
    )]
    InsecurePermissions {
        /// The path the caller asked us to open.
        path: std::path::PathBuf,
        /// The mode bits the kernel reports on the file.
        mode: u32,
        /// The mask the caller asked us to enforce against.
        max_mask: u32,
    },
}

/// POSIX `S_IFMT` — file-type mask within a stat `mode` field.
#[cfg(unix)]
const S_IFMT: u32 = 0o170_000;

/// POSIX `S_IFREG` — masked-mode value identifying a regular file.
#[cfg(unix)]
const S_IFREG: u32 = 0o100_000;

/// Open `path` for read, verifying on the resulting fd that it is a
/// regular file owned by either the current effective uid or root.
///
/// This is the TOCTOU-safe lift of the `open_fifo_safely` pattern from
/// `p10k-rs-git` (T1.14). Defences applied:
///
/// - `O_NOFOLLOW` on the open: a symlink (or a mid-flight symlink swap)
///   turns into `ELOOP`, surfaced as [`SafetyError::Open`].
/// - `fstat` on the open fd: re-verify the file type is a regular file
///   and the owner uid is either the current effective uid or `0` (root,
///   which covers `apt install`-style system binaries). The check
///   operates on the inode behind the fd we already hold, so it is
///   race-free relative to any further path manipulation.
///
/// **No mode check.** Use [`open_owned_mode_safely`] for callers that
/// additionally need to reject world/group-writable files (config,
/// instant-prompt dump). Distinguishing the two keeps the helper useful
/// for binaries (e.g. `gitstatusd`) whose modes are out of our control
/// but whose ownership we still want to gate on.
///
/// # Errors
///
/// Returns [`SafetyError::Open`] on open failure, [`SafetyError::Stat`]
/// on fstat failure, [`SafetyError::NotRegularFile`] if the fd points at
/// a directory / FIFO / socket / device, and [`SafetyError::ForeignOwner`]
/// if the owner uid is neither our effective uid nor root.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use p10k_rs_core::safety::open_owned_safely;
///
/// match open_owned_safely(Path::new("/usr/bin/gitstatusd")) {
///     Ok(_file) => { /* safe to use */ }
///     Err(e) => eprintln!("refusing binary: {e}"),
/// }
/// ```
#[cfg(unix)]
pub fn open_owned_safely(path: &Path) -> Result<File, SafetyError> {
    open_owned_inner(path, None)
}

/// Non-Unix shim: plain `File::open` with no TOCTOU / owner / mode
/// checks. Windows ACL probing is a separate slice (see v0.2.4 release
/// notes); until then, callers get reduced safety on Windows.
#[cfg(not(unix))]
pub fn open_owned_safely(path: &Path) -> Result<File, SafetyError> {
    File::open(path).map_err(SafetyError::Open)
}

/// Open `path` for read, additionally rejecting any file whose mode bits
/// fall outside `max_mode` (i.e. any bit set in `mode & !max_mode`).
///
/// All the guarantees of [`open_owned_safely`] apply. The mode check is
/// **additive** — pick `max_mode` to be the most permissive mode the
/// caller is willing to accept, and any extra bit (world write, group
/// write, set-uid, …) trips [`SafetyError::InsecurePermissions`].
///
/// Common values:
///
/// - `0o644` — the typical "user config file" ceiling. Allows
///   owner-rw, group/world-r; rejects group-w (`0o020`), world-w
///   (`0o002`), set-uid (`0o4000`), and friends.
/// - `0o600` — strict private-data ceiling (e.g. instant-prompt dump).
/// - `0o755` — executable ceiling.
///
/// Note: the check uses `mode & !max_mode != 0` against the **permission
/// bits only** (the low 12 bits, mask `0o7777`); the file-type field is
/// validated separately by the regular-file invariant.
///
/// # Errors
///
/// All variants of [`SafetyError`], plus
/// [`SafetyError::InsecurePermissions`] when bits outside `max_mode` are
/// set on the inode.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use p10k_rs_core::safety::open_owned_mode_safely;
///
/// // Refuse a config that is group- or world-writable.
/// let _f = open_owned_mode_safely(Path::new("/home/me/.config/p10k-rs/config.toml"), 0o644);
/// ```
#[cfg(unix)]
pub fn open_owned_mode_safely(path: &Path, max_mode: u32) -> Result<File, SafetyError> {
    open_owned_inner(path, Some(max_mode))
}

/// Non-Unix shim: see [`open_owned_safely`]'s non-Unix variant. The
/// `max_mode` parameter is accepted-and-ignored so callers compile
/// unchanged.
#[cfg(not(unix))]
pub fn open_owned_mode_safely(path: &Path, _max_mode: u32) -> Result<File, SafetyError> {
    File::open(path).map_err(SafetyError::Open)
}

/// Shared body of [`open_owned_safely`] and [`open_owned_mode_safely`].
/// Splitting the public surface (rather than threading an `Option`
/// through) keeps the simpler signature obvious at the call site.
#[cfg(unix)]
fn open_owned_inner(path: &Path, max_mode: Option<u32>) -> Result<File, SafetyError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[allow(clippy::cast_possible_wrap)]
    let nofollow = rustix::fs::OFlags::NOFOLLOW.bits() as i32;
    opts.custom_flags(nofollow);
    let f = opts.open(path)?;
    let stat = rustix::fs::fstat(&f).map_err(SafetyError::Stat)?;
    // `st_mode` is `u32` on Linux and `u16` on macOS; `u32::from`
    // widens both losslessly. See the matching note in
    // `p10k-rs-git::gitstatusd::open_fifo_safely`.
    #[allow(clippy::useless_conversion)]
    let mode_bits: u32 = u32::from(stat.st_mode);
    if mode_bits & S_IFMT != S_IFREG {
        return Err(SafetyError::NotRegularFile {
            path: path.to_path_buf(),
        });
    }
    let my_uid = rustix::process::geteuid().as_raw();
    if stat.st_uid != my_uid && stat.st_uid != 0 {
        return Err(SafetyError::ForeignOwner {
            path: path.to_path_buf(),
            uid: stat.st_uid,
        });
    }
    if let Some(max) = max_mode {
        let perm_bits = mode_bits & 0o7777;
        if perm_bits & !max != 0 {
            return Err(SafetyError::InsecurePermissions {
                path: path.to_path_buf(),
                mode: perm_bits,
                max_mask: max,
            });
        }
    }
    Ok(f)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn passes_plain_ascii_through() {
        assert_eq!(sanitize_for_terminal(""), "");
        assert_eq!(sanitize_for_terminal("main"), "main");
        assert_eq!(sanitize_for_terminal("feat/widget-1.2"), "feat/widget-1.2");
    }

    #[test]
    fn preserves_tab() {
        assert_eq!(sanitize_for_terminal("a\tb"), "a\tb");
    }

    #[test]
    fn strips_carriage_return() {
        // CR is the prompt-overwrite vector — anything after \r reprints
        // from column 0 on most terminals.
        assert_eq!(sanitize_for_terminal("normal\rEVIL$ "), "normalEVIL$ ");
    }

    #[test]
    fn strips_backspace() {
        assert_eq!(sanitize_for_terminal("ab\x08c"), "abc");
    }

    #[test]
    fn strips_escape_and_osc_payload() {
        // OSC 0 sets the terminal title. Stripping the ESC and BEL leaves
        // the otherwise-printable payload visible (intentional — the user
        // sees that something weird is in the input).
        assert_eq!(
            sanitize_for_terminal("main\x1b]0;TARS-OWNED\x07"),
            "main]0;TARS-OWNED",
        );
    }

    #[test]
    fn strips_screen_clear() {
        assert_eq!(sanitize_for_terminal("\x1b[2J\x1b[Hgone"), "[2J[Hgone");
    }

    #[test]
    fn strips_del() {
        assert_eq!(sanitize_for_terminal("a\x7Fb"), "ab");
    }

    #[test]
    fn strips_unicode_c1_controls() {
        // U+0085 (NEL) and U+0086 (SSA) are control codes some terminals
        // act on. Strip.
        assert_eq!(sanitize_for_terminal("a\u{0085}b\u{0086}c"), "abc");
    }

    #[test]
    fn preserves_non_control_unicode() {
        // Branch names with emoji, accents, CJK, etc. must round-trip.
        assert_eq!(sanitize_for_terminal("café"), "café");
        assert_eq!(sanitize_for_terminal("分支"), "分支");
        assert_eq!(sanitize_for_terminal("feat/🚀"), "feat/🚀");
    }

    #[test]
    fn does_not_escape_percent() {
        // `%` is a zsh PROMPT-expansion concern handled by the shell-aware
        // wrapping pass; this function must leave `%` alone so non-zsh
        // shells aren't double-processed.
        assert_eq!(sanitize_for_terminal("%n@%m"), "%n@%m");
    }

    #[test]
    fn safe_text_strips_controls_at_construction() {
        let t = SafeText::from_untrusted("main\rEVIL");
        assert_eq!(t.as_str(), "mainEVIL");
    }

    #[test]
    fn safe_text_from_bytes_handles_non_utf8() {
        let t = SafeText::from_untrusted_bytes(&[b'm', b'a', b'i', b'n', 0xFF, 0xFE]);
        assert!(t.as_str().starts_with("main"));
        assert!(t.as_str().contains('\u{FFFD}'));
    }

    #[test]
    fn safe_text_from_bytes_strips_controls() {
        let t = SafeText::from_untrusted_bytes(b"main\x1b]0;TARS\x07");
        assert_eq!(t.as_str(), "main]0;TARS");
    }

    #[test]
    fn safe_text_default_is_empty() {
        let t = SafeText::default();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert_eq!(t.as_str(), "");
    }

    #[test]
    fn safe_text_displays_as_inner_string() {
        let t = SafeText::from_untrusted("feat/x");
        assert_eq!(format!("{t}"), "feat/x");
    }

    #[test]
    fn safe_text_from_str_via_into_strips_controls() {
        // `From<&str>` sugar for tests and constants — same guarantee.
        let t: SafeText = "with\rcontrol".into();
        assert_eq!(t.as_str(), "withcontrol");
    }

    // ---------- Unicode hardening (T1.10, slice α) ----------

    #[test]
    fn strips_bidi_right_to_left_override() {
        // U+202E is the headline Trojan Source character — reverses
        // display so what reads as "hellotxt.exe" actually executes
        // as "helloexe.txt" (or the reverse). Strip the override
        // unconditionally.
        assert_eq!(&*sanitize_for_terminal("hello\u{202E}dlrow"), "hellodlrow");
    }

    #[test]
    fn strips_bidi_isolates() {
        // U+2066..U+2069 (LRI/RLI/FSI/PDI) are the modern BiDi
        // isolates introduced for the same attack class.
        assert_eq!(&*sanitize_for_terminal("a\u{2066}b\u{2069}c"), "abc");
    }

    #[test]
    fn strips_zero_width_joiner() {
        // ZWJ can splice glyphs invisibly. Strip from attacker text.
        assert_eq!(&*sanitize_for_terminal("a\u{200D}b"), "ab");
    }

    #[test]
    fn strips_zero_width_space_and_bom() {
        // U+200B (ZWSP) and U+FEFF (BOM) are invisible bytes used to
        // bypass equality / sort / search.
        assert_eq!(&*sanitize_for_terminal("a\u{200B}b\u{FEFF}c"), "abc");
    }

    #[test]
    fn strips_tag_chars() {
        // Tag chars (U+E0000..U+E007F) are an arbitrary-data-in-glyph
        // exfiltration channel, invisible in most terminals.
        assert_eq!(&*sanitize_for_terminal("ok\u{E0001}\u{E0042}"), "ok");
    }

    #[test]
    fn strips_variation_selectors() {
        // U+FE00..U+FE0F select alternate glyph forms; bare use in
        // attacker text is suspect and routinely used to spoof
        // file-extension display.
        assert_eq!(&*sanitize_for_terminal("plain\u{FE0F}text"), "plaintext");
    }

    #[test]
    fn nfc_normalises_decomposed_input() {
        // "café" decomposed: c, a, f, e, U+0301 (combining acute).
        // Sanitizer must NFC-normalise to the precomposed form so
        // equality comparisons against "café" (U+00E9) succeed.
        let decomposed = "cafe\u{0301}";
        let composed = "caf\u{00E9}"; // "café" — single codepoint é
        assert_eq!(&*sanitize_for_terminal(decomposed), composed);
    }

    #[test]
    fn fast_path_borrows_when_already_clean_and_nfc() {
        // Common-case branch name: no unsafe codepoints, already NFC.
        // The Cow must borrow (zero allocation).
        let s = "feat/widget-1.2";
        let out = sanitize_for_terminal(s);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn end_to_end_malicious_branch_neutralised() {
        // Realistic attack: a file or branch named "e\u{202E}xe.txt"
        // displays as "etxt.exe" in a BiDi-aware renderer — swapping
        // the extension. Stripping the RLO leaves the original
        // bytes visible without the display reorder.
        let attacker = "e\u{202E}xe.txt";
        assert_eq!(&*sanitize_for_terminal(attacker), "exe.txt");
    }

    #[test]
    fn safe_text_with_cap_truncates_long_input() {
        let long = "a".repeat(500);
        let t = SafeText::from_untrusted_with_cap(&long, 10);
        // 9 'a' + 1 '…' = 10 graphemes.
        assert!(t.as_str().ends_with('…'));
        assert_eq!(t.as_str().chars().count(), 10);
    }

    #[test]
    fn safe_text_with_cap_under_limit_is_unchanged() {
        let t = SafeText::from_untrusted_with_cap("short", 100);
        assert_eq!(t.as_str(), "short");
        assert!(!t.as_str().contains('…'));
    }

    #[test]
    fn safe_text_with_cap_respects_grapheme_boundary() {
        // "g̃" = g + U+0303 (combining tilde) — no NFC precomposed
        // form, so it stays as 2 codepoints, 1 grapheme cluster.
        // Truncation must not split the cluster.
        let input = "abc g\u{0303}xyz";
        let t = SafeText::from_untrusted_with_cap(input, 6);
        // Expect 5 graphemes preserved + 1 ellipsis = 6 graphemes total.
        // The g̃ cluster must be whole or absent — never split.
        let graphemes: Vec<&str> = t.as_str().graphemes(true).collect();
        assert_eq!(graphemes.len(), 6);
        assert!(t.as_str().ends_with('…'));
        // If "g\u{0303}" appears, it must appear as a whole 2-byte
        // sequence — never just "g" followed by ellipsis.
        if t.as_str().contains('g') {
            assert!(t.as_str().contains('\u{0303}'));
        }
    }

    #[test]
    fn safe_text_with_cap_strips_then_caps() {
        // Sanitisation runs FIRST, then truncation. A BiDi RLO in
        // the input shouldn't survive even if there's headroom in
        // the cap.
        let t = SafeText::from_untrusted_with_cap("safe\u{202E}", 100);
        assert_eq!(t.as_str(), "safe");
    }

    #[test]
    fn default_safe_text_cap_is_set_to_256() {
        // Sanity-pin the constant so a careless change is caught
        // in review — 256 graphemes is the documented contract.
        assert_eq!(DEFAULT_SAFE_TEXT_CAP, 256);
    }

    // ---------- open_owned_safely (T1.14) ----------
    //
    // The helper is `#[cfg(unix)]` — tests follow the same gate. We
    // hand-roll temp paths instead of pulling `tempfile` back in (removed
    // in slice 9 per workspace dep policy); mirrors the scratch-dir
    // pattern in `p10k-rs-git`.

    #[cfg(unix)]
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10k-rs-core-safety-test-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&p).unwrap_or_else(|e| panic!("create_dir {p:?}: {e}"));
        p
    }

    #[cfg(unix)]
    fn rm_rf(p: &std::path::Path) {
        let _ = std::fs::remove_dir_all(p);
    }

    #[cfg(unix)]
    fn chmod(p: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
            .unwrap_or_else(|e| panic!("chmod {p:?} {mode:o}: {e}"));
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_safely_accepts_regular_file_owned_by_us() {
        let dir = scratch_dir("owned-accept");
        let p = dir.join("ours.toml");
        std::fs::write(&p, b"hello").unwrap();
        let f = open_owned_safely(&p);
        assert!(f.is_ok(), "expected Ok, got {f:?}");
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_safely_rejects_symlink() {
        // Even a symlink that points at a perfectly fine file we own
        // must be refused: O_NOFOLLOW on the open turns it into ELOOP.
        let dir = scratch_dir("owned-symlink");
        let real = dir.join("real.toml");
        std::fs::write(&real, b"hello").unwrap();
        let link = dir.join("link.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = open_owned_safely(&link).expect_err("symlink must be refused");
        // `O_NOFOLLOW` → ELOOP on Linux, ENOTDIR on some BSDs; either way
        // it surfaces as `SafetyError::Open`. We don't pin the errno.
        assert!(
            matches!(err, SafetyError::Open(_)),
            "expected SafetyError::Open, got {err:?}"
        );
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_safely_rejects_directory() {
        // The fd opens fine (no O_DIRECTORY enforcement); the regular-file
        // post-check is what bails.
        let dir = scratch_dir("owned-dir");
        let err = open_owned_safely(&dir).expect_err("directory must be refused");
        assert!(
            matches!(err, SafetyError::NotRegularFile { .. }),
            "expected NotRegularFile, got {err:?}"
        );
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_mode_safely_rejects_world_writable() {
        let dir = scratch_dir("mode-world-writable");
        let p = dir.join("loose.toml");
        std::fs::write(&p, b"hello").unwrap();
        chmod(&p, 0o666);
        let err =
            open_owned_mode_safely(&p, 0o644).expect_err("world-writable file must be refused");
        match err {
            SafetyError::InsecurePermissions { mode, max_mask, .. } => {
                assert_eq!(mode & 0o002, 0o002, "world-write bit not set");
                assert_eq!(max_mask, 0o644);
            }
            other => panic!("expected InsecurePermissions, got {other:?}"),
        }
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_mode_safely_rejects_group_writable() {
        let dir = scratch_dir("mode-group-writable");
        let p = dir.join("group.toml");
        std::fs::write(&p, b"hello").unwrap();
        chmod(&p, 0o664);
        let err =
            open_owned_mode_safely(&p, 0o644).expect_err("group-writable file must be refused");
        assert!(
            matches!(err, SafetyError::InsecurePermissions { .. }),
            "expected InsecurePermissions, got {err:?}"
        );
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_mode_safely_accepts_0o600_under_0o644_cap() {
        let dir = scratch_dir("mode-tight-ok");
        let p = dir.join("private.toml");
        std::fs::write(&p, b"hello").unwrap();
        chmod(&p, 0o600);
        let f = open_owned_mode_safely(&p, 0o644);
        assert!(f.is_ok(), "0o600 ≤ 0o644 must pass, got {f:?}");
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_mode_safely_accepts_0o644_at_cap() {
        let dir = scratch_dir("mode-cap-ok");
        let p = dir.join("readable.toml");
        std::fs::write(&p, b"hello").unwrap();
        chmod(&p, 0o644);
        let f = open_owned_mode_safely(&p, 0o644);
        assert!(f.is_ok(), "0o644 at cap must pass, got {f:?}");
        rm_rf(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn open_owned_safely_returns_open_on_missing_file() {
        let dir = scratch_dir("owned-missing");
        let p = dir.join("nope.toml");
        let err = open_owned_safely(&p).expect_err("missing file must error");
        match err {
            SafetyError::Open(io) => {
                assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected Open(NotFound), got {other:?}"),
        }
        rm_rf(&dir);
    }
}
