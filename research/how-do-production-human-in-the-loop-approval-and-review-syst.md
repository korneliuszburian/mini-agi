## Findings

### 1. How production systems present approval/review queues to operators

- **Dedicated personal inbox / "awaiting my review" queue.** GitHub routes review requests to specific people or teams and gives each reviewer a filterable "awaiting your review" list (`https://github.com/pulls/review-requested`). Each PR is decided with one of three discrete outcomes — Comment, Approve, Request changes. — *GitHub Docs, "About pull request reviews"*. [fact]

- **Per-item status badges in a master list + an inline decision widget.** GitLab shows an approval-state icon on each merge request in the project/group MR list ("required approvals missing" vs "approvals satisfied"), and a widget on the MR itself showing per-reviewer state (Awaiting review / In progress / Approved / Commented / Requested changes). An eligible reviewer approves from the widget or via the `/approve` quick action. — *GitLab Docs, "Merge request approvals"*. [fact]

- **Prioritized triage list with a dual list/detail view.** Stripe Radar presents a *review queue* — "a prioritized list of completed or to-be-captured payments" — in two modes: a **list view** to scan without opening details, and a **detailed view** with customizable risk context. Operators navigate with the `J`/`K` keys or Previous/Next buttons, and act on each item (Approve / Refund / Refund-and-report-fraud). — *Stripe Docs, "Review payments"*. [fact]

- **Approval as a blocking stage in a pipeline, surfaced in the pipeline visualization.** AWS CodePipeline's *Manual approval* action stops a stage; approvers see it in the console with an optional review URL, comments, and an SNS notification when it is ready. — *AWS CodePipeline User Guide, "Add a manual approval action"*. [fact]

- **Approval as a per-rollout gate with a diff-to-review.** Google Cloud Deploy shows a **Review** link in the delivery-pipeline visualization when a rollout is pending approval; clicking it lists "rollouts pending approval," and each one opens an *Approve rollout* screen with a **Manifest diff** tab showing exactly what changed versus the current deploy, then **Approve** or **Reject** (console or `gcloud deploy rollouts approve|reject`). — *Google Cloud Docs, "Promote your release and manage approvals"*. [fact]

- **Pause-and-resume inside a CI/CD run.** Azure Pipelines' `ManualValidation@0` task pauses a YAML pipeline and shows a message bar linking to a *Manual validation* dialog containing the `instructions`; users with Queue-builds permission choose resume or reject. — *Microsoft Learn, "ManualValidation@0 – Manual validation task"*. [fact] Spinnaker's *Manual Judgment* stage likewise waits for the user to click **Continue**, with optional instructions and user-selectable input options that drive downstream pipeline behavior. — *Spinnaker Docs, "Pipeline Stages"*. [fact]

- **Approval requests mirrored into the collaboration inbox.** LaunchDarkly delivers approval requests to an **Approvals dashboard**, email, and in-app inbox, plus per-object "Pending changes" panels and Slack/Teams notifications. — *LaunchDarkly Docs, "Approvals"*. [fact]

- **Human-review loops triggered on demand from an ML service.** Amazon A2I presents review tasks to a configured *work team* (internal workforce, Mechanical Turk, or AWS-prescreened vendors) when a flow-definition condition fires; the work team sees items via a human task UI, and results land in S3 with CloudWatch Events on completion. — *AWS Rekognition/SageMaker Developer Guides, "Reviewing inappropriate content with Amazon Augmented AI" / "Using Amazon Augmented AI for Human Review"*. [fact]

### 2. Patterns that reduce approval fatigue

- **Automatically spread load across eligible reviewers (routing algorithms).** GitHub's code review *auto assignment* picks reviewers by one of two algorithms: **round robin** (who has received the least recent request) or **load balance** (counts recent and outstanding reviews, aiming for an equal number per member within any 30-day period); members whose status is "Busy" are excluded. — *GitHub Docs, "Managing code review settings for your team"*. [fact]

- **Workload-aware reviewer selection.** GitLab's *Automatic reviewer assignment* assigns CODEOWNERS automatically; the GitLab Duo Agent Platform strategy chooses the *minimum* number of reviewers needed to satisfy each approval rule, weighing availability, **review workload (open MRs awaiting their review)**, local time, and recency of activity. — *GitLab Docs, "Automatic reviewer assignment"*. [fact]

