//! Developer-side SoulGit proposal commands.

use std::process::Command;

use anyhow::{Context, Result, bail};
use soulgit_proposals::{
    CheckResult, ProposalState, ReviewDecision, author_ref, check_ref, head_ref, project,
    review_ref, state_ref, target_ref, validate_actor, validate_id, validate_target,
    PROPOSAL_PREFIX,
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
            runner,
            result,
            remote,
        } => check(&id, &remote, &runner, &result),
        ProposalAction::State { id, state, remote } => set_state(&id, &remote, &state),
        ProposalAction::List { remote } => list(&remote),
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
        &["/target/", "/author/", "/state/"],
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
    println!("stale results  retained for audit, ignored by projection");
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

fn check(id: &str, remote: &str, runner: &str, result: &str) -> Result<()> {
    validate_id(id)?;
    validate_actor(runner)?;
    let result: CheckResult = result.parse()?;
    let refs = remote_refs(remote, id)?;
    let head = current_head(id, &refs)?;
    let marker = format!("/checks/{runner}/");
    let desired = check_ref(id, runner, result)?;
    let mut refspecs = delete_matching_except(&refs, &[marker.as_str()], &[&desired]);
    refspecs.push(format!("+{head}:{desired}"));
    push_atomic(remote, None, refspecs)?;
    println!("proposal       {id}");
    println!("runner         {runner}");
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

#[cfg(test)]
mod tests {
    use super::delete_matching_except;

    #[test]
    fn deletes_only_the_selected_metadata_family() {
        let refs = vec![
            ("refs/soulgit/proposals/p/state/open".into(), "a".into()),
            ("refs/soulgit/proposals/p/checks/tests/passed".into(), "a".into()),
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
