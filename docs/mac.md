# MailCraft for Mac

This fork keeps MailCraft's Rust mail engine and introduces a Mac-oriented desktop shell.
The desktop app opens the new shell by default. The classic shell remains the default for the CLI's
headless UI tests and web front end; `App::enable_mac_style()` opts other hosts into the Mac shell.

## Try it

Run `./scripts/run-mac.sh --sample` from the repository. The script uses a workspace-local Rust toolchain
when present, otherwise the installed Cargo. The demo is in memory: Jordan's mailbox, Design team (shared),
and Alex Morgan (delegated) contain only synthetic example.net mail. They cannot connect or send.
Run without `--sample` to open a persistent profile in `~/Library/Application Support/MailCraft Mac`,
or use `--profile PATH` for a separate one. The Mac derivative has its own bundle ID and default profile.

All Inboxes combines Inbox folders from personal, shared, and delegated accounts, with a combined unread
count. Individual inboxes remain separate; selecting a message keeps the aggregate view open. Replies
and archive/delete operations retain the original mailbox identity. Search stays within the inboxes.

The mail interface follows a charcoal-and-blue Mac design: a unified title bar with native window controls,
a toolbar search field, larger message typography, separate previews and mailbox source lines, and
Favorites/Mailboxes/Smart folders in the sidebar. The normal desktop layout keeps the mailbox sidebar
at the far left, the message list next, and the reading pane on the right. The toolbar’s mailbox button
hides or restores the sidebar, keeping its width. More → Reading layout can
put the reader below the list or hide it; pane dividers remain draggable. Short windows compact and
collapse mailbox groups. All folders remain available from each mailbox’s More folders menu.

The interface keeps a compact mail toolbar,
blue unread dots, neutral message typography, a sidebar that puts shared/delegated mailboxes together,
and light/dark appearances. It reads the installed macOS system font at runtime; Apple fonts are never
bundled or redistributed. Compose windows always display From, expose Send and Attach directly, and
use a slim Formatting row for font, size, emphasis, paragraphs, and inserts.
A visible signature picker switches or removes saved signatures and opens their editor. Empty Cc fields
stay tucked away behind Cc/Bcc. The complete inherited toolset is accessible through More → All tools
as menus, rather than expanding a ribbon into the writing area. Compose windows on macOS use a unified
traffic-light title bar with a draggable toolbar. Existing engine commands, offline storage,
rules, search, draft auto-save, keyboard shortcuts, attachments, and import/export remain in use.

