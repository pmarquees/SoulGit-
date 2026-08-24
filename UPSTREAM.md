# Upstream relationship

SoulGit is a fork of [`tobi/walgit`](https://github.com/tobi/walgit). The Walgit commit history is preserved and
the upstream remote should remain named `upstream`.

```sh
git remote add upstream https://github.com/tobi/walgit.git
git fetch upstream
git merge upstream/main
```

Resolve upstream changes in favor of Walgit's storage, consistency, authentication, and round-trip invariants.
Keep SoulGit-specific proposal behavior isolated in `crates/soulgit-proposals`, the proposal CLI, API projection,
and inbox UI so future upstream merges remain reviewable.
