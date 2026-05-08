# For Maintainers

Various processes employed by maintainers, documented here for transparency and
possibly to offer guidance potential new maintainers.

## Releasing a new version

1. Ensure the Nix version in `shell.nix` matches the target Nix release. You
2. Compute the crate version (`0.<normalized_nix>.0`) and set it in the
   workspace `Cargo.toml` under `[workspace.package] version`.
3. Commit on `main`, then tag:

   ```sh
   git tag v0.2327.0 && git push origin v0.2327.0
   ```

4. The `publish` workflow (`.github/workflows/publish.yml`) runs on the tag,
   verifies the tag matches the crate version, runs tests, publishes both crates
   to crates.io, then creates the `release/v0.2327` branch with the version
   pre-bumped to `0.2327.1` for future backports. No further manual steps are
   needed.

## Backporting

Backports are label-driven and automated. All development happens on `main` and
is backported to release branches by the CI.

**To backport a fix:**

1. Open a PR against `main` as usual.
2. Before merging, add a label `backport release/v0.2324` (one per target
   branch).
3. Merge the PR. The `backport` workflow (`.github/workflows/backport.yml`)
   automatically cherry-picks the commits onto each requested release branch and
   opens a backport PR.
4. Review the backport PR, resolve any conflicts, and merge it.
5. Merging triggers `publish.yml`, which tags the current version, publishes the
   hotfix, and bumps the patch version on the release branch for the next
   backport.