- **Sequential decision fatigue is measurable in expert reviewers.** A peer-reviewed field study of 1,112 parole rulings by experienced judges found the share of favorable rulings fell from ≈65% to near zero within each decision session and jumped back to ≈65% after a break; the authors argue the depletion comes from the *act of deciding*, not elapsed time. — *Danziger, Levav & Avnaim-Pesso, PNAS 108(17):6889–6892 (2011), full text at NCBI PMC3084045*. [fact — the observational result; the causal "mental depletion" interpretation is the authors' and is contested in later literature] [uncertain]

- **Constrain human review to the cases where it adds value.** Stripe's best practice is to focus reviewer time on payments where human judgment adds insight and let automation decide the majority, and to avoid adding a review step where no inherent fulfillment delay exists (it becomes a bottleneck for good customers). — *Stripe Docs, "Review payments"*. [opinion — vendor guidance, not measured evidence]

- **Tune routing by confidence thresholds, sampling, and adjustable rules instead of reviewing everything.** A2I human-review loops fire on **confidence-range conditions** (e.g., review a label when its confidence is below X or above Y) or on **random sampling** of a percentage of items; thresholds are adjustable over time. — *AWS Rekognition Developer Guide, "Reviewing inappropriate content with Amazon Augmented AI"*. [fact] Stripe's Radar lets operators write *rules* that automatically place payments into review and provides Smart Refunds recommendations ranked by refund-confidence, so operators triage a ranked list rather than scan everything. — *Stripe Docs, "Review payments"*. [fact]

- **Auto-close / timeouts prevent queue stagnation and blind approval.** If a customer disputes a payment already in Stripe's review queue, the review is automatically closed. — *Stripe Docs, "Review payments"*. [fact] Azure's Manual Validation defaults `onTimeout: reject` (reject or auto-resume after a configured timeout, up to 30 days), so a stalled approval cannot silently hold a deployment. — *Microsoft Learn, "ManualValidation@0"*. [fact] GitLab *auto-approves approval rules that are impossible to satisfy* (e.g., zero eligible approvers) so MRs are not blocked forever — *except* for policy-created rules, which instead block the MR and show "Action required." — *GitLab Docs, "Merge request approvals"*. [fact]

- **Scope approvals to what actually needs a human.** LaunchDarkly lets Enterprise accounts *require* approvals only for specific environments (e.g., prod but not dev), with per-role review permissions. — *LaunchDarkly Docs, "Approvals"*. [fact] AWS restricts who may update an approval via IAM identities. — *AWS CodePipeline User Guide*. [fact]

### 3. Patterns for batching and processing decisions efficiently

- **Ranked/filterable queues instead of raw FIFO.** Stripe's queue is *prioritized*, filterable (e.g., "Smart Refunds" quick filter for high/very-high fraud-likelihood; "my reviews" vs "unassigned"), with a dense list view for rapid scanning and a keyboard-driven detail view for the slow lane. — *Stripe Docs, "Review payments"*. [fact]

- **Claim/assignment semantics to prevent duplicate work across a shared queue.** Stripe reviewers assign themselves to reviews, see who is working what, and filter to unassigned or owned items; assignment changes are recorded in the review timeline. — *Stripe Docs, "Review payments"*. [fact]

- **Batch *sampling* and batch *recommendations*, not bulk approve.** Verified systems do not ship a "bulk approve everything" action; batch efficiency comes from (a) batching *which items* enter review (rules, thresholds, random-sampling percentages in A2I), and (b) batching *recommendations* (Stripe Smart Refunds ranked list). Each high-risk decision still requires per-item review with a diff — Cloud Deploy shows the manifest diff; AWS attaches a review URL + comments. — *Stripe Docs; AWS Rekognition Developer Guide; Google Cloud Docs "Promote your release and manage approvals"*. [fact — observations of product behavior] 

- **Programmatic / tool-mediated approvals scale batching to workflow systems.** Cloud Deploy publishes approval-required notifications to the `clouddeploy-approvals` Pub/Sub topic, explicitly so an external workflow system (e.g., ServiceNow) can approve or reject rollouts via the API — enabling bulk/policy-driven approvals outside the human console. — *Google Cloud Docs, "Promote your release and manage approvals"*. [fact]

### 4. Audit-trail patterns

- **Immutable, per-object decision timeline.** Stripe's review timeline "shows a complete history of assignment changes and other actions," and the API emits `review.opened` / `review.closed` events with the close `reason`. — *Stripe Docs, "Review payments"*. [fact]

- **Approval actions generate infrastructure audit logs.** Every Cloud Deploy `ApproveRollout` (and `RejectRollout`/advance/cancel/rollback) produces an **Admin Activity audit log** entry, queryable in Cloud Logging by `protoPayload.methodName`; the docs enumerate which methods are Admin Activity vs Data Access and which IAM permission type each requires. — *Google Cloud Docs, "Cloud Deploy audit logging"*. [fact]

- **Review outcome is durable, not reversible.** Cloud Deploy records a rejected rollout with state `APPROVAL_REJECTED`, and a rejected rollout "can't be approved later unless re-promoted." — *Google Cloud Docs, "Promote your release and manage approvals"*. [fact]

- **Reviewer identity and decision state are attached to the artifact.** GitLab records who approved and shows per-reviewer state on the MR; GitHub keeps review conversations "in the pull request timeline so the team can track feedback and decisions"; GitLab also freezes existing MRs against later approval-rule changes (rule overrides do not apply retroactively). — *GitLab Docs, "Merge request approvals"; GitHub Docs, "About pull request reviews"*. [fact]

- **Human-review results are written to storage and streamed to subscribers.** A2I writes every review outcome to the flow definition's S3 output path and notifies CloudWatch Events on completion; `DescribeHumanLoop` returns the loop's outcome. — *AWS Rekognition/SageMaker Developer Guides*. [fact]

## Sources

- GitHub Docs — *About pull request reviews*: https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/reviewing-changes-in-pull-requests/about-pull-request-reviews
- GitHub Docs — *Managing code review settings for your team*: https://docs.github.com/en/organizations/organizing-members-into-teams/managing-code-review-settings-for-your-team
- GitLab Docs — *Merge request approvals*: https://docs.gitlab.com/ee/user/project/merge_requests/approvals/
- GitLab Docs — *Automatic reviewer assignment*: https://docs.gitlab.com/ee/user/project/merge_requests/reviews/automatic_reviewer_assignment/
- AWS CodePipeline User Guide — *Add a manual approval action*: https://docs.aws.amazon.com/codepipeline/latest/userguide/approvals-action-add.html
- Google Cloud Docs — *Promote your release and manage approvals*: https://cloud.google.com/deploy/docs/promote-release
- Google Cloud Docs — *Cloud Deploy audit logging*: https://cloud.google.com/deploy/docs/audit-logs
- Microsoft Learn — *ManualValidation@0 task*: https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/manual-validation-v0
- Spinnaker Docs — *Pipeline Stages (Manual Judgment)*: https://spinnaker.io/docs/reference/pipeline/stages/
- LaunchDarkly Docs — *Approvals*: https://launchdarkly.com/docs/home/releases/approvals.md
- Stripe Docs — *Review payments*: https://docs.stripe.com/radar/reviews
- AWS SageMaker Developer Guide — *Using Amazon Augmented AI for Human Review*: https://docs.aws.amazon.com/sagemaker/latest/dg/a2i-use-augmented-ai-a2i-human-review-loops.html
- AWS Rekognition Developer Guide — *Reviewing inappropriate content with Amazon Augmented AI*: https://docs.aws.amazon.com/rekognition/latest/dg/a2i-rekognition.html
- AWS SageMaker Developer Guide — *Label verification and adjustment*: https://docs.aws.amazon.com/sagemaker/latest/dg/sms-verification-data.html
- Danziger, Levav & Avnaim-Pesso, *Extraneous factors in judicial decisions*, PNAS 108(17):6889–6892 (2011), DOI 10.1073/pnas.1018033108 — full text: https://pmc.ncbi.nlm.nih.gov/articles/PMC3084045/

## Verdict

**Established.** Queue presentation across nine production systems converges on: a personal "awaiting you" inbox (GitHub, LaunchDarkly) or a shared prioritized queue (Stripe), with per-item decision widgets, list-vs-detail views, and approvals surfaced as blocking gates inside pipeline visualizations (AWS, Cloud Deploy, Azure, Spinnaker). Fatigue countermeasures are built into the products themselves: load-balancing/round-robin reviewer routing (GitHub, GitLab), workload/availability-aware assignment (GitLab), confidence-threshold and sampling-based routing (A2I), rule-driven triage and ranked recommendations (Stripe), scope-limited required approvals (LaunchDarkly), and timeout/auto-close semantics (Azure, Stripe, GitLab's auto-approval of unsatisfiable rules). Audit trails are standard: immutable per-item timelines + webhook events (Stripe), Admin Activity audit logs for each approval (Cloud Deploy), durable reject states (Cloud Deploy), reviewer identity on the artifact (GitLab/GitHub), and results persisted to object storage with completion events (A2I). The peer-reviewed fatigue evidence is real: sequential expert decisions degrade measurably across a session (Danziger et al. 2011).

**Uncertain.** No vendor publishes controlled evaluations proving that any specific feature (e.g., load-balancing routing) measurably reduces errors or approval latency — the fatigue-mitigation claims are product design rationale, not measured outcomes. The Danziger findings are observational and their "mental depletion" mechanism is contested. "Batching decisions" in the sources means batching *routing and recommendations*, not bulk-approving high-risk items; bulk approve appears to be deliberately absent for consequential approvals. The PNAS paper's original PDF was not read directly, but its full open-access text was verified on NCBI PMC; the GitLab and Cloud Deploy pages initially served HTTP 403/404 on alternate URLs but were verified at the cited canonical URLs.

**What would settle it.** (a) A controlled or quasi-experimental study measuring reviewer decision quality (error rates vs a ground-truth audit) as a function of queue length, ordinal position, and routing fairness — the platforms publish no such telemetry. (b) Vendor case studies reporting pre/post metrics (e.g., approval latency, % items requiring a second look, reviewer disagreement rate) after enabling auto-assignment, thresholds, or sampling. (c) For batch-vs-per-item trade-offs, an operator-side A/B measurement of error rate under sampled-review vs full-review workload would give evidence the docs currently lack.
