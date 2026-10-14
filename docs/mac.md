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

The interface has a unified macOS title bar with native window controls, a compact mail toolbar,
blue unread dots, neutral message typography, a sidebar that puts shared/delegated mailboxes together,
and light/dark appearances. It reads the installed macOS system font at runtime; Apple fonts are never
bundled or redistributed. Compose windows always display From, expose Send and Attach directly, and
keep the existing rich formatting tools behind Formatting. Existing engine commands, offline storage,
rules, search, draft auto-save, keyboard shortcuts, attachments, and import/export remain in use.

Settings use an icon category sidebar, raised section cards, inset fields, fine dividers,
Mac-style switches, and a Light/Dark/Auto appearance selector. Labels and controls share consistent alignment. Save Changes
applies pending edits; Cancel discards them. Accounts use a separate mailbox list and detail pane;
legacy data-file and advanced account options remain accessible from that pane. The app icon is original
vector artwork in `assets/app-icon/mailcraft.svg`; `cargo xtask ico` regenerates all desktop formats.

## Shared and delegated Microsoft 365 mailboxes

This first implementation uses OAuth IMAP/SMTP, **not Microsoft Graph or EWS**. Microsoft documents
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
administration, Send on Behalf, Graph support for organizations that disable IMAP/SMTP, shared Exchange
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
