# SoulGit proposal protocol

SoulGit keeps Walgit's S3/GCS object store as the permanent authority and adds a Git-native inbox in front of
protected branches. It is intentionally closer to a mail inbox than an automatic merge queue: contributors
publish proposals; subscribers are notified and fetch them; humans or agents attest to an exact revision; an
authorized broker may merge after applying repository-specific readiness rules.

## Ref model

For proposal `<id>` at commit `<oid>`:

```text
refs/soulgit/proposals/<id>/head                         -> <oid>
refs/soulgit/proposals/<id>/target/<branch>              -> <oid>
refs/soulgit/proposals/<id>/author/<identity>            -> <oid>
refs/soulgit/proposals/<id>/state/<state>                -> <oid>
refs/soulgit/proposals/<id>/reviews/<actor>/<decision>   -> <oid>
refs/soulgit/proposals/<id>/checks/<name>/<result>       -> <oid>
```

`<state>` is `open`, `reviewing`, `changes-requested`, `approved`, `merging`, `merged`, `rejected`,
`superseded`, or `expired`. A review decision is `approved` or `changes-requested`; a check result is `pending`,
`passed`, `failed`, or `skipped`.

The head is the revision boundary. Metadata, reviews, and checks are active only when their ref points at the
current head OID. A proposal update moves the head and metadata atomically; existing attestations then become
stale automatically. The server projects this ref family into the inbox and never stores proposal state in a
database.

## Contributor and agent workflow

```sh
# Publish the current commit as a proposal to main. The id is generated if omitted.
walgit proposal create --target main

# Replace the proposed revision. This uses a force-with-lease equivalent on the old head.
walgit proposal update <id>

# Human or agent attestations bind to the current head.
walgit proposal review <id> approved
walgit proposal check <id> tests passed
walgit proposal state <id> reviewing
walgit proposal list
```

The proposal commands use the current repository's Git remote and `user.email`; they do not require a server
configuration file. Agents should receive credentials restricted to their own review/check ref namespace.
The existing WAL event bridge emits proposal changes as ordinary `ref` events, so an agent can filter on
`refs/soulgit/proposals/` and remain idempotent by proposal id plus head OID.

## Authority and merge boundary

- The object-store manifest CAS remains the only commit point.
- Protect `refs/heads/main` so contributors cannot update it directly.
- Grant the merge broker permission to update that ref; the broker evaluates readiness rules and then performs
  an ordinary conditional Git push.
- A notification, peer advertisement, review, or check cannot move a canonical branch by itself.
- Peer-assisted object exchange is a later transport optimization. Received objects are always hash-verified,
  and the authoritative bucket remains the fallback.

The current implementation is the first vertical slice: ref protocol and CLI, server projection and API/SDK,
agent-compatible event semantics, and the web inbox. Broker automation, cryptographic attestation envelopes,
and peer-assisted object delivery are the next layers; none require changing this ref contract.
