# Release Process

NexDeck adheres to **Semantic Versioning (`MAJOR.MINOR.PATCH`)** as specified in [TZ.md](../../TZ.md) (§20).

---

## Version Rules

- **`0.x.y` (Pre-1.0):** Initial architecture and MVP stages (`0.1.0`, `0.2.0`, `0.3.0`, ...). Breaking API/protocol changes are permitted with clear changelog warnings.
- **`MAJOR`:** Incompatible API or transport protocol changes after `1.0.0`.
- **`MINOR`:** Backward-compatible features.
- **`PATCH`:** Backward-compatible fixes and security patches.

## Synchronizing Versions

Before tagging a release:

1. Update `version` in root `Cargo.toml` (`[workspace.package]`) and any client app manifests (`apps/mobile/pubspec.yaml` from Stage 2 onward).
2. Update `CHANGELOG.md` by moving `[Unreleased]` entries into a dated `[X.Y.Z]` section.
3. Merge the release preparation PR into `main` after all CI checks pass.
4. Tag the `main` commit and push the tag:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

Pushing a `v*.*.*` tag triggers [`.github/workflows/release.yml`](../../.github/workflows/release.yml), which verifies the workspace, builds Windows release artifacts, generates `SHA256SUMS.txt`, and publishes the GitHub Release.
