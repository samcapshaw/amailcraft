# Notes workspace

Notes remain local to the MailCraft profile and use its existing folders, categories, search, undo
and storage. Old plain-text notes retain their format. Pin a note in its editor or context menu to
keep it at the top of its folder's normal views.

The Apple styled, PC and Omarchy desktop profiles use the same dedicated page workspace, including templates, live blocks, nested pages, page actions and navigation history. Changing appearance does not switch Notes to a reduced editor. The inherited classic host without the desktop theme chooser retains its older icon/list view. A notebook chooser
and searchable page sidebar sit beside the editor. Use New page to start writing or Template library to browse ten starter layouts with visual previews,
search and Planning, Work and Personal groups. Nested pages can be collapsed in the sidebar. Pages
support text, headings, checklists, quotes, callouts, code, editable tables and local PNG/JPEG images.
Add to page explains the available block types. The dots beside each block open options for changing
its type, inserting below, moving up/down and deleting. Use the vertical three-dot button at the end of a page row, or right-click the page, for rename, favourites, subpages, categories,
version history and deletion. Source-email links remain on the page. Creating a notebook uses the
existing folder dialog. Old notes are converted to blocks on their first workspace edit.

Edits save through the normal command journal. Adjacent autosaves on the same page group into a
single undo step. History retains up to 20 previous content revisions within a 2 MiB budget; restoring
a revision retains the current content as another revision. History covers content, not page location
or category metadata. A page supports 500 blocks and 1 MiB of projected text. Nesting is limited to
12 levels and cannot cross notebook boundaries. Tables support 50 rows and 12 columns.

Images require a saved profile and use its content-addressed blob store. Imports are limited to
20 per page, 5 MiB each and 4096 pixels per dimension. Images remain owned by the page after block
removal so earlier revisions remain recoverable. No remote images or embedded HTML execute.
The inherited classic interface retains its Markdown writing/preview editor.

Built-in templates include daily, weekly and monthly plans, a habit tracker, meeting notes, a project
brief, a decision log, a journal, reading notes and a trip planner. These are local, editable layouts;
the This week template has two columns with live Today/Tomorrow agendas, priorities, dated habits
and a full-width Due soon block. Agenda cards use the existing Calendar recurrence engine, include
appointments overlapping the day and open the original appointment. Due tasks shows incomplete dated
tasks (including overdue work) through the configured horizon. Checking one completes the actual task
through `tasks.markComplete`, including its normal recurrence and undo behavior. New task opens the
existing task editor. These blocks reflect local organizer data; they do not add another sync provider.
Older habit tables remain editable, while new habit templates use dedicated dated trackers.
A page can be saved as a custom template from the page’s right-click menu → Save as template. Custom templates
retain typed blocks, local image references, categories, icon, colour and paper; creating a page from
one starts with fresh history and no parent or source-email link. Save as template stops creating templates at 100 active
templates, which appear under Yours in the library. Removing a template is undoable; templates in
Deleted Items are hidden and cannot be reused.

Right-click a page and choose Page style to select an original project icon, an accent colour, and
plain, ruled or dotted paper.
These are lightweight page backgrounds, not a handwriting or pen engine. Existing content is never
replaced by choosing a template: every use creates a separate page.

Mail → More → Create note from email copies the sender, date and a quoted plain-text excerpt into
a Markdown note with a section for action items. HTML-only mail is converted to text. Preview-only
messages must finish downloading first. Excerpts are limited to 100,000 characters and explicitly
marked when truncated. Notes default to the source account's local Notes folder. Source email opens
the original local message; if it is deleted, the note's copied text remains intact.

Commands: `notes.new` accepts a built-in `template` key or a custom `templateId`, `markdown` and `pinned`;
`notes.update` accepts `markdown`, `pinned`, `paper` and an allowlisted `icon`; `notes.saveTemplate` accepts a page `id` and optional `name`; `notes.fromMail` accepts message `ids` and an optional
Notes `folder`; `notes.openSource` opens the linked email from a note `id`. All changes use the normal
command registry and undo/journal pipeline.

