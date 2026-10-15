# Lookout reference assessment

Reviewed 2026-10-14 at [`ef26df2b79b7cbe193ff439f5b2f85b286972a56`](https://github.com/gavindi/lookout/tree/ef26df2b79b7cbe193ff439f5b2f85b286972a56).
Review scope: README, feature/parity documentation, changelog, workspace manifest, and license. No implementation was imported, and no runtime or protocol claims were verified.

## Reuse boundary

Lookout's workspace declares `GPL-3.0-or-later`. Our AGENTS.md says never to copy GPL/LGPL/AGPL/MPL code. Treat Lookout as a product and interoperability reference, not a source-code donor. Do not copy its implementation, tests, fixtures, or artwork. Any independent feature work should be implemented from public specifications with original synthetic test data. Do not treat an independent dependency as GPL merely because Lookout uses it; review the dependency's own license and source separately if needed.

Its GTK4/libadwaita/WebKitGTK and GNOME Online Accounts integration also differs substantially from our egui desktop and engine. Adopting the UI would undermine our established Apple and Omarchy profiles rather than accelerate them.

## Useful product directions

| Candidate | Value for MailCraft | Independent implementation direction |
|---|---|---|
| CalDAV calendars and CardDAV contacts | Broadens connected organizer support beyond Microsoft 365 | Public CalDAV/CardDAV specifications, our existing iCalendar/vCard support, bounded DAV responses and conditional writes; original local fixtures |
| Server-side rules | Rules can operate while MailCraft is closed | Separate Graph message-rule and ManageSieve backends, explicit account scope and permission; preserve local-only rules where a server cannot express a condition |
| Invitation cards inside mail | Accept/Tentative/Decline and review proposed times without hunting through Calendar | Original UI over our command registry and Graph response queue; iMIP/iTIP for standards-based accounts |
| Connected task lists | Makes our Tasks module useful across devices | Opt-in Microsoft To Do or CalDAV VTODO support, with clearly separate local and connected sources |
| Snooze and sync-state clarity | Useful daily mail workflow | Original persistent wake scheduling with explicit local/server semantics, plus accurate pending/error indicators |

The README documents CalDAV/CardDAV, Google Tasks, and mail workflows. The parity document also describes Graph categories and server rules. These are documentation claims, not an audit of their implementation. Its shared/delegated mailbox row is still marked missing, so it is not evidence of a shortcut for our central delegated-account goal. An endpoint table is not proof that a feature is implemented.

## Recommended order

Continue adapting the permissively licensed billsecond/mailcraft calendar operations and service-contract tests first. Use Lookout to inform our later independent DAV, server-rule, invitation and task roadmap. Keep the current Mac/Omarchy interface and our account-bound authorization, concurrency checks, and uncertain-write handling.
