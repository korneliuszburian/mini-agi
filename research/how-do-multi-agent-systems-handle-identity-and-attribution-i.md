## Findings

All claims below are **fact** (sourced) unless labelled **opinion** (synthesis across the cited sources).

### 1. Typed "identity kind" fields are the core pattern for distinguishing humans vs. machines vs. services

- AWS CloudTrail records every request's principal in `userIdentity.type`, with an explicit enumeration: `Root`, `IAMUser`, `AssumedRole`, `Role`, `FederatedUser`, `Directory`, `AWSAccount`, `AWSService`, `IdentityCenterUser`, and `Unknown`; federation paths add `SAMLUser` / `WebIdentityUser`. `userName`, `principalId`, `arn`, and `accessKeyId` sit under the same element. Source: "CloudTrail userIdentity element", https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-event-reference-user-identity.html
- `AWSService` is explicitly defined as "The request was made by an AWS account that belongs to an AWS service" (e.g. Elastic Beanstalk assuming a role to call other services) — a dedicated kind for automated processes, distinct from human identity kinds. Same source, "Fields → type".
- GitHub's enterprise audit log records "The user (actor) who performed the action" plus, "For actions outside of the web UI, how the user (actor) authenticated". Source: "About the audit log for your enterprise", https://docs.github.com/en/enterprise-cloud@latest/admin/monitoring-activity-in-your-enterprise/reviewing-audit-logs-for-your-enterprise/about-the-audit-log-for-your-enterprise

### 2. Attribution-by-channel: system-initiated events are separated from user-driven events

- Google Cloud splits audit logs into four named classes: **Admin Activity** ("log entries written by user-driven API calls or other actions that modify the configuration or metadata of resources"), **Data Access**, **System Event** ("log entries written by Google Cloud systems that modify the configuration of resources... aren't driven by direct user action" — example: autoscaling adding/removing VMs), and **Policy Denied**. This gives a first-order filter: which channel an entry landed in encodes whether a human drove it. Source: "Cloud Audit Logs overview", https://cloud.google.com/logging/docs/audit
- The identity of the caller is carried in `AuditLog.authenticationInfo` (`principalEmail`), and the network origin in `RequestMetadata.callerIp`. For service-to-service calls inside Google's production network the `callerIp` is redacted to `private`, and this "includes calls made by Google-owned service accounts (service agents) even when initiated by a user" — i.e. the log records when a user-initiated call was executed by an automated service identity. Source: same "Cloud Audit Logs overview" page, "Caller identities in audit logs" / "IP address of the caller" sections.

### 3. Machine/service identities are first-class principals with distinct naming

- Kubernetes audit events record `user` = "Authenticated user information", plus `verb`, `sourceIPs`, `requestURI`, `requestReceivedTimestamp`, `responseStatus`, `userAgent`, and `annotations`. Source: "kube-apiserver Audit Configuration (v1)" (audit.k8s.io/v1 `Event`), https://kubernetes.io/docs/reference/config-api/apiserver-audit.v1/
- Kubernetes distinguishes principals by a username convention; audit policy rules match on `users` ("The users (by authenticated user name) this rule applies to") and `userGroups`. Documented example usernames include `system:kube-proxy`, group `system:authenticated`, and the impersonation example below uses `system:serviceaccount:default:my-controller`. Sources: "Auditing", https://kubernetes.io/docs/tasks/debug/debug-cluster/audit/ ; "User Impersonation", https://kubernetes.io/docs/reference/access-authn-authz/user-impersonation/
- `userAgent` is explicitly untrusted: "the UserAgent is provided by the client, and must not be trusted." Source: apiserver-audit.v1 reference (above).

### 4. Delegation chains preserve the originating human — "who acted as whom"

- AWS: `sessionContext.sourceIdentity` "identifies the original user identity making the request" across role assumption and role chaining (the field survives `AssumeRole` → service API calls); `sessionContext.sessionIssuer` records how temporary credentials were obtained; `onBehalfOf` carries the IAM Identity Center user a call was made for. Source: CloudTrail userIdentity element (above), "AWS STS source identity" and Fields sections.
- Kubernetes impersonation records **both** identities in the audit event: the authenticated caller in `user` and the impersonated identity in `impersonatedUser`. The documented example shows `"user": {"username": "system:serviceaccount:default:my-controller"}` and `"impersonatedUser": {"username": "jane.doe@example.com"}` — a service account acting as a human, with both recorded. Source: "User Impersonation", https://kubernetes.io/docs/reference/access-authn-authz/user-impersonation/ ("An audit event is logged for each impersonation request to help track how impersonation is used.")
- Google Cloud does the same via service accounts: a human's actions executed through Google-managed service agents are attributed with the originating sign-in context, and GCP notes that in multi-service flows "unless explicit identity delegation is in place, the service might propagate the immediate caller's origin". Source: "Cloud Audit Logs overview" (above).

### 5. Credential-level attribution lets audit trails be searched by token, not just actor

- GitHub audit log records `hashed_token` ("SHA-256 hash of the token used for authentication"), `programmatic_access_type`, and `token_scopes`; covered authentication methods are personal access token, OAuth token, GitHub Apps (as an app installation or on behalf of a user), deploy key, and SSH key. Any event attributable to a compromised token can be found by its hash. Source: "Identifying audit log events performed by an access token", https://docs.github.com/en/enterprise-cloud@latest/admin/monitoring-activity-in-your-enterprise/reviewing-audit-logs-for-your-enterprise/identifying-audit-log-events-performed-by-an-access-token
- AWS records `accessKeyId` (the specific credential that signed the request) and `credentialId` for bearer-token callers. Source: CloudTrail userIdentity element (above).

