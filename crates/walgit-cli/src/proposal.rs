//! Developer-side SoulGit proposal commands.

use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use soulgit_proposals::{
    CheckResult, MergeStrategy, Proposal, ProposalState, ReviewDecision, SoulGitConfig,
    author_ref, check_ref, head_ref, project, result_ref, review_ref, state_ref, target_ref,
    validate_actor, validate_id, validate_target, PROPOSAL_PREFIX,
};

use crate::ProposalAction;

pub fn run(action: ProposalAction) -> Result<()> {
    match action {
        ProposalAction::Create {
            id,
            remote,
            target,
            author,
            head,
        } => create(id, &remote, &target, author.as_deref(), &head),
        ProposalAction::Update { id, remote, head } => update(&id, &remote, &head),
        ProposalAction::Review {
            id,
            decision,
            remote,
            reviewer,
        } => review(&id, &remote, reviewer.as_deref(), &decision),
        ProposalAction::Check {
            id,
            name,
            result,
            remote,
            actor,
        } => check(&id, &remote, actor.as_deref(), &name, &result),
        ProposalAction::State { id, state, remote } => set_state(&id, &remote, &state),
        ProposalAction::List { remote } => list(&remote),
        ProposalAction::Ready { id, remote } => ready(&id, &remote).map(|_| ()),
        ProposalAction::Merge { id, remote } => merge(&id, &remote),
        ProposalAction::Subscribe { remote } => subscribe(&remote),
        ProposalAction::Watch {
            remote,
            interval,
            once,
        } => watch(&remote, interval, once),
    }
}

