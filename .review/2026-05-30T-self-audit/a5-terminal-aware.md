# A5 — Terminal-aware attacker

**Capability summary:** an attacker who exploits the rendering
layer — terminal escape sequences embedded in attacker-controlled
text, OSC payloads abused for title spoofing or clipboard
hijacking, BiDi / zero-width / homoglyph attacks against the
human reading the prompt, prompt-expansion injection against
unpatched zsh. They can land bytes through any of the 44
untrusted-input sources from THREAT-MODEL.md § 2 (branch names,
cwd, env vars, tag names, jj bookmarks, …). Their goal: trick the
terminal, the shell's prompt-expansion engine, or the human into
treating attacker text as code or as different text than what's
displayed.

## Threats

### T-A5.1 — C0/C1/DEL injection (ESC, BEL, CR, NEL)
- **State:** **done.** `sanitize_for_terminal` strips every C0
  (except `\t`), every C1 (`U+0080..=U+009F`), and DEL (`U+007F`).
  The fast path borrows on already-clean input; the slow path
  allocates and re-walks.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:42-52` (`is_unsafe`
  predicate), `crates/p10k-rs-core/src/safety.rs:138-156`
  (`sanitize_for_terminal`).

### T-A5.2 — BiDi / Trojan Source attack via branch name
- **State:** **done.** `U+202E` (RLO), `U+202A..=U+202E` (LRE /
  RLE / PDF / LRO / RLO), `U+2066..=U+2069` (LRI / RLI / FSI /
  PDI), `U+200E` / `U+200F` (LRM / RLM) — all stripped.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:83-101`
  (`is_unicode_unsafe`).

### T-A5.3 — Zero-width / invisible Unicode (ZWJ / ZWSP / BOM /
CGJ / variation selectors / tag chars)
- **State:** **done.** `U+200B` (ZWSP), `U+200C` (ZWNJ),
  `U+200D` (ZWJ), `U+FEFF` (BOM), `U+034F` (CGJ), `U+180E`
  (Mongolian Vowel Separator), variation selectors
  `U+FE00..=U+FE0F` + `U+E0100..=U+E01EF`, tag chars
  `U+E0000..=U+E007F` — all stripped.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:83-101`.

### T-A5.4 — NFC normalisation evasion (`café` vs `cafe\u{0301}`)
- **State:** **done.** `sanitize_for_terminal` calls `.nfc()` on
  the filtered iterator before returning, so decomposed
  combining sequences round-trip to the precomposed form. The
  fast-path borrow check uses `is_nfc_quick`; the slow path
  rebuilds via the `unicode-normalization` crate.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:142-156`.

### T-A5.5 — zsh `%` expansion (`%n`, `%m`, `%(?...)`)
- **State:** **done.** `wrap_for_shell` doubles every `%` byte
  in attacker-influenced text before emission. This is the
  per-shell encoder layer that sits *after* the segment-side
  `SafeText` chokepoint.
- **Cite:** `crates/p10k-rs-core/src/lib.rs:898-901` (`%` →
  `%%`), `crates/p10k-rs-core/src/lib.rs:1582` (chokepoint
  documentation in the wrap-for-shell module).

