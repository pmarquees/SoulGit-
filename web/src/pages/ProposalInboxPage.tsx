import { useState } from "react";
import { Link } from "react-router-dom";
import { api, type Proposal, type ProposalState } from "../api";
import { Box } from "../components/Layout";
import { useData } from "../data";
import { useRepo } from "./RepoLayout";

type Filter = "all" | ProposalState;
const filters: { value: Filter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "open", label: "Open" },
  { value: "reviewing", label: "Reviewing" },
  { value: "approved", label: "Approved" },
  { value: "changes-requested", label: "Changes requested" },
];

export function ProposalInboxPage() {
  const { full } = useRepo();
  const [filter, setFilter] = useState<Filter>("all");
  const query = filter === "all" ? {} : { state: filter };
  const page = useData(`proposals:${full}:${filter}`, () => api.proposals(full, query));

  return (
    <section aria-labelledby="proposal-heading">
      <div className="proposal-heading">
        <div>
          <h2 id="proposal-heading" className="page-title">Proposal inbox</h2>
          <p className="muted proposal-intro">Incoming work waits here until a human or authorized agent decides it is ready.</p>
        </div>
        <div className="proposal-filters" role="group" aria-label="Filter proposals">
          {filters.map(({ value, label }) => (
            <button key={value} type="button" className={filter === value ? "btn active" : "btn"} onClick={() => setFilter(value)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      <Box>
        {page.proposals.map((proposal) => <ProposalRow key={proposal.id} repo={full} proposal={proposal} />)}
        {page.proposals.length === 0 && (
          <div className="proposal-empty">
            <strong>No proposals in this view.</strong>
            <span className="muted">Create one with <code>walgit proposal create</code>; direct pushes remain governed by repository policy.</span>
          </div>
        )}
      </Box>
    </section>
  );
}

function ProposalRow({ repo, proposal }: { repo: string; proposal: Proposal }) {
  const approvals = proposal.reviews.filter((r) => r.decision === "approved").length;
  const passed = proposal.checks.filter((c) => c.result === "passed").length;
  const failed = proposal.checks.filter((c) => c.result === "failed").length;
  return (
    <article className="proposal-row">
      <div className="proposal-main">
        <div className="row gap wrap">
          <Link to={`/${repo}/proposals/${proposal.id}`} className="strong proposal-id">{proposal.id}</Link>
          <span className={`proposal-state state-${proposal.state}`}>{proposal.state.replace("-", " ")}</span>
        </div>
        <div className="muted small proposal-meta">
          <code>{proposal.head.slice(0, 10)}</code> → <code>{proposal.target}</code>
          {proposal.author && <> · proposed by {proposal.author}</>}
        </div>
      </div>
      <div className="proposal-signals" aria-label="Review status">
        <span title="Approvals">{approvals} approval{approvals === 1 ? "" : "s"}</span>
        <span className={failed ? "signal-bad" : "signal-good"}>{passed} passed{failed ? `, ${failed} failed` : ""}</span>
        {proposal.issues.length > 0 && <span className="signal-bad">{proposal.issues.length} issue{proposal.issues.length === 1 ? "" : "s"}</span>}
      </div>
    </article>
  );
}
