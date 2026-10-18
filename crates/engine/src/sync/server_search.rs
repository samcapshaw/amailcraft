#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]
// Explicit Microsoft 365 search. Results are transient; opening one adopts its preview normally.
use crate::{CmdError, CmdResult, Session};
use mailcraft_model::{AccountId, AccountKind, FolderId};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone, Debug)]
struct Binding {
    account: AccountId,
    address: String,
    endpoint: String,
    owner: AccountId,
    folder: Option<String>,
}
impl Binding {
    fn valid(&self, s: &Session) -> bool {
        s.mailbox.account(self.account).is_some_and(|a| {
            a.kind == AccountKind::Graph
                && a.address == self.address
                && a.incoming.as_ref().is_some_and(|i| i.host == self.endpoint)
                && crate::accounts::graph_credential_owner(s, a).ok() == Some(self.owner)
        })
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub account: AccountId,
    pub remote: String,
    pub subject: String,
    pub sender: String,
    pub preview: String,
    value: Value,
    binding: Binding,
}

#[cfg(not(target_arch = "wasm32"))]
type Reply = (Binding, Result<mailcraft_net::graph::GraphPage, String>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct PendingOpen {
    binding: Binding,
    id: mailcraft_model::ItemId,
    remote: String,
    baseline: crate::accounts::JmapShadow,
    reported: bool,
}

#[derive(Default)]
pub struct State {
    pub visible: bool,
    pub query: String,
    pub target: Option<AccountId>,
    pub only_folder: bool,
    pub rows: Vec<Row>,
    pub busy: bool,
    pub error: Option<String>,
    pub complete: bool,
    next: BTreeMap<AccountId, (Binding, String)>,
    seen_pages: BTreeSet<(AccountId, String)>,
    source_query: String,
    executed_query: String,
    source_folder: Option<FolderId>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_open: Option<PendingOpen>,
    #[cfg(not(target_arch = "wasm32"))]
    receiver: Option<mpsc::Receiver<Reply>>,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: Option<Arc<AtomicBool>>,
}
impl State {
    pub fn has_more(&self) -> bool {
        !self.next.is_empty()
            && self.rows.len() < 1000
            && self.seen_pages.len() < 320
            && self.query == self.executed_query
    }
    pub fn cancel(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        self.next.clear();
        self.rows.clear();
        self.visible = false;
    }
}

fn fail(text: &str) -> CmdError {
    CmdError::Failed(text.into())
}

pub fn command(s: &mut Session, p: &Value) -> CmdResult {
    let action = p.get("action").and_then(Value::as_str).unwrap_or("search");
    if action == "show" {
        s.server_search.visible = true;
        if !s.server_search.busy {
            s.server_search.rows.clear();
            s.server_search.next.clear();
            s.server_search.complete = false;
            s.server_search.error = None;
            s.server_search.query = s.view.search.clone();
            s.server_search.source_query = s.view.search.clone();
            s.server_search.target = s
                .current_account()
                .filter(|id| s.mailbox.account(*id).is_some_and(|a| a.kind == AccountKind::Graph));
            s.server_search.only_folder = !s.view.unified_inbox && s.server_search.target.is_some();
        }
        return Ok(json!({"opened":true}));
    }
    if action == "cancel" {
        s.server_search.cancel();
        return Ok(json!({"cancelled":true}));
    }
    if action == "results" {
        poll(s);
        return Ok(
            json!({"busy":s.server_search.busy,"error":s.server_search.error,"more":s.server_search.has_more(),"results":s.server_search.rows.iter().enumerate().map(|(index,r)| json!({"index":index,"account":r.account.0,"subject":r.subject,"sender":r.sender,"preview":r.preview})).collect::<Vec<_>>()}),
        );
    }
    if action != "search" && action != "more" {
        return Err(CmdError::Params("Unknown server-search action".into()));
    }
    if s.view.work_offline {
        return Err(fail("Go online before searching Microsoft 365."));
    }
    if s.server_search.busy {
        return Err(fail("Wait for the current server request to finish."));
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(CmdError::Disabled("Server search requires the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let more = action == "more";
        let query = if more {
            s.server_search.executed_query.clone()
        } else {
            p.get("query").and_then(Value::as_str).unwrap_or(&s.view.search).trim().to_owned()
        };
        if query.is_empty() || query.len() > 2000 || query.chars().any(char::is_control) {
            return Err(CmdError::Params(
                "Enter a server search of 1–2000 bytes without control characters.".into(),
            ));
        }
        let mut requests = Vec::new();
        if more {
            if !s.server_search.has_more() {
                return Err(fail("No further server results are available."));
            }
            for (binding, next) in s.server_search.next.values() {
                if !binding.valid(s) {
                    return Err(fail("The searched account changed. Start a new search."));
                }
                requests.push((binding.clone(), Some(next.clone())));
            }
        } else {
            let scope = p.get("scope").and_then(Value::as_str).unwrap_or("mailbox");
            if !matches!(scope, "folder" | "mailbox" | "all") {
                return Err(CmdError::Params("Use folder, mailbox or all scope.".into()));
            }
            let target = if scope == "all" {
                None
            } else {
                Some(match p.get("account").filter(|v| !v.is_null()) {
                    Some(value) => value
                        .as_u64()
                        .and_then(|v| u32::try_from(v).ok())
                        .map(AccountId)
                        .ok_or_else(|| CmdError::Params("A valid account id is required.".into()))?,
                    None => s.current_account().ok_or_else(|| fail("Choose a Microsoft 365 mailbox."))?,
                })
            };
            for a in &s.mailbox.accounts {
                if a.kind != AccountKind::Graph || target.is_some_and(|id| id != a.id) {
                    continue;
                }
                let folder = if scope == "folder" {
                    let f = s
                        .mailbox
                        .folder(s.selection.folder)
                        .filter(|f| f.account == a.id)
                        .ok_or_else(|| fail("The current folder belongs to a different mailbox."))?;
                    Some(
                        f.remote_name
                            .clone()
                            .ok_or_else(|| fail("Synchronize this folder before searching its server."))?,
                    )
                } else {
                    None
                };
                requests.push((
                    Binding {
                        account: a.id,
                        address: a.address.clone(),
                        endpoint: a
                            .incoming
                            .as_ref()
                            .ok_or_else(|| fail("The account has no server."))?
                            .host
                            .clone(),
                        owner: crate::accounts::graph_credential_owner(s, a)?,
                        folder,
                    },
                    None,
                ));
            }
            if requests.is_empty() {
                return Err(fail("Choose a connected Microsoft 365 mailbox."));
            }
            if requests.len() > 16 {
                return Err(fail("Search one mailbox at a time when more than 16 are connected."));
            }
        }
        let source_folder =
            requests.iter().any(|(binding, _)| binding.folder.is_some()).then_some(s.selection.folder);
        let mut jobs = Vec::new();
        for (binding, next) in requests {
            let a = s.mailbox.account(binding.account).ok_or_else(|| fail("The account was removed."))?;
            let conn = super::native::conn_settings(s, a).map_err(CmdError::Failed)?;
            let super::native::Incoming::Graph(conn) = conn.incoming else {
                return Err(fail("This account does not use Microsoft 365."));
            };
            jobs.push((binding, conn, next));
        }
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let worker_query = query.clone();
        let _worker = std::thread::Builder::new()
            .name("mailcraft-server-search".into())
            .spawn(move || {
                for (binding, conn, next) in jobs {
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let result = conn
                        .connect()
                        .and_then(|c| {
                            c.search_messages(&worker_query, binding.folder.as_deref(), next.as_deref())
                        })
                        .map_err(|e| match e {
                            mailcraft_net::NetError::Http { status: 401, .. } => {
                                "Sign in again to search this mailbox.".into()
                            }
                            mailcraft_net::NetError::Http { status: 403, .. } => {
                                "The sign-in does not permit searching this mailbox.".into()
                            }
                            mailcraft_net::NetError::Http { status: 429, .. } => {
                                "Microsoft 365 is busy. Try again later.".into()
                            }
                            _ => {
                                "The server search could not finish. Check the connection or query and retry."
                                    .into()
                            }
                        });
                    if stopped.load(Ordering::SeqCst) || tx.send((binding, result)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|_| fail("The server search worker could not start."))?;
        if !more {
            s.server_search.rows.clear();
            s.server_search.next.clear();
            s.server_search.seen_pages.clear();
        }
        s.server_search.source_folder = source_folder;
        s.server_search.executed_query = query.clone();
        s.server_search.query = query;
        s.server_search.source_query = s.view.search.clone();
        s.server_search.error = None;
        s.server_search.complete = false;
        s.server_search.busy = true;
        s.server_search.visible = true;
        s.server_search.receiver = Some(rx);
        s.server_search.cancel = Some(cancel);
        Ok(json!({"started":true}))
    }
}

pub fn poll(s: &mut Session) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut changed = finish_open(s);
        if (s.server_search.busy || !s.server_search.rows.is_empty() || !s.server_search.next.is_empty())
            && (s.view.work_offline
                || s.view.search != s.server_search.source_query
                || s.server_search.source_folder.is_some_and(|f| f != s.selection.folder))
        {
            s.server_search.cancel();
            changed = true;
        }
        loop {
            let result = s.server_search.receiver.as_ref().map(|rx| rx.try_recv());
            match result {
                Some(Ok((binding, result))) => {
                    changed = true;
                    if s.server_search.cancel.as_ref().is_some_and(|v| v.load(Ordering::SeqCst))
                        || !binding.valid(s)
                    {
                        continue;
                    }
                    match result {
                        Err(e) => s.server_search.error = Some(e),
                        Ok(page) => {
                            s.server_search.next.remove(&binding.account);
                            if let Some(next) = page.next {
                                if s.server_search.seen_pages.insert((binding.account, next.clone())) {
                                    s.server_search.next.insert(binding.account, (binding.clone(), next));
                                } else {
                                    s.server_search.error = Some(
                                        "The server repeated a results page. Start a new search.".into(),
                                    );
                                }
                            }
                            for value in page.values {
                                if s.server_search.rows.len() >= 1000 {
                                    break;
                                }
                                let Some(row) = row_of(binding.clone(), &value) else { continue };
                                if s.server_search
                                    .rows
                                    .iter()
                                    .any(|r| r.account == row.account && r.remote == row.remote)
                                {
                                    continue;
                                }
                                if s.sync
                                    .prefs
                                    .data
                                    .graph
                                    .get(&row.account.0)
                                    .is_some_and(|state| state.shadow.contains_key(&row.remote))
                                    || s.mailbox.mail.values().any(|m| {
                                        m.remote_id.as_ref() == Some(&row.remote)
                                            && s.mailbox
                                                .folder(m.folder)
                                                .is_some_and(|f| f.account == row.account)
                                    })
                                {
                                    continue;
                                }
                                s.server_search.rows.push(row);
                            }
                        }
                    }
                }
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    s.server_search.receiver = None;
                    s.server_search.cancel = None;
                    s.server_search.busy = false;
                    s.server_search.complete = true;
                    changed = true;
                    break;
                }
                _ => break,
            }
        }
        let removed = s.server_search.rows.iter().any(|r| !r.binding.valid(s));
        if removed {
            s.server_search.rows.clear();
            s.server_search.next.clear();
            changed = true;
        }
        changed
    }
}

fn row_of(binding: Binding, value: &Value) -> Option<Row> {
    let text = |key: &str, limit: usize| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(limit)
            .collect::<String>()
    };
    let identifier = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty() && v.len() <= 4096 && !v.chars().any(char::is_control))
            .map(str::to_owned)
    };
    let remote = identifier("id")?;
    let subject = text("subject", 4096);
    let preview = text("bodyPreview", 255);
    let sender = value
        .pointer("/from/emailAddress/address")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(1024)
        .collect::<String>();
    let sender_name = value
        .pointer("/from/emailAddress/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(512)
        .collect::<String>();
    let clean = json!({"id":remote,"parentFolderId":identifier("parentFolderId").unwrap_or_default(),"subject":subject,"bodyPreview":preview,"from":{"emailAddress":{"address":sender,"name":sender_name}},"internetMessageId":text("internetMessageId",2048),"receivedDateTime":text("receivedDateTime",64),"hasAttachments":value.get("hasAttachments").and_then(Value::as_bool).unwrap_or(false),"isRead":value.get("isRead").and_then(Value::as_bool).unwrap_or(false),"isDraft":value.get("isDraft").and_then(Value::as_bool).unwrap_or(false),"flag":{"flagStatus":value.pointer("/flag/flagStatus").and_then(Value::as_str).filter(|v|matches!(*v,"flagged"|"complete"|"notFlagged")).unwrap_or("notFlagged")}});
    Some(Row { account: binding.account, remote, subject, sender, preview, value: clean, binding })
}

pub fn open(s: &mut Session, p: &Value) -> CmdResult {
    let index = p
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| CmdError::Params("A result index is required.".into()))?;
    let row = s
        .server_search
        .rows
        .get(index)
        .cloned()
        .ok_or_else(|| fail("This server result is no longer available."))?;
    if !row.binding.valid(s) {
        return Err(fail("The account changed. Search again."));
    }
    let folder_name = row
        .binding
        .folder
        .clone()
        .or_else(|| {
            row.value
                .get("parentFolderId")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .ok_or_else(|| fail("The result has no folder. Refresh the mailbox and search again."))?;
    let folder = s
        .mailbox
        .folders
        .values()
        .find(|f| f.account == row.account && f.remote_name.as_ref() == Some(&folder_name))
        .map(|f| f.id)
        .ok_or_else(|| fail("Refresh this mailbox to discover the result's folder."))?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = folder;
        Err(CmdError::Disabled("Open server results in the desktop app.".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        finish_open(s);
        if s.server_search.pending_open.is_some() {
            return Err(fail("Wait for local storage to save the previous preview."));
        }
        let existing = s
            .mailbox
            .mail
            .values()
            .find(|m| {
                m.remote_id.as_ref() == Some(&row.remote)
                    && s.mailbox.folder(m.folder).is_some_and(|f| f.account == row.account)
            })
            .map(|m| (m.id, m.folder));
        if existing.is_none()
            && s.sync
                .prefs
                .data
                .graph
                .get(&row.account.0)
                .is_some_and(|state| state.shadow.contains_key(&row.remote))
        {
            return Err(fail(
                "This message has a queued removal. Finish synchronization before reopening it.",
            ));
        }
        let (id, folder) = if let Some(existing) = existing {
            existing
        } else {
            let mut inbox = Vec::new();
            // Adoption does not advance download cursors or invoke arrival rules.
            let (cursors, round) = s
                .sync
                .prefs
                .data
                .graph
                .get(&row.account.0)
                .map(|state| (state.email_state.clone(), state.graph_round.clone()))
                .unwrap_or_default();
            super::graph::apply_checkpoint(
                s,
                row.account,
                super::graph::Checkpoint {
                    previews: [(row.remote.clone(), row.value.clone())].into(),
                    list: vec![super::jmap::JEmail {
                        id: row.remote.clone(),
                        mailboxes: vec![folder_name.clone()],
                        keywords: super::graph::delta_keywords(&row.value, None),
                        size: None,
                        raw: None,
                    }],
                    gone: Vec::new(),
                    cursors,
                    round,
                },
                &mut inbox,
            );
            // The command journal must accept the preview before its server baseline is persisted.
            if let Some(state) = s.sync.prefs.data.graph.get_mut(&row.account.0) {
                state.shadow.remove(&row.remote);
            }
            let id = s
                .mailbox
                .mail
                .values()
                .find(|m| {
                    m.remote_id.as_ref() == Some(&row.remote)
                        && s.mailbox.folder(m.folder).is_some_and(|f| f.account == row.account)
                })
                .map(|m| m.id)
                .ok_or_else(|| fail("The result could not be opened."))?;
            if let Some(item) = s.mailbox.mail.get_mut(&id) {
                item.server_search_preview = true;
            }
            s.sync.prefs.data.headers_only.remove(&id.0);
            (id, folder)
        };
        s.run("nav.selectFolder", &json!({"folder":folder.0}))?;
        s.run("search.close", &Value::Null)?;
        s.run("nav.selectItem", &json!({"id":id.0}))?;
        s.server_search.visible = false;
        s.server_search.pending_open = Some(PendingOpen {
            binding: row.binding,
            id,
            remote: row.remote,
            baseline: crate::accounts::JmapShadow {
                mailbox: folder_name,
                keywords: super::graph::delta_keywords(&row.value, None),
            },
            reported: false,
        });
        Ok(json!({"item":id.0}))
    }
}

/// Complete adoption only after the command journal has accepted the preview.
pub(crate) fn finish_open(s: &mut Session) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(pending) = s.server_search.pending_open.as_ref() else {
            return false;
        };
        if !pending.binding.valid(s)
            || !s.mailbox.mail.get(&pending.id).is_some_and(|m| {
                m.remote_id.as_ref() == Some(&pending.remote)
                    && s.mailbox.folder(m.folder).is_some_and(|f| f.account == pending.binding.account)
            })
        {
            s.server_search.pending_open = None;
            return true;
        }
        if s.store.is_some() && !s.pending_ops.is_empty() {
            let report = s.server_search.pending_open.as_ref().is_some_and(|p| !p.reported);
            if report {
                if let Some(p) = s.server_search.pending_open.as_mut() {
                    p.reported = true;
                }
                s.notify(crate::Notice::Error("The server preview is waiting for local storage. Its full download will start after the preview is saved.".into()));
            }
            return report;
        }
        let Some(pending) = s.server_search.pending_open.take() else {
            return false;
        };
        let account = pending.binding.account;
        s.sync
            .prefs
            .data
            .graph
            .entry(account.0)
            .or_default()
            .shadow
            .entry(pending.remote)
            .or_insert(pending.baseline);
        let incomplete = super::cmds::is_header_only(s, pending.id);
        if incomplete {
            s.sync.prefs.data.headers_only.insert(pending.id.0);
        }
        if s.store.is_some() {
            crate::accounts::save_prefs(s);
        }
        if incomplete && !s.view.work_offline {
            s.sync.fetch.entry(account).or_default().push(pending.id);
            if let Err(e) = s.send_receive(
                &[account],
                super::Plan {
                    send: false,
                    receive: false,
                    folders: Some(Vec::<FolderId>::new()),
                    user: true,
                },
            ) {
                s.notify(crate::Notice::Error(format!(
                    "The preview is saved; full content could not download: {e}"
                )));
            }
        }
        true
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use mailcraft_model::FolderRole;

    #[allow(clippy::unwrap_used, clippy::indexing_slicing)]
    fn fixture() -> (Session, Binding, FolderId) {
        let mut s = crate::sample::sample_session();
        let result = s.run("accounts.add",&json!({"kind":"graph","address":"team@example.net","incoming":{"host":mailcraft_net::graph::ENDPOINT}})).unwrap();
        let account = AccountId(result["account"].as_u64().unwrap() as u32);
        let folder = s.mailbox.role_folder(account, FolderRole::Inbox).unwrap();
        s.mailbox.folders.get_mut(&folder).unwrap().remote_name = Some("inbox".into());
        let binding = Binding {
            account,
            address: "team@example.net".into(),
            endpoint: mailcraft_net::graph::ENDPOINT.into(),
            owner: account,
            folder: Some("inbox".into()),
        };
        s.view.search = "budget".into();
        s.server_search.source_query = s.view.search.clone();
        (s, binding, folder)
    }
    #[allow(clippy::unwrap_used)]
    fn deliver(s: &mut Session, binding: Binding, value: Value, next: Option<String>) {
        let (tx, rx) = mpsc::channel();
        s.server_search.receiver = Some(rx);
        s.server_search.cancel = Some(Arc::new(AtomicBool::new(false)));
        s.server_search.busy = true;
        tx.send((
            binding,
            Ok(mailcraft_net::graph::GraphPage { values: vec![value], next, ..Default::default() }),
        ))
        .unwrap();
        drop(tx);
        assert!(poll(s));
        assert!(!s.server_search.busy);
    }
    #[test]
    fn server_results_are_transient_deduplicated_and_bound_to_query_and_account() {
        let (mut s, binding, folder) = fixture();
        let before = s.mailbox.mail.len();
        let value = json!({"id":"remote","parentFolderId":"inbox","subject":"Budget","bodyPreview":"Review","hasAttachments":true});
        deliver(&mut s, binding.clone(), value.clone(), Some("page2".into()));
        assert_eq!(s.mailbox.mail.len(), before);
        assert_eq!(s.server_search.rows.len(), 1);
        deliver(&mut s, binding.clone(), value.clone(), Some("page2".into()));
        assert_eq!(s.server_search.rows.len(), 1);
        assert!(!s.server_search.has_more());
        assert!(s.server_search.error.as_ref().unwrap().contains("repeated"));
        s.server_search.rows.clear();
        s.mailbox.add_mail(mailcraft_model::MailItem {
            folder,
            remote_id: Some("remote".into()),
            ..Default::default()
        });
        deliver(&mut s, binding.clone(), value.clone(), None);
        assert!(s.server_search.rows.is_empty());
        s.view.search = "different".into();
        deliver(&mut s, binding.clone(), json!({"id":"other"}), None);
        assert!(s.server_search.rows.is_empty());
        s.server_search.source_query = s.view.search.clone();
        s.mailbox.accounts.retain(|a| a.id != binding.account);
        deliver(&mut s, binding, value, None);
        assert!(s.server_search.rows.is_empty());
    }
    #[test]
    fn opening_a_server_result_adopts_only_its_mailbox_without_advancing_sync() {
        let (mut s, binding, folder) = fixture();
        s.sync.prefs.data.graph.entry(binding.account.0).or_default().email_state = "saved-cursor".into();
        let before = s.mailbox.mail.len();
        let row=row_of(binding.clone(),&json!({"id":"remote","parentFolderId":"inbox","subject":"Budget","hasAttachments":true,"isRead":true})).unwrap();
        s.server_search.rows.push(row);
        s.view.work_offline = true;
        let result = s.run("search.openServerResult", &json!({"index":0})).unwrap();
        let id = mailcraft_model::ItemId(result["item"].as_u64().unwrap());
        assert_eq!(s.mailbox.mail.len(), before + 1);
        assert_eq!(s.selection.folder, folder);
        assert_eq!(s.selection.focus, Some(id));
        assert!(s.mailbox.mail[&id].has_attachments());
        assert!(s.mailbox.mail[&id].read);
        assert!(s.mailbox.mail[&id].server_search_preview);
        assert_eq!(s.sync.prefs.data.graph[&binding.account.0].email_state, "saved-cursor");
        s.mailbox.accounts.retain(|a| a.id != binding.account);
        assert!(s.run("search.openServerResult", &json!({"index":0})).is_err());
    }
    #[test]
    fn server_preview_baseline_waits_for_durable_mail_and_survives_reload() {
        let (mut s, binding, _) = fixture();
        let dir = tempfile::tempdir().unwrap();
        s.load_settings(dir.path());
        crate::accounts::load_prefs(&mut s);
        s.view.work_offline = true;
        s.view.search = "budget".into();
        s.pending_ops.clear();
        let mut store = mailcraft_store::Store::open(dir.path()).unwrap();
        store.checkpoint(&s.mailbox).unwrap();
        s.store = Some(store);
        s.server_search.rows.push(row_of(binding.clone(),&json!({"id":"durable","parentFolderId":"inbox","subject":"Saved preview","hasAttachments":true})).unwrap());
        // Exercise the command body before the registry journals its mailbox diff.
        let result = open(&mut s, &json!({"index":0})).unwrap();
        let id = mailcraft_model::ItemId(result["item"].as_u64().unwrap());
        s.pending_ops.push(mailcraft_store::Op::PutMail(Box::new(s.mailbox.mail[&id].clone())));
        assert!(finish_open(&mut s));
        assert!(s.server_search.pending_open.is_some());
        assert!(
            s.sync
                .prefs
                .data
                .graph
                .get(&binding.account.0)
                .is_none_or(|state| !state.shadow.contains_key("durable"))
        );
        assert!(!s.store.as_ref().unwrap().load().unwrap().mail.contains_key(&id));
        s.flush_pending();
        assert!(s.pending_ops.is_empty());
        assert!(s.server_search.pending_open.is_none());
        assert!(s.sync.prefs.data.graph[&binding.account.0].shadow.contains_key("durable"));
        s.store = None;
        let mut reopened = Session::open(dir.path()).unwrap();
        crate::accounts::load_prefs(&mut reopened);
        assert!(reopened.mailbox.mail[&id].has_attachments());
        assert!(reopened.sync.prefs.data.graph[&binding.account.0].shadow.contains_key("durable"));
    }

    #[test]
    fn server_search_worker_pages_explicitly_and_never_imports_results_automatically() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::time::{Duration, Instant};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let endpoint = format!("{base}/v1.0");
        let next = format!("{endpoint}/users/team%40example.net/messages?opaque=second");
        let worker = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            for page in 0..2 {
                let deadline = Instant::now() + Duration::from_secs(10);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(stream) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => panic!("fixture accept: {e}"),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                assert!(request.starts_with("GET /v1.0/users/team%40example.net/messages?"));
                if page == 0 {
                    assert!(request.contains("$search="));
                } else {
                    assert!(request.contains("opaque=second"));
                }
                let mut auth = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    auth |= line.trim() == "Authorization: Bearer fixture-token";
                }
                assert!(auth);
                let body = if page == 0 {
                    json!({"value":[{"id":"one","subject":"Budget one"}],"@odata.nextLink":next})
                } else {
                    json!({"value":[{"id":"one"},{"id":"two","subject":"Budget two"}]})
                }
                .to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        let mut s = crate::sample::sample_session();
        let result=s.run("accounts.add",&json!({"kind":"graph","address":"team@example.net","incoming":{"host":endpoint,"allowPlaintextAuth":true},"oauthToken":"fixture-token"})).unwrap();
        let account = result["account"].as_u64().unwrap();
        s.view.search = "budget".into();
        let before = s.mailbox.mail.len();
        s.run("search.server", &json!({"account":account,"query":"budget"})).unwrap();
        assert!(s.run("search.server", &json!({"account":account,"query":"another"})).is_err());
        let wait = |s: &mut Session| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while s.server_search.busy && Instant::now() < deadline {
                poll(s);
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!s.server_search.busy);
            assert!(s.server_search.error.is_none(), "{:?}", s.server_search.error);
        };
        wait(&mut s);
        assert_eq!(s.server_search.rows.len(), 1);
        assert!(s.server_search.has_more());
        assert_eq!(s.mailbox.mail.len(), before);
        s.run("search.server", &json!({"action":"more"})).unwrap();
        wait(&mut s);
        assert_eq!(s.server_search.rows.len(), 2);
        assert!(!s.server_search.has_more());
        assert_eq!(s.mailbox.mail.len(), before);
        worker.join().unwrap();
    }

    #[test]
    fn server_results_cannot_resurrect_a_queued_remote_removal() {
        let (mut s, binding, _) = fixture();
        s.sync.prefs.data.graph.entry(binding.account.0).or_default().shadow.insert(
            "removed".into(),
            crate::accounts::JmapShadow { mailbox: "inbox".into(), keywords: Vec::new() },
        );
        let value = json!({"id":"removed","parentFolderId":"inbox"});
        deliver(&mut s, binding.clone(), value.clone(), None);
        assert!(s.server_search.rows.is_empty());
        s.server_search.rows.push(row_of(binding, &value).unwrap());
        assert!(s.run("search.openServerResult", &json!({"index":0})).is_err());
        assert!(!s.mailbox.mail.values().any(|m| m.remote_id.as_deref() == Some("removed")));
    }

    #[test]
    fn server_search_refuses_offline_and_invalid_queries() {
        let (mut s, binding, _) = fixture();
        s.view.work_offline = true;
        assert!(s.run("search.server", &json!({"account":binding.account.0,"query":"budget"})).is_err());
        s.view.work_offline = false;
        assert!(s.run("search.server", &json!({"query":"x".repeat(2001)})).is_err());
        assert!(s.run("search.server", &json!({"query":"x\ny"})).is_err());
        assert!(matches!(
            s.run("search.server", &json!({"account":u64::MAX,"query":"budget"})),
            Err(CmdError::Params(_))
        ));
        assert!(s.run("search.openServerResult", &json!({"index":u64::MAX})).is_err());
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}
