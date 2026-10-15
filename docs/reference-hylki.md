# Hylki reference assessment

Reviewed 2026-10-14 at [`c47878a17bc72a3c72083de37cede621a272495b`](https://github.com/hyprlab/hylki/tree/c47878a17bc72a3c72083de37cede621a272495b).
Scope: README, package manifest, feature and user documentation. No implementation, tests, fixtures or artwork imported; no runtime or interoperability validation.

Hylki declares `AGPL-3.0-or-later`. Our project's prohibition on copying AGPL code means this is a product reference, not a direct implementation donor. Its GNOME/libadwaita interface is also different from our egui shell. Independently implemented features should use public specifications and original test data.

## Useful directions

- **Compose refinement:** quote folding, inline-image controls and named per-account HTML signatures offer interaction ideas. We already have signatures and compose functionality, so compare specific missing behavior before adding duplicate machinery.
- **Simpler Send later:** Hylki documents editable scheduled Outbox messages. Our compose engine already supports deferred delivery; a compact scheduling picker and clear queued status could expose existing power with comparatively little backend work. Preserve our uncertain-submission hold rather than copying automatic-retry semantics.
- **Cloud attachment links:** optional upload to user-selected storage, with explicit sharing/expiry choices where supported. Potentially useful for our current Graph direct-send attachment size limit, but requires new storage authorization and clear sharing behavior. This is not a replacement for native Graph large-attachment uploads.
- **Reader View and per-message appearance:** simplified newsletter reading, with original content available. This reinforces the message-appearance ideas identified in Flectar.
- **Conversation completeness:** Hylki documents showing Sent and archived replies alongside the current folder's messages. Compare our existing conversation behavior before implementing anything new.
- **Sync UX:** quick first-page results, background indexing and source-aware state are useful acceptance criteria, not a reason to replace our network/storage stack.

Its docs did not establish a shortcut for our connected calendar creation, shared/delegated Graph authorization, or organizer editing priorities. Feature descriptions are claims, not independent verification. Keep billsecond/mailcraft as the immediate permissively licensed calendar-code candidate. Hylki's most actionable near-term inspiration is a polished Send later interface over our existing deferred-delivery engine.
