# GitHub reuse shortlist — 2026-10-15

This is a source-review shortlist for faster feature development, not a dependency approval or a
claim of production compatibility. No third-party implementation, assets or dependencies were imported
into MailCraft. Selected files were downloaded into ignored `target/scratch/github-reuse-review`;
no third-party code was executed and no real accounts were connected.

Our current baseline already has Graph mail, read-only CalDAV snapshots and connection management,
selected connected edits, and a local increment for single timed Microsoft 365 meeting invitations.
Reuse should extend those boundaries: keep the command registry, mailbox model, durable pending-write
queue, credential store, original artwork and Apple styled/PC/Omarchy interfaces.

## Ranked candidates

| Candidate | Reviewed snapshot | Useful implementation | Fit and integration cost |
|---|---|---|---|
| [ZIDAN44/mailcraft](https://github.com/ZIDAN44/mailcraft) | `27e59b974c42664ec6a1eb3b9516fe5143361ccf` | `crates/net/src/dav.rs`: CalDAV/CardDAV discovery, ETag enumeration, batched multiget, conditional PUT; `graph_pim.rs`: calendar/contact conversion and recurrence mapping; `fake_graph.rs`: synthetic protocol server. `ews.rs` is a later on-premises Exchange candidate. | Closest application-level match: Rust and egui, but its PIM model and engine APIs differ substantially. Selective adaptation is more appropriate than a merge. Highest near-term value: incremental DAV and CardDAV. |
| [pimalaya/io-webdav](https://github.com/pimalaya/io-webdav) | Documentation review; pin a source commit before adoption | Protocol library documents sync tokens, collection sync, multiget, CalDAV/CardDAV and caller-controlled redirects. | Strong modular alternative for DAV work. Prototype its I/O-free layer behind our existing bounded HTTP transport before committing to a dependency. No build or compatibility test performed. |
| [pimalaya/io-msgraph](https://github.com/pimalaya/io-msgraph) | `2ae26370aefa671c16316dbad98107634436bce6` | Contact creation, batching, event creation and Graph↔iCalendar conversion, including recurrence and Windows/IANA zone mapping. Inspected `users.rs` exports calendar/event modules. | Useful for contact creation/deletion and recurring-calendar interoperability. The inspected README still says calendars are planned, while source exports them: assess the pinned implementation, not README coverage alone. Preserve our write queue and authorization checks. |
| [AdriaanBoshoff/OpenMailClient](https://github.com/AdriaanBoshoff/OpenMailClient) | `1bbe94a1ce739299c68668b73ad4b0db988c6a5c` | `crates/pst`: PST/OST reader, PST writers, structural checker and tests; `crates/engine/src/attachments.rs`: attachment handling. | Best specialized migration candidate. Its property-bag/store model differs from our mailbox model. Start with an isolated read-only PST import bridge; OST remains read-only, and writing existing files is a separate project. It is not a shortcut to delegated Graph accounts. |
| [folknor/ratatoskr](https://github.com/folknor/ratatoskr) | `5201476fc683d138cb3928ced0922f7e1d5c7842` | CalDAV client and synthetic shared-Graph/calendar fixtures; recovery-oriented service harness tests are present in its tree. | Useful for interoperability and failure scenarios. Iced UI plus asynchronous service/IPC architecture increases direct integration cost. Review selected fixtures and small protocol pieces rather than its host architecture. |
| [stalwartlabs/calcard](https://github.com/stalwartlabs/calcard) and [mail-parser](https://github.com/stalwartlabs/mail-parser) | Documentation/license review only | Calendaring/contact conversion, recurrence and time-zone handling; MIME/charset robustness and fuzzing resources. | Candidates for focused parser evaluation against our existing formats/MIME crates. Keep hostile-input caps and regression tests. Do not replace parsers merely because a README lists more formats. |

## Concrete review findings

- ZIDAN44 DAV PUT uses `If-Match` for updates and `If-None-Match: *` for creation. Its DELETE can
  proceed without an ETag: our adaptation must require a known version for destructive writes.
- Its Graph calendar mapping treats omitted `canEdit` as writable, and the inspected mutation loop
  issues unconditioned PATCH/DELETE requests. Those policies must not replace our explicit permissions,
  conditional writes, source binding or held uncertain outcomes.
- Ratatoskr's inspected CalDAV source similarly describes treating missing privileges as editable.
  Its credential-bearing client derives Debug; credential redaction needs review before reuse.
- OpenMailClient's PST crate depends on its own model and `miniz_oxide`; a whole-crate copy would bring
  a second data model. Its documented format coverage and structure-check tests are promising evidence
  for a prototype, not a substitute for testing our importer against bounded synthetic files.
- Pimalaya's event-creation source logs event data at trace level. Any integration should review logging
  and data exposure as well as response limits and dependency features.

## Licensing and provenance

The inspected ZIDAN44 and OpenMailClient MIT notices name the ArtCraft/MailCraft contributors and
OpenMailClient contributors respectively; both workspaces declare MIT OR Apache-2.0. Pimalaya's
libraries likewise offer MIT/Apache-2.0; Ratatoskr's inspected LICENSE is Apache-2.0. Stalwart's calcard
and mail-parser documentation declares Apache-2.0 OR MIT. These are candidates under our current
policy, subject to individual file provenance, bundled data/assets and transitive dependency review.

[SUNET/chithi](https://github.com/SUNET/chithi) has a GPL-3.0 LICENSE and
[Allodia's sync engine](https://github.com/allodia-eu/email-calendar-sync-engine) declares MPL-2.0.
They are excluded from code imports by our current AGENTS.md policy. Behavioral comparisons and
public specifications remain available for independent implementation.

Any adopted code must retain required notices and record the exact upstream commit, adapted paths,
changes and dependencies in attribution/notice records. A permissive top-level license alone does
not approve every dependency, screenshot, font or icon.

## Recommended execution order

1. Prototype incremental DAV sync and a CardDAV address book, comparing ZIDAN44's selected routines
   with Pimalaya's modular transport. Prove unchanged resources are skipped, conflicts preserve local
   data, and remote failures retain the last good snapshot using local synthetic servers.
2. Extend Graph contact creation/deletion and recurring-event/time-zone handling, using selected
   Pimalaya/ZIDAN44 mappings and fixtures where compatible. Keep server writes in our existing queue.
3. Build a read-only PST migration bridge from OpenMailClient's format work; require structure checks,
   bounded reads and original-file preservation before exposing it in Import.
4. Assess EWS only if on-premises Exchange is a concrete requirement; do not delay Microsoft 365
   shared/delegated reliability to introduce another provider architecture.

These priorities are engineering judgments from selected source inspection. No speedup estimate or
full dependency audit is implied. The fastest useful path is a small adapter plus regression fixtures,
keeping the interface we already designed.

## Additional review: UCS-Care-Group/mailcraft

The [updated assessment](reuse-ucs-mailcraft.md) moves UCS-Care-Group/mailcraft to the top of the
application-level shortlist for Microsoft 365 download reliability, agent permissions and advanced
meeting workflows. Its architecture is particularly close to ours. The DAV/CardDAV and PST candidates
above remain relevant to their respective feature areas.
