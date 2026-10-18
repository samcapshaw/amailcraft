# Microsoft 365 server search

When a mail search is active, choose **Search Microsoft 365…** above the message list. The separate
search window lets you edit the query and choose the current folder, one Microsoft 365 mailbox,
or all connected Microsoft 365 mailboxes (up to 16). Press **Search server** to send the query.
The PC search ribbon’s Include Older Results tool opens the same explicit window.
Ordinary local search never sends it. This works in Apple styled, PC and Omarchy desktop profiles.

This search uses [Graph mail-search syntax](https://learn.microsoft.com/en-us/graph/search-query-parameter#use-search-on-message-collections),
including words, phrases, `from:`, `to:`, `subject:`, `body:`, `attachment:` and
`hasAttachments:true`. MailCraft's local search language has different filters and matching rules;
local-only filters are not translated or applied to the separate server results. Edit the server query
when needed. Search may require refreshing/signing in to an account first.

Each explicit request fetches one page per selected mailbox, normally 50 results. **More results**
fetches the next page. Graph limits each mailbox search to 1,000 results; MailCraft holds at most
1,000 additional results combined. Already-downloaded messages are omitted from this separate list,
so use local search to find them. No IMAP, CalDAV or contact servers are searched.

Results stay in the session until cancelled, replaced, or invalidated by a local query/account or current-folder change.
Closing cancels further requests; a request already on the network finishes under its timeout, and
its result is discarded. A new request waits for the previous worker to finish. Queries and results
are not added to the mailbox or application logs. Account address, endpoint and credential-owner
bindings are rechecked before results are accepted or opened. Continuations cannot leave the
searched mailbox or folder.

**Open** saves the selected preview in its actual local folder, selects it in the normal reader and
requests full MIME through the existing download worker. With a persistent store, the command journal
must accept the preview before its server baseline is saved or its body download starts. Storage failures
defer that step until the pending journal is successfully flushed. The mailbox must have been synchronized
enough to discover that folder. Existing local copies are reused. Opening a historical search hit does
not invoke new-mail arrival rules, and does not advance delta-download cursors. Content download
may be deferred when the account is already busy; use the existing download controls to retry.

This is an initial desktop implementation, verified with synthetic fixtures. Live Microsoft 365 tenant
and delegated-permission compatibility have not been verified. The browser host cannot search a
mail server directly. No upstream implementation or visual assets were imported.
