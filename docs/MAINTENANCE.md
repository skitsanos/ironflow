# Maintenance and cleanup

## Inventory Before Cleanup

- Record the starting branch and commit, `git status --porcelain=v1`, stash
  hashes (`git stash list --format=%H`), and `git worktree list --porcelain`.
  Use `git clean -ndX` only as an inventory of ignored files in a checkout,
  never as permission to delete them. Preserve unrelated changes and stashes.
- Use a separate checkout and target directory for builds that need isolation.
  Record the exact paths, container and volume names, image IDs, and builder
  name created by the task. A path, tag, or name prefix alone does not establish
  ownership.

## Keep Verification Evidence

Wait for the checks needed for the operation. For a push, verify the required
CI on the pushed SHA; for a release, also finish the public artifact checks.
A push or a successful build alone does not make its outputs disposable.

Record the verified commit, applicable CI run links and artifact digests.
If a disposable checkout passed the full local gate, retain its
`ironflow-gate-receipt.json` outside the target directory before cleaning it.
Verify the retained copy with `bun --no-env-file scripts/gate_receipt.ts check
<commit>`, setting `CARGO_TARGET_DIR` to the directory containing the copy.
This preserves evidence for that tree; it does not qualify a different tree
or extend the receipt's pre-push validity.

## Clean Only What The Task Owns

- Resolve `target_directory` with `cargo metadata --locked --no-deps
  --format-version 1`. Use `cargo clean --target-dir <verified-path>` only
  when the directory was created for the task and is not shared. The
  integration gate already reclaims IronFlow build artifacts in its own
  target directory. Preserve shared Cargo downloads, caches, and toolchains.
- Remove only recorded Docker fixtures and unused images. Check IDs and
  references from other containers first. For an isolated Docker build, remove
  the task-owned buildx builder after its work ends. Do not prune a shared
  builder or run system-wide Docker cleanup.
- Remove task-owned scratch files after retaining the required evidence.
  Remove an inactive, task-owned worktree with `git worktree remove` only
  after checking for uncommitted changes and retained ignored data. Keep
  unmerged local commits needed for later work. Preview stale worktree
  metadata with `git worktree prune --dry-run`; skip pruning if it would
  touch a record outside the task.
- Recheck the checkout and stash inventory. Report what was removed, what
  was intentionally kept, and reclaimed space when measured.

## Preserve Permanent Branches

Keep `main` and `develop` locally and remotely, including after release
promotion. GitHub's merged-head auto-deletion handles disposable PR branches.
The [Preserve main and develop ruleset](https://github.com/skitsanos/ironflow/rules/28243455)
blocks deletion of those two branches without adding push restrictions or
bypass actors. Do not disable it for cleanup.

- Before deleting a local task branch, verify that the intended target
  contains its commit or the verified squash merge. Use `git branch -d`;
  leave the branch alone if Git refuses to delete it safely.
- Before removing a stale tracking ref, confirm the PR merged, its remote head
  is absent, and the cached tip matches the merged PR head. Remove only that
  tracking ref. Preserve changed or unmerged tips. Manual remote deletion
  requires explicit authorization and a fresh tip check.