Additional workspace commands are `notes.reparent`, `notes.restoreRevision` and
`notes.attachImage`. `notes.update` accepts typed `blocks`; plain-text clients can continue editing
`text`, which flattens typed blocks while retaining previous content in history.

Notes are local. This does not add cloud synchronization, a canvas, collaboration, AI agents or
UberNotebook's entire editing surface. Rich inline formatting is still a gap. See [Notes roadmap](notes-roadmap.md) for the broader target.

## Columns, links and live blocks

Use **+ Two columns** to create a side-by-side section. Each column starts with an editable paragraph;
its block menu can insert any block below it. The block menu's Column controls move blocks between
lanes. Drag the dotted grip onto another block to place it before that block and adopt its column.
Move up/down remain available in the menu. At narrow widths the same columns stack in reading order;
the page data is unchanged. Layout sections must remain consecutive, with at most three columns.

**+ Add to page** also offers Calendar agenda, Due tasks, Habit tracker and Links & references.
Habit trackers navigate weeks independently and retain checks by absolute local calendar day; each
habit shows its weekly completion count. They support 30 routines and 3,660 checked dates per routine.
Creating a page from a custom template clears habit completions, leaving the original untouched.

The References submenu in any block's options connects a page, contact or date. The dedicated
Links & references block exposes the same chooser directly. Linked pages use IDs, survive renames
and appear under Linked from on the destination page. Deleted references remain visibly unavailable.
Dates open Calendar and contacts open their existing editor. This is an explicit reference picker,
not automatic parsing of typed @mentions or natural-language dates.

Inline #tags and explicit page categories appear as sidebar filters and are included in page search.
Page links and habit history are saved with blocks, participate in undo/revision restoration, and
survive profile serialization. Each page's serialized blocks are capped at 1 MiB; history includes
metadata in its 2 MiB budget. Live queries show at most 500 entries per block.

`notes.workspace {day,id?}` exposes agenda, due tasks and backlinks to command clients. `day` is an
integer local-calendar day since the Unix epoch. `notes.openReference {kind,id}` opens a page,
person, task or appointment through the command registry.


## Page navigation

The page header shows its notebook, parent pages and current title. Click an ancestor to open it;
the notebook menu lists root pages. Long paths collapse into a menu to keep the header compact.
Back and Forward revisit the last 100 page visits, including followed page links, and skip deleted
or unavailable destinations. History is local to the open window and does not change page content.

## Writing and presentation

Select text inside a writing block to reveal Bold, Italic, Highlight, Link and Clear formatting.
Formatting is saved as Unicode character ranges, so the editor displays the actual text rather than
Markdown punctuation. Bold/Italic/Highlight toggle for a selected range. Links accept HTTP, HTTPS or
mailto destinations; use the explicit Open link action outside editing to open one. Existing plain
text and Markdown notes remain readable. Inline formatting is preserved by autosave, undo, content
history and custom templates; plain-text projections do not encode those styles.

Type `/` at the start of a block, then a name such as `/heading`, `/table`, `/image` or `/habit`.
The filtered insert menu supports Up/Down, Enter and Escape, as well as clicking a choice. It converts
the current query block, retaining its column position. Images offer a file picker after insertion;
a saved profile is required for local image storage. The Add to page button remains available.

Block menus and drag handles become visible when hovering or editing, with a clear insertion line
while dragging. Move up/down and column options remain available in the block menu.

Right-click a page or use its trailing three-dot menu, then Page style to choose an adaptive,
reading, wide or full-page content width. Toggle Show colour cover for an original tonal cover that
follows the page accent. Daily, weekly and project starters have covers and tailored widths; custom
templates retain your own choices. Narrow windows constrain the chosen width to available space.
