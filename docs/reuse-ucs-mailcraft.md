# UCS-Care-Group MailCraft: update review

## Reliability follow-up: 2026-10-17 (later snapshot)

Reviewed the 38 commits after the previous snapshot through
[`ddf626819e54cb35c41f711ad6936567f7ba64eb`](https://github.com/UCS-Care-Group/mailcraft/tree/ddf626819e54cb35c41f711ad6936567f7ba64eb).
The comparison JSON is in ignored `target/scratch/ucs-latest-compare.json`.

Our independently implemented follow-up preserves omitted Graph delta keyword fields (read,
draft and flag), including successive events during one download round. Full-message fetch batches
continue past HTTP 400/404/410/422 and oversized individual responses, report the failures without
including message identifiers or server response bodies, and stop after three consecutive individual
failures. Network, authentication, throttling and other failures still stop immediately. Successful
content is emitted normally; failed items remain incomplete for a later explicit retry. Existing batch
size and cancellation limits remain in force. Regression tests cover sparse updates, mixed successes,
consecutive failures and account-wide refusals. No upstream code or assets were copied.

The delta `$expand` removal does not apply: our requests already omit that expansion. Shared-mailbox
sending/drafts and Gmail archive/delete changes remain candidates for separate work.

## Follow-up review: 2026-10-17

Pinned snapshot: [`a030a7209e3b8543aa4679e34c502798452ea88d`](https://github.com/UCS-Care-Group/mailcraft/tree/a030a7209e3b8543aa4679e34c502798452ea88d).
GitHub's comparison against the October 15 snapshot reports 136 commits (including merges) and
188 changed files. Source was inspected locally without building or executing it. The downloaded
snapshot and comparison are in ignored `target/scratch/ucs-review-2026-10-17`. MIT OR Apache-2.0
licensing and the MailCraft contributors' MIT notice remain present.

Our current baseline is `568cf96`: single timed meeting invitations and resumable Graph downloads
are already implemented. The earlier recommendation below is historical; do not redo the download
checkpoint work.

### Most useful remaining adaptations

| Priority | Inspected code | Benefit and integration boundary |
|---|---|---|
| 1 | `crates/net/src/graph/pace.rs`, `graph/mail/fetch.rs` | A per-mailbox request gate reserves capacity for foreground work, pauses requests after throttling, and distinguishes repeatable reads from operations that could create duplicates. Our transport currently reports throttling without an equivalent coordinated gate. Adapt the gate and read retry policy first; retain our held uncertain writes and conditional updates. |
| 2 | `crates/engine/src/cmd/shared.rs`, `crates/ui-egui/src/shared_mailboxes.rs`, `apps/mailcraft-cli/tests/graph_shared.rs` | Opens a shared mailbox through its owner's Microsoft sign-in with shared-mail permissions; addresses the target mailbox explicitly and tests that its work never reaches the owner's mailbox. This could reduce repeated sign-ins in our linked-account setup. It changes our credential/account model and needs careful owner binding, consent handling, account-removal behavior, and synthetic isolation tests. |
| 3 | `crates/net/src/graph/mail/fetch.rs`, `crates/model/src/sync/`, `crates/engine/src/store.rs` | Foreground/background full-message fetching and disk-resident body/attachment data with bounded memory. Our page checkpoint path still fetches new-message MIME within each page. Separating previews from full content could improve first-run speed and large-mailbox memory use. Search, reply, rules, export, undo and missing-content recovery must keep working. |
| 4 | `crates/engine/src/store.rs` (`Saver`), store tests and desktop save integration | Background saves with progress and explicit close/open coordination could keep the UI responsive after large downloads. We already have a journal and external raw-message blobs in a separate store crate; adapt the worker/lifecycle design, not their file format or entire store. Preserve our rule that download progress cannot lead durable mail data. |
| 5 | `crates/net/src/graph/mail/drafts.rs`, `crates/engine/src/cmd/graph_drafts.rs` | Server drafts, reply/forward creation, attachment reconciliation and large-attachment upload sessions extend our local-draft/direct-send implementation. It is a substantial feature, not a drop-in helper. Preserve reviewed uncertain outcomes, current sender permissions and source binding. |
| 6 | `crates/engine/src/cmd/sync_progress.rs`, `crates/model/src/sync/progress.rs` | Structured progress per account, phases, counts, bounded problem lists, and full-message progress; waiting accounts can be skipped. Useful for a compact progress sheet styled for our profiles, rather than importing their UI host. |

The shared-mailbox implementation explicitly remains behind `MAILCRAFT_SHARED_MAILBOXES=1`.
Its current slice downloads mail and integrates search/counts/favorites; shared writes, Send As/on
behalf, and shared calendars are still absent. It must not be presented as a finished solution to our
shared/delegated-account goal.

Draft code uses a persistent marker to look for uncertain creates, and Graph reply/forward actions to
retain the server conversation where possible. Upload-session URLs and lost-response reconciliation
need independent security/concurrency review before adaptation. The candidate's own gap document
says its newer Graph writes have not met the real service; this review did not run their tests or verify
a live tenant. README feature claims and gap documentation sometimes differ, so implementation and
fixture presence are the basis for this shortlist.

Recommended sequence: coordinated throttling and foreground priority, owner-linked shared mailbox
authentication, then preview/body separation with structured progress. Treat background persistence
and server drafts as separate bounded projects. Keep our Apple styled, PC and Omarchy design.

No source or assets have been imported by this review. Any later selective adaptation should pin its
source, preserve the applicable license/copyright notices, and land with our own regression tests.

## Original review: 2026-10-15

Reviewed 2026-10-15 at [`d78cae3996a9f35a8abb9095aef05d895bb83220`](https://github.com/UCS-Care-Group/mailcraft/tree/d78cae3996a9f35a8abb9095aef05d895bb83220).
The MIT notice names the MailCraft contributors; the repository offers MIT OR Apache-2.0.
Selected sources, tests, manifests and documentation were inspected in ignored
`target/scratch/github-reuse-review/UCS-Care-Group/mailcraft`. No code/assets were imported,
no external implementation was executed, and no live accounts were used.

## Why this changes the shortlist

Its layered Rust crates, egui front end and command-registry approach are particularly close to our
architecture. Its model uses different identifiers and calendar/upload types, so adoption still needs
adapters. It is now the primary application-level reference for Microsoft 365 reliability and meeting
workflows. ZIDAN44/Pimalaya remain relevant to DAV/CardDAV; OpenMailClient remains the archive-import
candidate. Keep our Apple styled, PC and Omarchy interfaces, credential store and durable write queue.

## Recent changes with the clearest value

| Priority | Update and inspected source | Benefit beyond our current implementation |
|---|---|---|
| 1 | [Resumable Graph downloads](https://github.com/UCS-Care-Group/mailcraft/commit/acd586ba43ea6d6c09b64abc7020f498f6ec0470), `crates/net/src/graph/mail/down.rs` | Persist each page with its continuation link, get the newest page of every folder first, then fill history. This could improve first-run usability and recovery on large mailboxes. We already have delta sync, expired-cursor recovery and cancellation; the opportunity is finer-grained progress/resume behavior. |
| 2 | [Full-message fetching](https://github.com/UCS-Care-Group/mailcraft/commit/342f2de0874982a7f88a02a15cad8bddbb123db6), `graph/mail/fetch.rs` | Separate lightweight folder-list sync from full message content fetching, with foreground demand and background policy. Assess against our existing MIME fetch path before porting. |
| 3 | `crates/net/src/graph/pace.rs` | Mailbox-wide throttling, Retry-After pauses and foreground/background request priority. Retain our treatment of uncertain writes; do not transplant automatic mutation retry policy wholesale. |
| 4 | [Agent command permissions](https://github.com/UCS-Care-Group/mailcraft/commit/9447cb912b461610d6707e0c0b66ab8faf4c0042), `engine/src/agent.rs`, `mcp/src/files.rs`, `ui-egui/src/agent_gate.rs` | Explicit command classification, default refusal for unclassified commands, checks for parameter-dependent sending/deletion, owner-controlled approvals, and a bounded file-access directory. Our MCP currently dispatches through the registry without an equivalent complete permission layer. These changes would strengthen agent-facing use. |
| 5 | [Build/security hardening](https://github.com/UCS-Care-Group/mailcraft/commit/d83b351ef0f31232bbd8049a9d0a066451743d25), `deny.toml` | Dependency/advisory checking, pinned workflow actions and documented threat boundaries. Review and adapt to our license policy and existing CI. Their commit also updates rustls; our lockfile currently selects 0.23.45. Verify actual advisories and affected configurations independently before making vulnerability claims or updating dependencies. |

## Existing calendar capabilities worth adapting next

The inspected `crates/net/src/graph.rs` contains meeting cancellation through the cancel action,
occurrence resolution, recurrence/time-zone handling, free/busy scheduling and online-meeting fields.
`graph/tests/meetings.rs` includes synthetic tests for online-meeting creation/readback, invalid join
links, response options, forwarding and proposing another time. `docs/calendar-parity.md` documents
the corresponding UI behavior and remaining gaps.

These are beyond our newly implemented single timed meeting invitations. The most useful sequence is:

1. Organizer-only meeting cancellation, with current source/version review and held uncertain outcomes.
2. A free/busy scheduling assistant for required and optional attendees.
3. Online-meeting creation and safe join-link display.
4. Occurrence-versus-series editing and richer time-zone controls.

The source also has behaviors that need deliberate adaptation. Its Graph upload path can retry with
unsupported options removed, while ours holds uncertain mutations for review. Any fallback must be
limited to a definite refusal, preserve transaction identity where applicable, and explain changed
meeting options. The inspected event-write path does not establish our existing If-Match policy:
keep our conditional edits and preflight checks rather than assuming a larger feature surface gives
the same concurrency guarantees. A transaction ID is not a guarantee of exactly-once invitation delivery.

The README reports use against one Microsoft 365 mailbox; this review did not reproduce that validation.
Feature and test presence demonstrate candidate implementation coverage, not proven compatibility with
our app, every tenant, shared calendars or delegated permissions.

## Recommendation

Prioritize the resumable Graph read/download path and agent-facing permission boundaries, then adapt
meeting cancellation and scheduling into our existing command and pending-write architecture. Use
selected source and synthetic scenarios with retained notices, pinned provenance and dependency checks.
No visual assets, fonts or whole UI host need to be imported.
