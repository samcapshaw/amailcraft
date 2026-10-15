# Omamail reuse assessment

Reviewed 2026-10-14: [huacnlee/omamail](https://github.com/huacnlee/omamail), commit `5db17740f99feb1252ba136db20d9fb67899cd91`.
Source inspected in `target/scratch/omamail-review`; no code or assets imported, and no accounts connected or external code executed.

## Fit and licensing

The repository LICENSE is MIT, copyright 2026 Jason Lee. Selected code is a reuse candidate subject to file provenance and dependency/license review. Any imported substantial code must retain its copyright and permission notice, record its source commit and adaptations, and update attribution. The top-level license does not replace dependency or asset review.

Its Rust backend is relevant; Qt/QML and JavaScript UI components do not fit our egui front end directly. Keep our command registry, mailbox model, persistence, error types, original artwork, and Apple styled/PC/Omarchy interface profiles.

## Best candidates

| Priority | Area and inspected source | Potential benefit and integration boundary |
|---|---|---|
| 1 | Active Omarchy palette: `ui/Service.qml`, `ui/settings/Appearance.js` | Follow the installed Omarchy palette rather than only a fixed Tokyo Night preset. Implement an optional read-only colors.toml importer in our theme system, with validation, fallback and explicit user preference; do not adopt the QML host. |
| 2 | Calendar discovery and transport: `src/calendar/discovery.rs`, `src/calendar/mod.rs`, `src/calendar/attendance.rs` | Candidates for CalDAV/iCloud and Google calendar expansion beyond our current connected Graph work. The HTTP layer bounds responses, rejects redirects and accepts conditional writes. Adapt transport and test cases into our net/engine boundaries; retain source/account binding and held uncertain writes. |
| 3 | Gmail API: `src/providers/gmail*.rs` | Provider-specific labels, API operations and throttling could accelerate a native Gmail transport. This is a separate project from existing IMAP support. Its mutation queue is explicitly in-memory, so it cannot replace our durable pending-change machinery. |
| 4 | Delayed sending: `src/outbox/mod.rs`, `src/outbox/storage.rs`, `src/outbox/tests.rs` | Useful recovery semantics and test ideas: interrupted delivery is unknown, and restart does not silently resume a send. Compare with our existing outbox and delayed-delivery commands before adding another queue. |
| 5 | Desktop integration and keyboard behavior | Omarchy bar/mailto integration, notifications and shortcut discoverability are useful behavioral references. Plugin host support differs from standalone support; do not claim these features transfer automatically. |

## Lower priority

HEY CLI integration is useful only if HEY becomes a requested provider. Agent-assisted drafting adds process management and privacy decisions and is not a shortcut to shared/delegated mailbox support. We already have OS credential storage, MIME, IMAP, JMAP, unified folders and compose tools; replacing those wholesale would likely cost more than targeted improvements.

## Recommended next increment

An opt-in **Follow Omarchy theme** setting is the smallest useful adaptation. Then assess CalDAV discovery and synthetic fixtures as a separate feature increment. The first increment is now implemented independently from Omarchy’s public palette format: an opt-in read-only theme follower with bounded parsing, periodic refresh and Tokyo Night fallback. No Omamail code was imported; the other candidates remain assessment only.
