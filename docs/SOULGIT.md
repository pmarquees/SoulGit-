# SoulGit proposal protocol

SoulGit keeps Walgit's S3/GCS object store as the permanent authority and adds a Git-native inbox in front of
protected branches. It is intentionally closer to mail than an automatic merge queue: contributors publish;
subscribers receive proposal refs without changing their working trees; humans or agents attest to an exact
revision; an authorized merge advances the canonical branch only after repository readiness rules pass.

## Ref model

For proposal `<id>` at commit `<oid>`:

```text
refs/soulgit/proposals/<id>/head                                -> <oid>
refs/soulgit/proposals/<id>/target/<branch>                     -> <oid>
refs/soulgit/proposals/<id>/author/<identity>                   -> <oid>
refs/soulgit/proposals/<id>/state/<state>                       -> <oid>
refs/soulgit/proposals/<id>/reviews/<actor>/<decision>          -> <oid>
refs/soulgit/proposals/<id>/checks/<actor>/<name>/<result>      -> <oid>
refs/soulgit/proposals/<id>/result                              -> <merge-oid>
```

`<state>` is `open`, `reviewing`, `changes-requested`, `approved`, `merging`, `merged`, `rejected`,
`superseded`, or `expired`. A review decision is `approved` or `changes-requested`; a check result is `pending`,
`passed`, `failed`, or `skipped`.

The head is the revision boundary. Metadata, reviews, and checks are active only when their ref points at the
current head OID. Updating a proposal moves the head and its metadata atomically; existing attestations become
stale without being destroyed. The result ref records the commit that reached the target branch. The server
projects the family into an inbox and stores no proposal database.

The head commit's subject and body are the proposal title and description. This keeps the content portable and
avoids a second mutable metadata object.

## Readiness and agent grants

Readiness is versioned on the canonical target branch in `.soulgit.toml`:

```toml
version = 1

[merge]
min_approvals = 1
required_checks = ["tests"]
allow_author_approval = false
strategy = "fast-forward" # fast-forward | merge-commit | squash

[[agents]]
principal = "svc-ci"
checks = ["tests"]
review = false
merge = false

[[agents]]
principal = "svc-merge"
checks = []
review = false
merge = true
```

Agent capabilities are independent. Listing an agent does not implicitly permit it to review or merge. Check
publishers must be configured and can publish only names in their `checks` grant. A configured agent may review
or merge only when its corresponding boolean is true. People and agents use the same ref protocol and the same
exact-revision rule.

`.soulgit.toml` answers **whether a proposal is ready**. `policy.json` answers **whether this principal may move
this ref**. The two controls remain separate, and the merge operation must pass both.

## Contributor and subscriber workflow

```sh
# Publish the current commit as a proposal to main. The id is generated if omitted.
walgit proposal create --target main

# Replace the proposed revision, leasing the old head. Attestations become stale.
walgit proposal update <id>

# Humans and agents attest to the current head.
walgit proposal review <id> approved
walgit proposal check <id> tests passed --actor svc-ci
walgit proposal state <id> reviewing

# Inspect projected proposals and readiness.
walgit proposal list
walgit proposal ready <id>

# Add proposal refs to ordinary fetches, or poll and print proposal-ref events.
walgit proposal subscribe
walgit proposal watch --interval 15

# Merge using the strategy committed on the target branch.
walgit proposal merge <id>
```

`subscribe` adds this refspec and fetches immediately:

```text
+refs/soulgit/proposals/*:refs/remotes/<remote>/soulgit/proposals/*
```

This is SoulGit's safe form of “everyone gets it”: proposal refs and required objects arrive, but no local branch
or working tree is changed. The WAL event bridge offers lower-latency wakeups for services; `watch` is the simple
client-side polling foundation. Both are at-least-once and consumers remain idempotent by proposal id plus head.

## Web and agent API

`GET /{owner}/{repo}/api/proposals/{id}` returns the projected proposal, commit title/description, merge rules,
and computed readiness. Authenticated callers can use:

```text
POST /{owner}/{repo}/api/proposals/{id}/reviews  {"decision":"approved"}
POST /{owner}/{repo}/api/proposals/{id}/checks   {"name":"tests","result":"passed"}
POST /{owner}/{repo}/api/proposals/{id}/merge
```

The dependency-free `web/sdk/repos.ts` exposes the same operations. The web merge endpoint performs
fast-forward merges; the CLI additionally creates merge commits and squash commits locally before publishing.

## Atomic merge boundary

A merge is one atomic ref transaction containing:

1. the target branch lease and new commit;
2. the proposal's `merged` state;
3. its result commit; and
4. a no-op lease on the exact proposal head that was reviewed.

If the target or proposal changes concurrently, none of those refs move. The transaction is then evaluated by
push policy and committed through the object-store manifest CAS. A notification, peer advertisement, review, or
check can never move a canonical branch by itself.

## Authority and peer delivery

- The object-store manifest CAS remains the only commit point and authoritative node-of-record.
- Any serving node is disposable and can reconstruct accepted state from the bucket.
- Proposal delivery is notification plus normal Git fetch today.
- A future peer layer may advertise and exchange immutable Git objects, Soulseek-style, but all objects remain
  hash-verified and the bucket is the correctness fallback.
- Cryptographic attestation envelopes and NAT-aware peer discovery are later transport/security layers; neither
  requires changing the proposal ref contract.
