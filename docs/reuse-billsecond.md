# Reuse assessment: billsecond/mailcraft

Reviewed on 2026-10-14 at commit [`01abe0ad21f13c1a664acd45c6e0a6fa702ddfe3`](https://github.com/billsecond/mailcraft/tree/01abe0ad21f13c1a664acd45c6e0a6fa702ddfe3).
This is a source inspection, not an execution or interoperability audit. No implementation or assets were imported.

## Conclusion

There is substantial useful calendar and Microsoft-account work to adapt. This is not a drop-in upstream update: its workspace has different time, protocol, calendar-format, model, host-service, and sync interfaces. Its front end is `MailApp`; ours is `App`. Keep our desktop themes and connected-write safeguards, and bring over isolated behavior with its test cases.

## Priority candidates

| Priority | Candidate | Evidence in that repository | Integration into this fork |
|---|---|---|---|
| 1 | Graph calendar creation/deletion and organizer invitations, updates and cancellations | `crates/net/src/graph.rs`: `push_events`, event serialization and `transactionId`; `crates/model/src/remote.rs`: `EventOp`, `EventFields`, `EventOutcome` | Adapt serialization and operation cases into our net client and durable proposal queue. Preserve mailbox-bound requests, conditional writes and uncertain-submission review. New creation needs a stable transaction ID across retries. |
| 1 | Calendar protocol fixtures and regression cases | `crates/net/src/graph/standin.rs`, `standin_tests.rs`, `tests/meetings.rs` | Adapt the service-contract fixtures first. They cover creation, changing/deleting occurrences, non-default calendars, stale readback, and unsupported meeting options. Keep all tests synthetic. |
| 2 | Time zones and recurring series | `crates/time/src/zone.rs`, `zone/windows.rs`, `crates/model/src/zoned.rs`, `crates/ical/src/zones.rs` | Bridge their timestamp/zone types to ours. Review migrations, daylight-saving ambiguity and all-day semantics before enabling complete series editing. Their zone module uses bundled `jiff` rules; our dependency and attribution gates need to cover the addition. |
| 3 | Scheduling Assistant and meeting actions | `graph.rs`: `schedule`; `crates/engine/src/cmd/calendar/meeting.rs`, `invitation.rs`; `graph/tests/meetings.rs` | Add free/busy queries and invitation recognition behind our command registry, then build screens using our shared UI tokens. Forwarding, proposed times and online meetings each need explicit scope and failure behavior. |
| 4 | Administrator-consent recovery | `crates/net/src/oauth.rs`: refusal classification and admin-consent helpers | Useful error cases and explanatory flow. Adapt to our public-client registration and Graph-only authorization model; do not copy app IDs, mixed-resource assumptions, or organization approval claims. Their documentation marks this flow unverified against the real service. |
| 5 | HTML mail and image robustness cases | `crates/ui-egui/src/reading/images.rs`, HTML corpus and image/network tests | Compare individual failure cases with our existing sanitizer, rich-text renderer and remote-image policy. Port missing regressions rather than replace our rendering stack. |

## Keep or defer

- Our Graph mail transport, independent linked-mailbox grants, selected contact editing, ETag checks, and held/uncertain proposal flow already cover work this repository does differently or does not cover. Its Graph contacts API is read-only. A whole-client replacement would lose useful behavior.
- Its calendar write code carries change keys and verifies readback, but this inspection found no `If-Match` request handling in its crates. Keep our concurrency protection when adapting its broader operations.
- The latest IMAP UIDVALIDITY recovery change is worth comparing as a regression case. Our `sync/native.rs::forget_old_uids` already handles invalidating stale IDs and retaining moved items as uploads, so it is not automatically a missing feature.
- Server Favorites is a lower priority than working calendar operations. It adds configuration-item parsing and consent choices. Do not introduce its EWS path into our Graph-first account flow.
- Keep our Apple/Omarchy/PC layout profiles, branding, account UI, packaging identifiers, and release configuration.

## Provenance and validation

The workspace declares `MIT OR Apache-2.0`, matching ours, and includes both license texts and a NOTICE identifying material adapted from WordCraft. Preserve applicable notices for any adapted portions. Time-zone mappings identify Unicode CLDR data separately; review its attribution as well. Do not import artwork or fonts as part of code reuse. Check each selected file and dependency before importing it; this assessment is not a license audit of the entire tree.

The repository reports daily use on one Microsoft 365 mailbox, while its own gaps document still calls it early alpha and identifies unverified workflows. That is useful experience, not proof of complete shared/delegated support or production readiness. No real accounts were contacted during this review.

Recommended first implementation: adapt the calendar service-contract tests, then add connected appointment creation/deletion through our existing proposal queue. Follow with organizer invitations/cancellations, time-zone-aware series editing, and scheduling. Run our existing quality gates and retain attribution for every adapted slice.

Follow-up: the initial single-appointment creation/deletion increment was implemented using our existing queue and original synthetic fixtures against Microsoft's public API specification. No billsecond implementation or test code was copied for that increment. The broader calendar operations remain candidates for selective adaptation.
