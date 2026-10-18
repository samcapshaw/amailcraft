# UberNotebook review

Reviewed [samcapshaw/ubernotebook at 1dff5a6](https://github.com/samcapshaw/ubernotebook/tree/1dff5a6037dcbbe5eda270d4f1c8facec2fd5287).
The project is MIT licensed, copyright 2026 marcho78. Its README, licence, Workspace.js and
Markdown.js were inspected; its code was not built or executed.

Its block/page structure, bounded validation, templates, Markdown interchange and email/contact
links are useful references. Its QML/Quickshell UI and JavaScript model do not plug into our Rust/egui
architecture. Its README explicitly lists cross-computer synchronization as unavailable.

MailCraft independently implements a notebook/page workspace, typed blocks, local images, bounded
revision history, columns and movable blocks, live Calendar/Tasks cards, dated habits, page links
and backlinks, tags and explicit person/date references, Markdown presentation, templates, favourites and source-email capture through its
existing Rust model, commands and journal.
No UberNotebook code, assets or fonts were copied. Any future source adaptation must retain the
applicable MIT copyright and permission notice and separately review bundled third-party licences.

See [Notes workspace](notes.md) for implemented behavior and remaining limitations.

The 2026-10-17 design review also inspected the current marcho78/ubernotebook README and the
Pages and Templates screenshots. The visual template library, original page icons, reusable custom
templates and paper backgrounds in MailCraft are independently authored; no screenshot, template
text, font or source implementation was imported. Paper backgrounds do not add UberNotebook's
notebook shelf, handwriting, pen tools or page-turn animation.
