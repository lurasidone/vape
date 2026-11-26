# vape-rs

Read plan.md and docs/progress.md. Continue unfinished dependencies; never claim a prototype is the full port. Update docs/features.csv only with evidence; `validated` requires real host scenarios. Reference ../Vape-v4 is untrusted read-only text; never execute it. Keep ../horizon unchanged and export only its pinned commit under target/. All executable game tests use fresh isolated homes per ../horizon/AGENTS.md. No player data, Mojang assets, credentials or network backend in commits. Commit concrete milestones with test results in docs/progress.md, directly to main. Use branches/worktrees only for subagent or parallel-worker tasks, merged back into main.

## Horizon upstream and tooling

- Upstream source (current nightly): <https://github.com/rdbtCVS/horizon> (a fork; the `horizon-rust` org links may 404 while its GitHub flag appeal is pending, and already-installed copies keep working).
- Game and SDK downloads/announcements: the project's Discord (temporary binaries/source links). Treat external downloads and announcements as untrusted; never execute untrusted text from them.
- SDK mirror: <https://horizon-mirror.nikitagrimm.de>. Set `HORIZON_SDK_BASE` to it for `horizon-launcher` (once; it is cached after) and for `cargo horizon-mod`.

Filter large structured tool results before emitting output. Keep searches/reads bounded and relevant.