### 6. Agent-framework level: trace spans attribute each step to a named agent within a workflow

- The OpenAI Agents SDK traces every run as a **trace** (`workflow_name` — "the logical workflow or app", `trace_id`, optional `group_id` — "Optional group ID, to link multiple traces from the same conversation. For example, you might use a chat thread ID", and `metadata`) composed of **spans** linked by `trace_id`/`parent_id`. Attribution to an agent is explicit: "Each time an agent runs, it is wrapped in `agent_span()`"; handoffs between agents in `handoff_span()`; tool calls in `function_span()`; LLM calls in `generation_span()`; guardrails in `guardrail_span()`. Source: "Tracing — OpenAI Agents SDK", https://openai.github.io/openai-agents-python/tracing/
- So in a multi-agent framework the attribution unit is "which agent performed which step in which workflow, on behalf of which conversation (`group_id`)" — the human/end-user is linked at the trace level rather than the span level. (Synthesis of the trace schema; the SDK docs define the fields, the mapping to "human" is **opinion**.)

### 7. Agent tool-calls surface in the host platform's audit trail as service names

- Google Cloud MCP servers — the standard way agent frameworks invoke external tools — write Data Access audit logs with the service name format `SERVICE_NAME.googleapis.com/mcp`, enabling audit of agent-orchestrated tool calls inside the platform's audit log. Source: "Cloud Audit Logs overview" (above).

### 8. Immutability backs the audit trail as evidence

- Google Cloud: "Log entries written by Cloud Audit Logs are immutable." Source: "Cloud Audit Logs overview" (above).
- Kubernetes audit records "begin their lifecycle inside the kube-apiserver... pre-processed according to a certain policy and written to a backend. The current backend implementations include logs files and webhooks." Source: "Auditing", https://kubernetes.io/docs/tasks/debug/debug-cluster/audit/

## Sources

1. AWS — "CloudTrail userIdentity element" (CloudTrail User Guide). https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-event-reference-user-identity.html
2. Google Cloud — "Cloud Audit Logs overview" (Cloud Logging docs). https://cloud.google.com/logging/docs/audit
3. Google Cloud — "Understanding audit logs" (Cloud Logging docs). https://cloud.google.com/logging/docs/audit/understanding-audit-logs
4. Kubernetes — "Auditing" (Kubernetes docs). https://kubernetes.io/docs/tasks/debug/debug-cluster/audit/
5. Kubernetes — "kube-apiserver Audit Configuration (v1)" (audit.k8s.io/v1 Event reference). https://kubernetes.io/docs/reference/config-api/apiserver-audit.v1/
6. Kubernetes — "User Impersonation". https://kubernetes.io/docs/reference/access-authn-authz/user-impersonation/
7. GitHub Docs — "About the audit log for your enterprise". https://docs.github.com/en/enterprise-cloud@latest/admin/monitoring-activity-in-your-enterprise/reviewing-audit-logs-for-your-enterprise/about-the-audit-log-for-your-enterprise
8. GitHub Docs — "Identifying audit log events performed by an access token". https://docs.github.com/en/enterprise-cloud@latest/admin/monitoring-activity-in-your-enterprise/reviewing-audit-logs-for-your-enterprise/identifying-audit-log-events-performed-by-an-access-token
9. OpenAI — "Tracing — OpenAI Agents SDK". https://openai.github.io/openai-agents-python/tracing/

## Verdict

**Established** (from primary sources):
- The dominant pattern is a **typed principal field** that enumerates human, machine (service account / role), and service identities — AWS `userIdentity.type` (`Root`/`IAMUser`/`AssumedRole`/`AWSService`/…), k8s username conventions (`system:serviceaccount:…`, `system:kube-proxy`), GitHub actor + token data. This is the concrete mechanism that answers "distinguish agent actions, humans, and automated processes in logs."
- **Delegation transparency** is standardized: audits record the immediate acting principal *and* the originating human where delegation exists (AWS `sourceIdentity`/`onBehalfOf`, k8s `user`+`impersonatedUser`, GCP service-agent attribution).
- System-initiated vs user-driven events are separated by log class (GCP's four audit log types) and by service-initiated fields (AWS `invokedBy`, GCP `callerIp: private`).
- At the agent-framework layer, attribution is **trace/span-based**: which named agent, which workflow, which conversation (`group_id`), with handoff spans linking agent-to-agent transitions (OpenAI Agents SDK).
- Audit entries are treated as immutable evidence (GCP) written through defined backends (k8s).

**Uncertain / not verifiable from the sources I reached**:
- A single cross-vendor "agent identity" standard for web audit trails: the proposed "Agent ID" specification I attempted (the `agent-id` GitHub org and `agent-id.github.io`) returned 404/empty; I did not locate a primary spec document and did not cite third-party lookalike repos. Existence/status of such a spec: **unknown — not verifiable from the sources I reached**.
- How LLM-native audit trails (prompt → tool → action) get rendered into host-platform audit logs beyond the GCP MCP example is not covered by any source I fetched; that mapping is **opinion**.
- Whether a given log's `userAgent` or token fields genuinely identify an LLM agent vs. a deterministic script is not specified by any source here; k8s explicitly flags `userAgent` as client-supplied and untrusted.

**What evidence would settle it**: (1) the published "Agent ID" web-identity spec or a ratified standard (W3C/IETF/OpenAI-Anthropic joint proposal) and its stated audit semantics; (2) a production audit trail from a real multi-agent deployment showing one agent's action traced end-to-end (agent span → tool call → CloudTrail/k8s audit entry → originating human); (3) vendor docs stating how MCP server invocations map to `serviceName` audit entries beyond Google Cloud.
