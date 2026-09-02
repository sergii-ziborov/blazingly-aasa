# Oracle runs

Raw output from Apple's `swcutil`, captured on macOS 27.0 (26A5388g), arm64, 2026-09-02.

`swcutil` requires root for every subcommand and exists only on macOS, so this cannot run in
ordinary CI. It is captured here so the conclusions drawn from it are auditable without a Mac and
without root, and so a future run can be diffed against this one.

| File | What produced it |
| --- | --- |
| `swcutil-corpus.txt` | `swcutil verify -d <domain> -j <file> -u <url>` over all 73 matching cases |
| `swcutil-probes.tsv` | `swcutil match -u <url> -j <dict>` over 67 targeted probes |

`swcutil match` is the useful one: it tests a single pattern dictionary against a single URL, with
no document structure or app identifiers in the way, so it isolates one semantic question per run.

Reproduce with `scripts/oracle_swcutil.sh`, which needs `sudo`.

## What the first run settled

68 of 73 corpus cases agreed. One of the five disagreements was an artifact of the harness — this
crate lets an empty domain mean "skip the host check", and `swcutil` requires a domain, so the
harness substituted one. The other four were real, and all four were this crate being wrong.
See `docs/parity.md` for what changed.
