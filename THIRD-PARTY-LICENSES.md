# Third-Party Licenses

## gitstatusd

`p10k-rs` bundles unmodified `gitstatusd` binaries from the upstream
[romkatv/gitstatus](https://github.com/romkatv/gitstatus) project (pinned
tag v1.5.4) as part of release artifacts.

**License:** GPL-3.0-or-later

**Bundling:** The architecture (subprocess + stdin/stdout IPC pipes) is
arms-length communication between independent programs, not a derivative work.
Per GPL § 0, bundling binaries is "mere aggregation," not a copyleft trigger
on `p10k-rs` itself. The `p10k-rs` codebase remains MIT/Apache-2.0; the
bundled `gitstatusd` retains GPL-3.0.

**Obligations:**

1. Include `gitstatusd`'s GPL-3.0 license file alongside its binary in
   release artifacts.
2. Provide a written source-code offer per GPL § 6: a pointer to
   [github.com/romkatv/gitstatus](https://github.com/romkatv/gitstatus) at
   the pinned tag is sufficient.
3. Preserve upstream copyright notice: `gitstatusd --version` output in
   release documentation serves this purpose.

**Source availability:** The full source for the bundled version is available
at https://github.com/romkatv/gitstatus/releases/tag/v1.5.4.

---

*This is the consensus open-source reading and not legal advice. For questions
or commercial use, consult an attorney familiar with GPL licensing.*