Calendar, Contacts, Tasks, and Notes are always available from the compact bottom navigation bar.
The toolbar follows the selected section: Calendar exposes New event and New meeting; other sections
expose their own creation actions. More → All tools provides the existing module commands without
expanding a ribbon. Calendar retains its day/week/month/schedule views, editors, reminders, and
iCalendar import/subscriptions. Graph accounts additionally offer opt-in read-only calendar and default-contact snapshots; see [connected services](microsoft365.md#connected-calendars-and-contacts).

Settings use an icon category sidebar, raised section cards, inset fields, fine dividers,
Mac-style switches, and a Light/Dark/Auto appearance selector. Labels and controls share consistent alignment. Save Changes
applies pending edits; Cancel discards them. Accounts use a separate mailbox list and detail pane;
legacy data-file and advanced account options remain accessible from that pane. The app icon is original
vector artwork in `assets/app-icon/mailcraft.svg`; `cargo xtask ico` regenerates all desktop formats.

## Shared and delegated Microsoft 365 mailboxes

Microsoft 365 Graph mail is now available through the account wizard; see [Graph setup and limits](microsoft365.md). The instructions below describe the separate OAuth IMAP/SMTP path. EWS is not implemented. Microsoft documents
shared-mailbox XOAUTH2 using a delegate's authorization and the target mailbox address as the SASL user:
https://learn.microsoft.com/en-us/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth/

1. Add your personal Microsoft 365 IMAP/SMTP account in More → Accounts. Configure OAuth with your own
   public-client app registration and sign in. See [account setup](accounts.md) and
   [OAuth registration](../crates/net/data/README.md).
2. Choose More → Add shared or delegated mailbox, or Shared mailbox in Account Settings.
3. Select the personal OAuth account and enter the target mailbox's address and display name.
   Select shared or delegated to label the mailbox. Receive-only is the default; Send as is opt-in.
4. In the account window that opens, sign in **as the delegate**, using the inherited OAuth registration.
   The new mailbox gets its own OAuth grant and refresh token; no credentials are copied. Incoming and
   outgoing XOAUTH2 usernames use the target mailbox's address.
5. Get new mail. The mail server authorizes access and Send As. This client never grants permissions.

Your organization must allow the applicable IMAP/SMTP protocols, OAuth scopes, and mailbox permissions.
Adding the mailbox preserves the default sender. Reply and new-message identities follow the mailbox;
a receive-only shared From identity cannot fall back to your personal SMTP transport.

Implemented and tested locally: mailbox creation/validation, independent authorization configuration,
correct SASL usernames, duplicate prevention, receive-only send gating, account labels, and command routing.
**No real Microsoft tenant has been connected or tested.** Permission discovery, auto-mapping, permission
administration, configurable Send on Behalf, and shared Exchange
calendars, and signed/notarized distribution are not implemented. Shared/delegated labels describe how
mailboxes are opened; they are not proof of server-granted permissions.

## Development checks

- `cargo test -p mailcraft-engine -p mailcraft-ui-egui --lib`
- `cargo clippy -p mailcraft -p mailcraft-ui-egui -p mailcraft-engine --all-targets -- -D warnings`
- `cargo run -p mailcraft-ui-egui --example ui_shot -- target/scratch/mac-light.png 1440x900 --mac --select`
- Add `--dark` for the dark appearance; render at `400x700` for the narrow layout.

Use `TMPDIR=$PWD/target/tmp` and keep build concurrency bounded, as specified in AGENTS.md.
The fork lives at [samcapshaw/amailcraft](https://github.com/samcapshaw/amailcraft), based on
[MonkeySaint/mailcraft](https://github.com/MonkeySaint/mailcraft). Run
`./scripts/screenshots-mac.sh` to regenerate the current README screenshots from synthetic demo mail.

Settings and supporting windows share the Mac surface palette, rounded form controls,
section spacing and window shadows. Account cards use the same treatment as Settings;
calendar, contact and task editors use the same form controls and blue save actions.

## Appearance profiles

Settings → General → Make it yours offers Apple, Outlook layout, and Omarchy/Tokyo Night.
Apple remains the default for the desktop app. Outlook restores the inherited ribbon layout;
Omarchy keeps the compact layout with navy, blue and lavender colors. All profiles support
Light, Dark and Auto. These settings apply to mail, settings, compose and organizer windows.

Custom accent/background/surface/text colors are edited with color pickers or `#RRGGBB` fields.
The preview stays inside Settings until Save Changes; Cancel discards pending edits.
Invalid hex values are refused before applying any of the pending edits. Custom colors form
a fixed palette in both appearances until customization is turned off.

Profiles use the existing validated settings store and persist across launches.
`mailcraft --interface apple|outlook|omarchy` explicitly selects and saves a profile;
without the flag the saved selection is used. On Linux, Omarchy styling is confined to this app.
Microsoft 365 Graph mail and All Sent are implemented separately from appearance profiles.
Calendar/contact creation/deletion and broader editing remain backend work; see [Graph setup and limits](microsoft365.md).

Connected Microsoft 365 calendars and default contacts can now be enabled in Graph account settings. They use read-only offline snapshots, with an explicit source strip and refresh action. See [connected-service setup and limits](microsoft365.md#connected-calendars-and-contacts).

Connected organizer items also support optional, server-confirmed edits and attendee responses through the dedicated Microsoft 365 sheets. Generic cached-item editors stay read-only. Enable the separate editing grant and refresh; see [editing setup and limits](microsoft365.md#editing-and-meeting-responses).