### T-A5.6 — `PROMPT_SUBST` RCE on unpatched zsh
(CVE-2021-45444; branch name like `$(rm -rf ~)`)
- **State:** **done.** `wrap_for_shell` escapes `$`, backtick,
  and `\` for any byte that survived past the CSI / OSC pass-
  through arms (i.e. text content rather than escape-sequence
  bytes). This neutralises command substitution and
  continuation-line tricks on zsh <5.8.1.
- **Cite:** `crates/p10k-rs-core/src/lib.rs:903-924` (T1.12 /
  slice γ guard).

### T-A5.7 — OSC 8 hyperlink with `javascript:` / `data:` URL
- **State:** **out of scope at the rendering layer; documented
  intent.** The architecture does not emit OSC 8 hyperlinks
  with attacker-controlled URLs from the prompt today. The
  THREAT-MODEL.md primary-defenses column lists
  "scheme-whitelist on OSC 8" as a defence for the rare future
  case where OSC 8 is emitted. **No-code-citation** because
  the emission path does not exist yet.
- **Cite:** `(no-code-citation)` — verified by absence of
  `osc_8` / `\x1b]8;` emission in `crates/p10k-rs-segments/`.

### T-A5.8 — OSC 52 clipboard injection from prompt
- **State:** **done by absence.** No OSC 52 emission path
  exists in `crates/p10k-rs-segments/` or `crates/p10k-rs-core/`.
  THREAT-MODEL.md explicitly forbids it; the section is out of
  scope for the prompt by design.
- **Cite:** `(no-code-citation)` — verified by absence.

### T-A5.9 — OSC 1337 iTerm proprietary protocol abuse
- **State:** **done by absence.** No `\x1b]1337;` emission path
  in any segment.
- **Cite:** `(no-code-citation)` — verified by absence.

### T-A5.10 — Env var → segment text without SafeText (kubecontext,
aws, docker_context, terraform, anaconda, virtualenv, pyenv,
nodenv, fnm, pixi, mise, vi_mode, context)
- **State:** **done at render boundary, partial at the type
  boundary.** All 12 env-reading segments verified call
  `SafeText::from_untrusted` or `sanitize_for_terminal` on the
  env value before it lands in the rendered output:
  - `kubecontext.rs:114` (`SafeText::from_untrusted(&s)`)
  - `aws.rs:91, 108` (`SafeText::from_untrusted` /
    `sanitize_for_terminal`)
  - `docker_context.rs:127, 130, 135, 142` (all routes)
  - `terraform.rs:90, 96`
  - `anaconda.rs:95, 114`
  - `virtualenv.rs:92, 107`
  - `pyenv.rs:84, 96`
  - `nodenv.rs:85, 98`
  - `fnm.rs:93, 108`
  - `pixi.rs:92, 106`
  - `mise.rs:137`
  - `context.rs:175, 185, 194, 199, 206` (USER / LOGNAME /
    HOSTNAME / nodename / COMPUTERNAME via `sanitize_for_terminal`)
  The **residual gap is structural, not exploitable today**:
  the `Segment` trait's text-bearing fields accept raw
  `String`, so a future segment that reads an env var and
  returns it raw will silently bypass the chokepoint. Slice β
  of THREAT-MODEL.md proposes lifting this to a `SafeText`-
  only trait boundary.
- **Cite:** see grepped paths above; all verified.

### T-A5.11 — `context.rs` reads 8 env vars; verify the wrap
- **State:** **done at render, partial at type boundary.**
  Reads: `$USER` (×2), `$LOGNAME` (×2), `$P10K_RS_DEFAULT_USER`,
  `$HOSTNAME`, `$COMPUTERNAME`, plus a `.any(|key| std::env::var(key)…)`
  emptiness probe at line 150. Every value that reaches
  output is wrapped through `sanitize_for_terminal` (lines
  175, 185, 194, 199, 206). The emptiness probe at line 150
  is data-only (`!v.is_empty()`); no rendering side-effect.
- **Cite:** `crates/p10k-rs-segments/src/context.rs:53-79`
  (env reads), `crates/p10k-rs-segments/src/context.rs:175-206`
  (sanitisation), `crates/p10k-rs-segments/src/context.rs:29`
  (`use p10k_rs_core::safety::sanitize_for_terminal;`).

### T-A5.12 — Oversize text DOS the line wrap
- **State:** **done.** `SafeText::from_untrusted_with_cap` caps
  at `DEFAULT_SAFE_TEXT_CAP = 256` grapheme clusters with an
  `…` marker; grapheme-cluster boundaries are respected so no
  half-cluster cuts.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:35-40`,
  `crates/p10k-rs-core/src/safety.rs:163-176`.

### T-A5.13 — Homoglyph attack (Cyrillic 'а' vs Latin 'a' in branch)
- **State:** **open (intentional).** THREAT-MODEL.md § 9
  explicitly puts "refusing to render mixed-script branches"
  out of scope — "Flag, never block. False-positive shape is
  non-trivial." No mixed-script detection today.
- **Cite:** `(no-code-citation)` — by design.

### T-A5.14 — Width-math attack (wide char that mis-aligns the line)
- **State:** **partial.** Length math in the segment-truncation
  path uses `chars().count()` rather than `unicode-width` in a
  few spots (THREAT-MODEL.md § 5 Layer-2 entry). Grapheme-
  cluster truncation is correct for *boundary safety*; width-
  aware truncation for *display safety* is slice α-adjacent
  in the recommendations and not landed.
- **Cite:** `(partial — design intent, code state per
  THREAT-MODEL.md § 5)`.

## Residual gaps (ranked, this attacker class)

1. **`Segment` trait accepts raw `String`** for text-bearing
   fields. All extant env-reading segments wrap into
   `SafeText` / `sanitize_for_terminal` before emission, but
   the type system does not enforce it. A future segment
   author who forgets the wrap ships a regression. Slice β.
   *(Not closeable in lane A — needs a trait-surface change.)*
2. **`context.rs` raw `std::env::var(…)`** reads (8 spots)
   produce `String`s that are sanitised on the way out, but
   the read site does not type-tag the value as
   "attacker-controlled." Cosmetic until the trait boundary
   change happens. *(Lane A may add `SafeText` wraps closer
   to the read site, which would improve the read-site
   audit story without changing exploitability.)*
3. **Width-math uses `chars().count()` in a couple of
   truncation paths.** Visual alignment can be desynced by
   attacker-chosen wide characters; not a code-execution
   class, just a "line drifts" class.
4. **Homoglyph / mixed-script detection** is intentionally
   absent. THREAT-MODEL.md § 9 puts it out of scope; flagged
   here for completeness only.

## Conclusion

The terminal-aware surface is the surface this project takes
most seriously, and it shows. C0/C1/DEL + the four Unicode
invisibility classes + NFC normalisation + `%` doubling + `$` /
backtick / `\` escaping cover every documented
single-byte-injection class. OSC 52 / OSC 1337 / sixel are
forbidden by absence rather than by filter, which is the right
choice for a prompt. The genuine residuals are structural
(`Segment` trait accepts `String`, not `SafeText`) rather than
exploitable today; a careful audit found every env-reading
segment already routing through the chokepoint. Width-math and
homoglyph attacks remain open by design, with the explicit
rationale that "flag, never block" is the right shape and the
false-positive cost of mixed-script blocking is too high. Net:
A5 is the strongest-defended surface in this codebase.
