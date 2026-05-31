# typed: false
# frozen_string_literal: true

# Homebrew formula for powerlevel10k-rs.
#
# Bottle-only on macOS: pulls the prebuilt, sigstore-signed tarball that
# the release pipeline publishes (`.github/workflows/release.yml`). The
# tarball already ships `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`,
# and `THIRD-PARTY-LICENSES.md` alongside the `p10k-rs` binary, so this
# formula is intentionally thin — extract, install the binary, done.
#
# Linux installs are out of scope for now; users on Linux should use
# `install.sh` from the repo root. (Adding linuxbrew support is a future
# slice — the prebuilt Linux tarballs already exist, the formula just
# doesn't wire them yet.)
class P10kRs < Formula
  desc "Single-binary, declarative-TOML, multi-shell Powerlevel10k port in Rust"
  homepage "https://github.com/scurran1986/powerlevel10k-rs"
  version "1.0.0"
  license any_of: ["MIT", "Apache-2.0"]

  # The release pipeline publishes per-triple tarballs at:
  #   https://github.com/scurran1986/powerlevel10k-rs/releases/download/
  #     v<version>/p10k-rs-<version>-<rust-triple>.tar.gz
  # Rust triples — not Homebrew arch names. Don't "fix" the URL to
  # `arm64`/`x86_64` unless the release pipeline starts publishing
  # under those names.
  on_macos do
    on_arm do
      url "https://github.com/scurran1986/powerlevel10k-rs/releases/download/v1.0.0/p10k-rs-1.0.0-aarch64-apple-darwin.tar.gz"
      sha256 "22e48cb35be40ceb25bb448813d6a6a9d3f1024d194cdfef05ce844e046e53ad"
    end

    on_intel do
      url "https://github.com/scurran1986/powerlevel10k-rs/releases/download/v1.0.0/p10k-rs-1.0.0-x86_64-apple-darwin.tar.gz"
      sha256 "e048ba111e7b1d040dd9b1ad6138c8659b666f693d94ddd4fffa9fd84ce6a16e"
    end
  end

  def install
    # The release tarball unpacks into a single directory named
    # `p10k-rs-<version>-<triple>/`. Homebrew's `url` extraction lands us
    # inside that directory automatically, so the binary and the doc
    # files are at the cwd root here.
    bin.install "p10k-rs"

    # Bundled licence + doc files. Guarded so a future tarball reshuffle
    # doesn't break the install.
    doc.install "README.md" if File.exist?("README.md")
    doc.install "THIRD-PARTY-LICENSES.md" if File.exist?("THIRD-PARTY-LICENSES.md")

    # Man page (release tarball includes `man1/p10k-rs.1` since v0.2.7).
    man1.install "man1/p10k-rs.1" if File.exist?("man1/p10k-rs.1")
  end

  def caveats
    <<~EOS
      To activate the prompt, add one of these to your shell rc file:

        zsh   (~/.zshrc):       eval "$(p10k-rs init zsh)"
        bash  (~/.bashrc):      eval "$(p10k-rs init bash)"
        fish  (~/.config/fish/config.fish):
                                p10k-rs init fish | source

      Then open a new shell. Configuration lives at
      ~/.config/p10k-rs/config.toml; with no config the binary ships a
      sensible 5-segment default (dir / vcs / time / status / prompt).

      Run `p10k-rs verify` to confirm the bundled gitstatusd helper
      matches the pinned sha256, and `p10k-rs daemon-health` to check
      that the long-lived git daemon is wired and responsive.
    EOS
  end

  test do
    # Bare smoke test: the binary launches and reports its version.
    # `p10k-rs version` (subcommand, not the clap-auto `--version`) is
    # the diagnostic introduced in v0.2.0; it always exits 0 and emits
    # a multi-line summary that mentions the version string.
    assert_match version.to_s, shell_output("#{bin}/p10k-rs version")
  end
end
