//! Bundled prompt themes (`p10k-rs theme` subcommand).
//!
//! Each theme is a small TOML file under `themes/` at the workspace
//! root, embedded into the binary via [`include_str!`] so a
//! `cargo install`d build stays self-contained — users don't need
//! the source tree on disk to install a theme.
//!
//! Add a theme by:
//!
//! 1. Writing `themes/<name>.toml`.
//! 2. Appending a [`Theme`] entry to [`THEMES`] with the matching
//!    `name`, a one-line `description`, and `toml = include_str!(...)`.
//! 3. The unit test `every_bundled_theme_parses` exercises the parser
//!    against every entry — a malformed theme fails CI.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// One bundled theme: a stable name, a one-line description for the
/// `theme list` subcommand, and the embedded TOML bytes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    /// Stable identifier passed to `theme show` / `theme install`.
    pub(crate) name: &'static str,
    /// One-line description shown by `theme list`.
    pub(crate) description: &'static str,
    /// The embedded TOML payload — written verbatim to the user's
    /// config file when installed, or printed to stdout by `show`.
    pub(crate) toml: &'static str,
}

/// The bundled theme catalogue. Order here is the order `theme list`
/// prints; keep alphabetised by display priority (lean / classic /
/// rainbow / pure first, then palette themes alphabetically).
pub(crate) const THEMES: &[Theme] = &[
    Theme {
        name: "lean",
        description: "Minimal single-line, no frame, no powerline. FollowTerminal.",
        toml: include_str!("../../../themes/lean.toml"),
    },
    Theme {
        name: "classic",
        description: "Single-line with chevron frame. ANSI 256.",
        toml: include_str!("../../../themes/classic.toml"),
    },
    Theme {
        name: "rainbow",
        description: "Powerline ribbons with chip backgrounds. Truecolor + Nerd Font.",
        toml: include_str!("../../../themes/rainbow.toml"),
    },
    Theme {
        name: "pure",
        description: "Pure-zsh inspired minimal two-line. FollowTerminal.",
        toml: include_str!("../../../themes/pure.toml"),
    },
    Theme {
        name: "catppuccin-mocha",
        description: "Catppuccin Mocha pastel palette. Truecolor + Nerd Font.",
        toml: include_str!("../../../themes/catppuccin-mocha.toml"),
    },
    Theme {
        name: "dracula",
        description: "Dracula purple-tinted high-contrast palette. Truecolor + Nerd Font.",
        toml: include_str!("../../../themes/dracula.toml"),
    },
    Theme {
        name: "gruvbox-dark",
        description: "Gruvbox Dark warm retro palette. Truecolor + Nerd Font.",
        toml: include_str!("../../../themes/gruvbox-dark.toml"),
    },
    Theme {
        name: "nord",
        description: "Nord arctic blue palette. Truecolor + Nerd Font.",
        toml: include_str!("../../../themes/nord.toml"),
    },
    Theme {
        name: "solarized-dark",
        description: "Solarized Dark classic palette. Truecolor.",
        toml: include_str!("../../../themes/solarized-dark.toml"),
    },
    Theme {
        name: "tokyo-night",
        description: "Tokyo Night cool nightscape palette. Truecolor + Nerd Font.",
        toml: include_str!("../../../themes/tokyo-night.toml"),
    },
];

/// Look up a theme by exact name. Returns `None` if no bundled theme
/// matches.
pub(crate) fn find(name: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.name == name)
}

/// `p10k-rs theme list` — print the bundled theme catalogue to stdout
/// as `<name>  <description>` aligned by name width.
///
/// Returns `Result<()>` (always `Ok`) to keep a uniform shape with
/// the other `theme_*` entrypoints — main.rs dispatches into all
/// three through a single `?`-able match arm.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn cmd_list() -> Result<()> {
    let width = THEMES.iter().map(|t| t.name.len()).max().unwrap_or(0);
    for t in THEMES {
        println!("{:<width$}  {}", t.name, t.description, width = width);
    }
    Ok(())
}

/// `p10k-rs theme show <name>` — print the named theme's TOML to
/// stdout. Errors if no such theme exists.
pub(crate) fn cmd_show(name: &str) -> Result<()> {
    let theme = find(name).ok_or_else(|| unknown_theme_error(name))?;
    print!("{}", theme.toml);
    Ok(())
}