fn create(
    id: Option<String>,
    remote: &str,
    target: &str,
    author: Option<&str>,
    head: &str,
) -> Result<()> {
    let id = id.unwrap_or_else(|| format!("p-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]));
    validate_id(&id)?;
    validate_target(target)?;
    let author = actor(author)?;
    let oid = rev_parse(head)?;
    let head_name = head_ref(&id)?;
    let refs = [
        head_name.clone(),
        target_ref(&id, target)?,
        author_ref(&id, &author)?,
        state_ref(&id, ProposalState::Open)?,
    ];
    let mut args = vec![
        "push".to_string(),
        "--atomic".to_string(),
        format!("--force-with-lease={head_name}:"),
        remote.to_string(),
    ];
    args.extend(refs.iter().map(|name| format!("{oid}:{name}")));
    git(&args)?;
    println!("proposal       {id}");
    println!("head           {oid}");
    println!("target         {target}");
    println!("author         {author}");
    Ok(())
}

fn update(id: &str, remote: &str, head: &str) -> Result<()> {
    validate_id(id)?;
    let remote_refs = remote_refs(remote, id)?;
    let current = project(remote_refs.iter().map(|(name, oid)| (name.as_str(), oid.as_str())))
        .pop()
        .ok_or_else(|| anyhow::anyhow!("proposal `{id}` does not exist on `{remote}`"))?;
    let oid = rev_parse(head)?;
    let head_name = head_ref(id)?;
    let target_name = target_ref(id, &current.target)?;
    let author_name = author_ref(id, &current.author)?;
    let state_name = state_ref(id, ProposalState::Open)?;
    let mut refspecs = delete_matching_except(
        &remote_refs,
        &["/target/", "/author/", "/state/", "/result"],
        &[&target_name, &author_name, &state_name],
    );
    refspecs.extend([
        format!("+{oid}:{head_name}"),
        format!("+{oid}:{target_name}"),
        format!("+{oid}:{author_name}"),
        format!("+{oid}:{state_name}"),
    ]);
    push_atomic(remote, Some((&head_name, &current.head)), refspecs)?;
    println!("proposal       {id}");
    println!("head           {oid}");
    println!("state          open");
    println!("attestations   prior reviews and checks are now stale");
    Ok(())
}

fn review(id: &str, remote: &str, reviewer: Option<&str>, decision: &str) -> Result<()> {
    validate_id(id)?;
    let reviewer = actor(reviewer)?;
    let decision: ReviewDecision = decision.parse()?;
    let refs = remote_refs(remote, id)?;
    let head = current_head(id, &refs)?;
    let marker = format!("/reviews/{reviewer}/");
    let desired = review_ref(id, &reviewer, decision)?;
    let mut refspecs = delete_matching_except(&refs, &[marker.as_str()], &[&desired]);
    refspecs.push(format!("+{head}:{desired}"));
    push_atomic(remote, None, refspecs)?;
    println!("proposal       {id}");
    println!("reviewer       {reviewer}");
    println!("decision       {decision}");
    println!("head           {head}");
    Ok(())
}

fn check(
    id: &str,
    remote: &str,
    explicit_actor: Option<&str>,
    name: &str,
    result: &str,
) -> Result<()> {
    validate_id(id)?;
    validate_actor(name)?;
    let actor = actor(explicit_actor)?;
    let result: CheckResult = result.parse()?;
    let refs = remote_refs(remote, id)?;
    let head = current_head(id, &refs)?;
    let marker = format!("/checks/{actor}/{name}/");
    let desired = check_ref(id, &actor, name, result)?;
    let mut refspecs = delete_matching_except(&refs, &[marker.as_str()], &[&desired]);
    refspecs.push(format!("+{head}:{desired}"));
    push_atomic(remote, None, refspecs)?;
    println!("proposal       {id}");
    println!("actor          {actor}");
    println!("check          {name}");
    println!("result         {result}");
    println!("head           {head}");
    Ok(())
}

fn set_state(id: &str, remote: &str, state: &str) -> Result<()> {
    validate_id(id)?;
    let state: ProposalState = state.parse()?;
    let refs = remote_refs(remote, id)?;
    let head = current_head(id, &refs)?;
    let desired = state_ref(id, state)?;
    let mut refspecs = delete_matching_except(&refs, &["/state/"], &[&desired]);
    refspecs.push(format!("+{head}:{desired}"));
    push_atomic(remote, None, refspecs)?;
    println!("proposal       {id}");
    println!("state          {state}");
    println!("head           {head}");
    Ok(())
}

fn list(remote: &str) -> Result<()> {
    let refs = remote_refs(remote, "*")?;
    let proposals = project(refs.iter().map(|(name, oid)| (name.as_str(), oid.as_str())));
    if proposals.is_empty() {
        println!("(no proposals)");
        return Ok(());
    }
    for proposal in proposals {
        println!(
            "{:<24} {:<18} {:<20} {} -> {}",
            proposal.id,
            proposal.state,
            proposal.author,
            &proposal.head[..proposal.head.len().min(12)],
            proposal.target,
        );
    }
    Ok(())
}

fn ready(id: &str, remote: &str) -> Result<(Proposal, SoulGitConfig)> {
    validate_id(id)?;
    let refs = remote_refs(remote, id)?;
    let proposal = project(refs.iter().map(|(name, oid)| (name.as_str(), oid.as_str())))
        .pop()
        .ok_or_else(|| anyhow::anyhow!("proposal `{id}` does not exist on `{remote}`"))?;
    let target_ref_name = format!("refs/heads/{}", proposal.target);
    let target_oid = remote_ref(remote, &target_ref_name)?
        .ok_or_else(|| anyhow::anyhow!("target branch `{}` does not exist", proposal.target))?;
    fetch_proposal_objects(remote, &proposal)?;
    let config = load_soulgit_config(&target_oid)?;
    let readiness = proposal.readiness(&config);
    println!("proposal       {}", proposal.id);
    println!("head           {}", proposal.head);
    println!("target         {}", proposal.target);
    println!(
        "approvals      {}/{}",
        readiness.approvals, readiness.approvals_required
    );
    println!(
        "checks         {}",
        if config.merge.required_checks.is_empty() {
            "none required".to_string()
        } else if readiness.missing_checks.is_empty() {
            "passed".to_string()
        } else {
            format!("missing {}", readiness.missing_checks.join(", "))
        }
    );
    println!("ready          {}", readiness.ready);
    for blocker in &readiness.blockers {
        println!("blocker        {blocker}");
    }
    Ok((proposal, config))
}

fn merge(id: &str, remote: &str) -> Result<()> {
    let (proposal, config) = ready(id, remote)?;
    let readiness = proposal.readiness(&config);
    if !readiness.ready {
        bail!("proposal `{id}` is not ready to merge");
    }
    let who = actor(None)?;
    if let Some(agent) = config.agent(&who) {
        if !agent.merge {
            bail!("agent `{who}` is not allowed to merge by .soulgit.toml");
        }
    }

    let target_name = format!("refs/heads/{}", proposal.target);
    let target_oid = remote_ref(remote, &target_name)?
        .ok_or_else(|| anyhow::anyhow!("target branch `{}` does not exist", proposal.target))?;
    fetch_proposal_objects(remote, &proposal)?;
    let merge_oid = build_merge_commit(&proposal, &target_oid, config.merge.strategy)?;
    let proposal_refs = remote_refs(remote, id)?;
    let merged_state = state_ref(id, ProposalState::Merged)?;
    let result = result_ref(id)?;
    let mut refspecs = delete_matching_except(
        &proposal_refs,
        &["/state/", "/result"],
        &[&merged_state, &result],
    );
    refspecs.extend([
        format!("{}:{}", proposal.head, head_ref(id)?),
        format!("{}:{target_name}", merge_oid),
        format!("+{}:{merged_state}", proposal.head),
        format!("+{merge_oid}:{result}"),
    ]);
    let proposal_head_ref = head_ref(id)?;
    push_atomic_leases(
        remote,
        &[
            (proposal_head_ref.as_str(), proposal.head.as_str()),
            (&target_name, target_oid.as_str()),
        ],
        refspecs,
    )?;
    println!("merged         {id}");
    println!("strategy       {:?}", config.merge.strategy);
    println!("commit         {merge_oid}");
    println!("target         {}", proposal.target);
    Ok(())
}

fn build_merge_commit(
    proposal: &Proposal,
    target_oid: &str,
    strategy: MergeStrategy,
) -> Result<String> {
    if strategy == MergeStrategy::FastForward {
        let status = Command::new("git")
            .args(["merge-base", "--is-ancestor", target_oid, &proposal.head])
            .status()
            .context("check fast-forward ancestry")?;
        if !status.success() {
            bail!("proposal is not a fast-forward of its current target");
        }
        return Ok(proposal.head.clone());
    }
    let output = git_capture(&["merge-tree", "--write-tree", target_oid, &proposal.head])?;
    let tree = output
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("git merge-tree returned no tree"))?;
    let message = format!("Merge SoulGit proposal {}", proposal.id);
    let mut args = vec!["commit-tree", tree, "-p", target_oid];
    if strategy == MergeStrategy::MergeCommit {
        args.extend(["-p", proposal.head.as_str()]);
    }
    args.extend(["-m", message.as_str()]);
    git_capture(&args)
}

fn load_soulgit_config(target: &str) -> Result<SoulGitConfig> {
    let spec = format!("{target}:.soulgit.toml");
    let output = Command::new("git")
        .args(["show", spec.as_str()])
        .output()
        .context("read .soulgit.toml")?;
    if output.status.success() {
        return SoulGitConfig::parse(&String::from_utf8(output.stdout)?)
            .map_err(anyhow::Error::msg);
    }
    Ok(SoulGitConfig::default())
}

fn fetch_proposal_objects(remote: &str, proposal: &Proposal) -> Result<()> {
    git(&[
        "fetch".to_string(),
        "--no-tags".to_string(),
        remote.to_string(),
        head_ref(&proposal.id)?,
        format!("refs/heads/{}", proposal.target),
    ])
}

fn subscribe(remote: &str) -> Result<()> {
    ensure_subscription(remote)?;
    git(&["fetch".to_string(), remote.to_string()])?;
    println!("subscribed     {remote}");
    println!("local refs     refs/remotes/{remote}/soulgit/proposals/*");
    Ok(())
}

fn ensure_subscription(remote: &str) -> Result<()> {
    let spec = format!("+{PROPOSAL_PREFIX}*:refs/remotes/{remote}/soulgit/proposals/*");
    let key = format!("remote.{remote}.fetch");
    let existing = git_capture_optional(&["config", "--get-all", &key])?;
    if !existing.lines().any(|line| line == spec) {
        git(&[
            "config".to_string(),
            "--add".to_string(),
            key,
            spec.clone(),
        ])?;
    }
    Ok(())
}

fn watch(remote: &str, interval: u64, once: bool) -> Result<()> {
    if interval == 0 {
        bail!("--interval must be greater than zero");
    }
    ensure_subscription(remote)?;
    let mut before = local_proposal_refs(remote)?;
    loop {
        git(&["fetch".to_string(), "--quiet".to_string(), remote.to_string()])?;
        let after = local_proposal_refs(remote)?;
        for (name, oid) in &after {
            if before.get(name) != Some(oid) {
                println!("proposal event {oid} {name}");
            }
        }
        for name in before.keys() {
            if !after.contains_key(name) {
                println!("proposal event deleted {name}");
            }
        }
        if once {
            return Ok(());
        }
        before = after;
        std::thread::sleep(Duration::from_secs(interval));
    }
}

fn local_proposal_refs(remote: &str) -> Result<std::collections::BTreeMap<String, String>> {
    let prefix = format!("refs/remotes/{remote}/soulgit/proposals/");
    let format = "%(refname) %(objectname)";
    let output = git_capture_optional(&["for-each-ref", &format!("--format={format}"), &prefix])?;
    Ok(output
        .lines()
        .filter_map(|line| {
            let (name, oid) = line.split_once(' ')?;
            Some((name.to_string(), oid.to_string()))
        })
        .collect())
}

fn actor(explicit: Option<&str>) -> Result<String> {
    let value = match explicit {
        Some(value) => value.to_string(),
        None => git_capture(&["config", "--get", "user.email"])
            .context("no proposal identity; configure git user.email or pass --author/--reviewer")?,
    };
    let value = value.trim().to_string();
    validate_actor(&value)?;
    Ok(value)
}

fn rev_parse(rev: &str) -> Result<String> {
    git_capture(&["rev-parse", &format!("{rev}^{{commit}}")])
}

fn current_head(id: &str, refs: &[(String, String)]) -> Result<String> {
    let expected = head_ref(id)?;
    refs.iter()
        .find(|(name, _)| name == &expected)
        .map(|(_, oid)| oid.clone())
        .ok_or_else(|| anyhow::anyhow!("proposal `{id}` has no head ref"))
}

fn remote_refs(remote: &str, id: &str) -> Result<Vec<(String, String)>> {
    let pattern = if id == "*" {
        format!("{PROPOSAL_PREFIX}*")
    } else {
        format!("{PROPOSAL_PREFIX}{id}/*")
    };
    let output = git_capture(&["ls-remote", "--refs", remote, &pattern])?;
    let mut refs = Vec::new();
    for line in output.lines() {
        let Some((oid, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        refs.push((name.trim().to_string(), oid.to_string()));
    }
    Ok(refs)
}

fn remote_ref(remote: &str, name: &str) -> Result<Option<String>> {
    let output = git_capture_optional(&["ls-remote", "--refs", remote, name])?;
    Ok(output.lines().find_map(|line| {
        let (oid, found) = line.split_once(char::is_whitespace)?;
        (found.trim() == name).then(|| oid.to_string())
    }))
}

fn delete_matching_except(
    refs: &[(String, String)],
    markers: &[&str],
    keep: &[&str],
) -> Vec<String> {
    refs.iter()
        .filter(|(name, _)| {
            !keep.contains(&name.as_str())
                && markers.iter().any(|marker| name.contains(marker))
        })
        .map(|(name, _)| format!(":{name}"))
        .collect()
}

fn push_atomic(
    remote: &str,
    lease: Option<(&str, &str)>,
    refspecs: Vec<String>,
) -> Result<()> {
    let mut args = vec!["push".to_string(), "--atomic".to_string()];
    if let Some((name, oid)) = lease {
        args.push(format!("--force-with-lease={name}:{oid}"));
    }
    args.push(remote.to_string());
    args.extend(refspecs);
    git(&args)
}

fn push_atomic_leases(
    remote: &str,
    leases: &[(&str, &str)],
    refspecs: Vec<String>,
) -> Result<()> {
    let mut args = vec!["push".to_string(), "--atomic".to_string()];
    args.extend(
        leases
            .iter()
            .map(|(name, oid)| format!("--force-with-lease={name}:{oid}")),
    );
    args.push(remote.to_string());
    args.extend(refspecs);
    git(&args)
}

fn git(args: &[String]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn git_capture(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn git_capture_optional(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if output.status.success() {
        return Ok(String::from_utf8(output.stdout)?.trim().to_string());
    }
    if output.status.code() == Some(1) {
        return Ok(String::new());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!("git {} failed: {stderr}", args.join(" "));
}

#[cfg(test)]
mod tests {
    use super::delete_matching_except;

    #[test]
    fn deletes_only_the_selected_metadata_family() {
        let refs = vec![
            ("refs/soulgit/proposals/p/state/open".into(), "a".into()),
            (
                "refs/soulgit/proposals/p/checks/ci@example.com/tests/passed".into(),
                "a".into(),
            ),
        ];
        assert_eq!(
            delete_matching_except(&refs, &["/state/"], &[]),
            [":refs/soulgit/proposals/p/state/open"]
        );
        assert!(delete_matching_except(
            &refs,
            &["/state/"],
            &["refs/soulgit/proposals/p/state/open"]
        )
        .is_empty());
    }
}
