import { useState, useTransition } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../api";
import { Box } from "../components/Layout";
import { invalidate, reportError, useData } from "../data";
import { useRepo } from "./RepoLayout";

export function ProposalPage() {
  const { full } = useRepo();
  const { proposalId = "" } = useParams();
  const detail = useData(`proposal:${full}:${proposalId}`, () => api.proposal(full, proposalId));
  const [pending, startTransition] = useTransition();
  const [notice, setNotice] = useState("");
  const { proposal, readiness } = detail;

  const refresh = () => {
    invalidate(`proposal:${full}:${proposalId}`);
    invalidate(`proposals:${full}:`);
  };
  const act = (label: string, action: () => Promise<unknown>) => {
    setNotice("");
    startTransition(async () => {
      try {
        await action();
        setNotice(label);
        refresh();
      } catch (error) {
        reportError(error, `proposal ${proposalId}`);
      }
    });
  };

  return (
    <section className="proposal-detail" aria-labelledby="proposal-title">
      <div className="proposal-detail-head">
        <div>
          <Link to={`/${full}/proposals`} className="muted small">← Proposal inbox</Link>
          <h2 id="proposal-title">{detail.title}</h2>
          <div className="row gap wrap proposal-detail-meta">
            <span className={`proposal-state state-${proposal.state}`}>{proposal.state.replace("-", " ")}</span>
            <span className="muted">{proposal.id}</span>
            <code>{proposal.head.slice(0, 12)}</code>
            <span className="muted">→ {proposal.target}</span>
          </div>
        </div>
        <div className="proposal-actions">
          <button className="btn" disabled={pending} onClick={() => act("Approval recorded", () => api.reviewProposal(full, proposalId, "approved"))}>
            Approve
          </button>
          <button className="btn" disabled={pending} onClick={() => act("Changes requested", () => api.reviewProposal(full, proposalId, "changes-requested"))}>
            Request changes
          </button>
          <button
            className="btn primary"
            disabled={pending || !readiness.ready}
            title={readiness.ready ? `Merge using ${detail.merge.strategy}` : readiness.blockers.join("; ")}
            onClick={() => act("Merged into the canonical branch", () => api.mergeProposal(full, proposalId))}
          >
            {pending ? "Working…" : `Merge into ${proposal.target}`}
          </button>
        </div>
      </div>
      {notice ? <div className="flash success proposal-notice">{notice}</div> : null}

      <div className="proposal-detail-grid">
        <div>
          <Box title="Proposal">
            <div className="pad proposal-description">
              {detail.description ? <p>{detail.description}</p> : <p className="muted">No commit description.</p>}
              <p className="muted small">Proposed by {proposal.author}. The commit message is the proposal title and description.</p>
            </div>
          </Box>
          <Box title="Reviews" className="proposal-panel">
            {proposal.reviews.length ? proposal.reviews.map((review) => (
              <div className="proposal-status-row" key={review.reviewer}>
                <span>{review.reviewer}</span>
                <strong className={review.decision === "approved" ? "signal-good" : "signal-bad"}>{review.decision.replace("-", " ")}</strong>
              </div>
            )) : <div className="pad muted">No reviews yet.</div>}
          </Box>
          <Box title="Agent checks" className="proposal-panel">
            {proposal.checks.length ? proposal.checks.map((check) => (
              <div className="proposal-status-row" key={`${check.actor}:${check.name}`}>
                <span><strong>{check.name}</strong> <span className="muted">by {check.actor}</span></span>
                <strong className={check.result === "passed" ? "signal-good" : check.result === "failed" ? "signal-bad" : "muted"}>{check.result}</strong>
              </div>
            )) : <div className="pad muted">No agent checks published.</div>}
          </Box>
        </div>
        <aside>
          <Box title={readiness.ready ? "Ready to merge" : "Not ready"}>
            <div className="pad readiness-card">
              <div><strong>{readiness.approvals}/{readiness.approvals_required}</strong><span className="muted"> approvals</span></div>
              <div><strong>{detail.merge.required_checks.length - readiness.missing_checks.length}/{detail.merge.required_checks.length}</strong><span className="muted"> required checks</span></div>
              <div><span className="muted">Strategy:</span> {detail.merge.strategy}</div>
              {readiness.blockers.length ? <ul>{readiness.blockers.map((blocker) => <li key={blocker}>{blocker}</li>)}</ul> : <p className="signal-good">Every configured gate has passed.</p>}
            </div>
          </Box>
          <Box title="Revision contract" className="proposal-panel">
            <div className="pad muted small">Reviews and checks count only for <code>{proposal.head.slice(0, 12)}</code>. Updating the proposal invalidates them automatically.</div>
          </Box>
        </aside>
      </div>
    </section>
  );
}