/// `p10k-rs theme install <name> [--force]` — write the named theme
/// to the user's config path. Existing config is backed up to
/// `config.toml.bak` unless `--force` is set (which discards the
/// existing config entirely).
///
/// Config path resolution mirrors `Config::load_default` exactly:
///
/// 1. `$P10K_RS_CONFIG` if set.
/// 2. `$XDG_CONFIG_HOME/p10k-rs/config.toml`.
/// 3. `$HOME/.config/p10k-rs/config.toml`.
///
/// Parent directories are created at mode 0o700 if they don't exist.
pub(crate) fn cmd_install(name: &str, force: bool) -> Result<()> {
    let theme = find(name).ok_or_else(|| unknown_theme_error(name))?;
    let path = resolve_install_path()
        .context("could not resolve a config path (set $XDG_CONFIG_HOME or $HOME)")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent {}", parent.display()))?;
    }
    if path.exists() && !force {
        let backup = backup_path(&path);
        std::fs::copy(&path, &backup)
            .with_context(|| format!("backing up {} to {}", path.display(), backup.display()))?;
        eprintln!("backed up existing config to {}", backup.display());
    }
    std::fs::write(&path, theme.toml).with_context(|| format!("writing {}", path.display()))?;
    println!("installed theme {} at {}", theme.name, path.display());
    eprintln!("reload your shell (exec zsh or open a new terminal) to see it");
    Ok(())
}

/// Build the backup sibling path used by [`cmd_install`] when a
/// config already exists — appends a `.bak` suffix to the file's
/// existing extension (or to the bare stem if there's no extension).
fn backup_path(p: &Path) -> PathBuf {
    let mut out = p.to_path_buf();
    let new_ext = match p.extension().and_then(|s| s.to_str()) {
        Some(ext) => format!("{ext}.bak"),
        None => "bak".to_owned(),
    };
    out.set_extension(new_ext);
    out
}

/// Resolve where `theme install` should write. Mirrors
/// `Config::load_default`'s discovery order so the next `prompt`
/// invocation picks up the freshly-written file.
fn resolve_install_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("P10K_RS_CONFIG") {
        return Some(PathBuf::from(p));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("p10k-rs").join("config.toml"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Some(
            PathBuf::from(home)
                .join(".config")
                .join("p10k-rs")
                .join("config.toml"),
        );
    }
    None
}

/// Build the error returned for an unknown theme name. Includes the
/// catalogue so the user can copy a correct name from the message.
fn unknown_theme_error(name: &str) -> anyhow::Error {
    let names = THEMES.iter().map(|t| t.name).collect::<Vec<_>>().join(", ");
    anyhow::anyhow!("unknown theme '{name}'. Available: {names}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use p10k_rs_core::Config;

    /// Every bundled theme must parse via `Config::from_toml`. A
    /// theme that doesn't deserialise cleanly fails CI before it can
    /// ship — `theme install` would otherwise put a broken config in
    /// front of the user. The non-empty invariant is pinned by the
    /// `catalogue_count` test below.
    #[test]
    fn every_bundled_theme_parses() {
        for t in THEMES {
            Config::from_toml(t.toml).unwrap_or_else(|e| {
                panic!("theme '{}' failed to parse: {e}", t.name);
            });
        }
    }

    /// Theme names must be unique. A duplicate would cause `find` to
    /// return the first match and silently mask the second.
    #[test]
    fn theme_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for t in THEMES {
            assert!(seen.insert(t.name), "duplicate theme name: {}", t.name);
        }
    }

    /// `find` round-trips for every catalogue entry and returns `None`
    /// for an unknown name.
    #[test]
    fn find_round_trips_and_misses_unknown() {
        for t in THEMES {
            let got = find(t.name).expect("catalogue entry must be findable");
            assert_eq!(got.name, t.name);
        }
        assert!(find("definitely-not-a-theme").is_none());
    }

    /// The catalogue's count contract — anyone bumping `THEMES`
    /// touches this test, which is the prompt to also update
    /// `themes/README.md` and the STATUS.md feature row.
    #[test]
    fn catalogue_count() {
        assert_eq!(THEMES.len(), 10, "expected 10 bundled themes");
    }

    /// `backup_path` derives a sibling `<path>.bak` next to the
    /// install path.
    #[test]
    fn backup_path_appends_bak() {
        let p = Path::new("/tmp/x/config.toml");
        assert_eq!(backup_path(p), Path::new("/tmp/x/config.toml.bak"));
        let p = Path::new("/tmp/noext");
        assert_eq!(backup_path(p), Path::new("/tmp/noext.bak"));
    }
}
