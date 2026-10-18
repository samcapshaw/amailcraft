# Notes target and verified scope

UberNotebook is the product reference, not a claim of feature parity. MailCraft's implementation is
independent Rust/egui code. The weekly page is the first end-to-end benchmark.

| Capability | Current state |
| --- | --- |
| Notebook and nested-page navigation, search, favourites | Implemented |
| Typed text, headings, checklists, quotes, callouts, code, tables and local images | Implemented |
| Side-by-side columns and movable blocks | Implemented; two/three lanes, drag grip and menu movement; narrow windows stack |
| Today/Tomorrow calendar agendas | Implemented against local Calendar, including recurrence |
| Due tasks embedded on a page | Implemented against actual Tasks, with completion and undo |
| Dated habits and weekly progress | Implemented; persistent absolute dates and week navigation |
| Page links and backlinks | Implemented with stable IDs and destination rename support |
| Tags, people and date references | Implemented sidebar tag filters and explicit reference pickers; typed @mention completion remains a gap |
| Starter and reusable custom templates | Implemented; ten starters, original previews, styles and paper backgrounds |
| Autosave, undo and bounded content history | Implemented |
| Source-email capture | Implemented |
| Rich inline formatting and slash-command completion | Implemented for writing blocks: bold, italic, highlight, links and filtered keyboard/click insertion |
| Adjustable page widths and optional colour covers | Implemented across desktop profiles; starter and custom template presentation |
| Arbitrary nested block groups and resizable column ratios | Not yet implemented |
| Boards, mind maps, sketches, equations | Not yet implemented |
| Handwriting, pens, notebook shelf and page-turning | Not yet implemented |
| PDF/file/bookmark embeds and full attachment library | Only local PNG/JPEG image blocks implemented |
| People/date natural-language mentions and task extraction | Explicit references only |
| Dictation, audio recording/transcription and AI workspace | Not yet implemented |
| Notes-specific import/export and Markdown folder mirroring | Not yet implemented |
| Collaboration or Notes cloud synchronization | Not implemented; notes remain local to the profile |

Next milestones should improve the editing surface (rich text, slash commands and nested blocks),
then attachment/interchange workflows, before adding separate canvas and recording tools. Existing
mail, Calendar, Contacts and Tasks capabilities do not imply the corresponding Notes integration
is complete.
