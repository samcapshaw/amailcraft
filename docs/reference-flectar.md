# Flectar Mail reference assessment

Reviewed 2026-10-14 at [`ddf2ce63b34ade5e975b669f8a84e2a10ef6cbf2`](https://github.com/flectar/mail/tree/ddf2ce63b34ade5e975b669f8a84e2a10ef6cbf2).
Scope: README, licensing and third-party notices, package manifests. No application implementation, tests, fixtures or artwork were imported, and no performance or interoperability claims were tested.

## Code reuse

Flectar-authored application code defaults to `AGPL-3.0-only`; its separate `flectar-mail-core` crate also declares that license. Our AGENTS.md prohibits copying AGPL code, so separating its core from its UI does not make that core an eligible donor under our current rules.

The repository identifies separately licensed third-party material, including Apache-licensed Blitz and Parley components. Those must be assessed independently; their inclusion in an AGPL application does not by itself make the upstream libraries AGPL. If our HTML renderer needs a larger compatibility upgrade, evaluate the original upstream renderer, its applicable licenses and dependencies, and the migration cost. Do not assume all Flectar-authored modifications have the same terms as upstream. The app itself uses Slint rather than our egui UI, so a direct front-end import is also a poor architectural fit.

## Useful product ideas

| Candidate | Relevance to our build |
|---|---|
| Work/Personal account profiles with subtle account colors | Useful in our unified inbox, especially when personal, shared and delegated mail are mixed. Keep account identity explicit, and do not use color as the only cue. |
| Independent message appearance and original-color toggle | Supports readable HTML in a dark app without forcing a whole-app theme change. Fits our Apple and Omarchy profiles. |
| Searchable attachment/file view | Extends attachments beyond individual messages; consider original per-account indexing before adding JMAP/WebDAV file storage. |
| Storage usage and verified backups | Useful maintenance UI for a local-first client. Back up account configuration separately from keyring credentials, and verify restores with synthetic profiles. |
| Persistent workspace tabs | Useful for comparing conversations, saved searches and Calendar, but optional so our compact default stays simple. |
| OpenPGP/MIME | A later security feature with substantial key-management and failure-handling work, not a quick code adaptation. Flectar documents desktop GnuPG integration, not S/MIME support. |

Their README also documents CalDAV/CardDAV and experimental native HTML rendering. These are product claims, not validation of their behavior. The advertised RAM figure is not a comparable benchmark of our app. This review did not establish documented shared/delegated mailbox parity or a reusable shortcut for our Microsoft 365 authorization model.

## Recommendation

Keep billsecond/mailcraft as the immediate permissively licensed source candidate for connected calendar operations. Use Flectar's documented account profiles and message-appearance controls as references for independently implemented UX improvements. Evaluate upstream Blitz separately only if renderer compatibility becomes a priority; replacing our rendering stack now would add migration and regression work.
