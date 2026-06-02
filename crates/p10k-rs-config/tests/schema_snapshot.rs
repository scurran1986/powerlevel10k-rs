#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Freeze the user-visible TOML schema with `insta` snapshots.
//!
//! Phase-6 roadmap criterion #2: "no breaking config changes in 1.x".
//! The complementary gate is `cargo-semver-checks` on the public Rust
//! API; that workflow catches breaking signature changes in
//! `p10k-rs-config`'s exported types. These snapshots catch the
//! orthogonal failure mode: a serde-only break (field rename, removed
//! variant, changed tag spelling) that the Rust API gate would treat
//! as additive.
//!
//! Two tests:
//!
//! 1. `config_default_is_frozen` — pins `Config::default()`. Fresh
//!    installs with no `~/.config/p10k-rs/config.toml` render against
//!    this surface, so a drift here is a drift in what every new user
//!    sees out of the box.
//! 2. `config_kitchen_sink_is_frozen` — round-trips the bundled
//!    `themes/lean.toml` through `Config::from_toml` and snapshots
//!    the parsed struct. The theme exercises every layout slot
//!    (`left`, `right`), every prompt-relevant enum (`Mode`,
//!    `ColorMode`, `TransientPromptMode`), per-segment `foreground`
//!    overrides, and per-state overrides — the surface a real config
//!    actually touches.
//!
//! When the schema *does* change intentionally, regenerate the
//! snapshots with `INSTA_UPDATE=always cargo test -p p10k-rs-config
//! --test schema_snapshot` and review the diff before committing the
//! new `.snap` files. An unexplained diff in CI is the signal.
//!
//! # Stability note
//!
//! `Config::segments` and `Config::ai.hosts` are `HashMap`s, whose
//! iteration order is randomised per process. Snapshotting them
//! directly would flake. The two tests below project through
//! `SortedConfig`, a `BTreeMap`-backed view that keys-sort
//! deterministically while preserving every observable field. This
//! still freezes the user-visible TOML surface — the on-disk order
//! of `[segment.<name>]` blocks has never been semantically
//! meaningful in TOML.

use std::collections::BTreeMap;

use p10k_rs_config::{
    AiConfig, ColorMode, Config, HostConfig, InstantPromptMode, Layout, Mode, SegmentConfig,
    ShellIntegration, TransientPromptMode,
};
use serde::Serialize;

/// Snapshot-stable projection of [`Config`].
///
/// Same fields as `Config`, but `segments` and `ai.hosts` are
/// `BTreeMap`s so the YAML output is byte-stable across runs.
#[derive(Debug, Serialize)]
struct SortedConfig<'a> {
    schema_version: u32,
    mode: Mode,
    colors: ColorMode,
    layout: &'a Layout,
    instant_prompt: InstantPromptMode,
    transient_prompt: TransientPromptMode,
    segment: BTreeMap<&'a str, &'a SegmentConfig>,
    ai: SortedAi<'a>,
    shell_integration: ShellIntegration,
}

#[derive(Debug, Serialize)]
struct SortedAi<'a> {
    osc7: bool,
    osc133: bool,
    host: BTreeMap<&'a str, &'a HostConfig>,
    model: Option<&'a str>,
    context_tokens: Option<u32>,
}

impl<'a> SortedConfig<'a> {
    fn from(cfg: &'a Config) -> Self {
        Self {
            schema_version: cfg.schema_version,
            mode: cfg.mode,
            colors: cfg.colors,
            layout: &cfg.layout,
            instant_prompt: cfg.instant_prompt,
            transient_prompt: cfg.transient_prompt,
            segment: cfg.segments.iter().map(|(k, v)| (k.as_str(), v)).collect(),
            ai: SortedAi::from(&cfg.ai),
            shell_integration: cfg.shell_integration,
        }
    }
}

impl<'a> SortedAi<'a> {
    fn from(ai: &'a AiConfig) -> Self {
        Self {
            osc7: ai.osc7,
            osc133: ai.osc133,
            host: ai.hosts.iter().map(|(k, v)| (k.as_str(), v)).collect(),
            model: ai.model.as_deref(),
            context_tokens: ai.context_tokens,
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "insta::get_cargo_workspace shells out to `cargo metadata`; outside the miri syscall-free scope per workflows/miri.yml"
)]
fn config_default_is_frozen() {
    let cfg = Config::default();
    insta::assert_yaml_snapshot!(SortedConfig::from(&cfg));
}

#[test]
#[cfg_attr(
    miri,
    ignore = "insta::get_cargo_workspace shells out to `cargo metadata`; outside the miri syscall-free scope per workflows/miri.yml"
)]
fn config_kitchen_sink_is_frozen() {
    let src = include_str!("../../../themes/lean.toml");
    let cfg = Config::from_toml(src).expect("lean theme parses");
    insta::assert_yaml_snapshot!(SortedConfig::from(&cfg));
}
