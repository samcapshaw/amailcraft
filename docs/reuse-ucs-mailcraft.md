# UCS-Care-Group MailCraft: update review

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
