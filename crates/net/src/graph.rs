//! Microsoft Graph v1.0 mail transport. Tokens and continuation URLs never leave the configured origin.
use crate::http::{HttpClient, Url, percent_decode, percent_encode};
use crate::{ConnectOptions, NetError, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod pace;

pub const ENDPOINT: &str = "https://graph.microsoft.com/v1.0";
const MAX_PAGES: usize = 1000;
const MAX_ITEMS: usize = 100_000;

#[derive(Clone, PartialEq, Eq)]
pub struct GraphConn {
    pub endpoint: String,
    pub mailbox: String,
    token: String,
    pub options: ConnectOptions,
}
impl std::fmt::Debug for GraphConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphConn")
            .field("endpoint", &self.endpoint)
            .field("mailbox", &self.mailbox)
            .finish_non_exhaustive()
    }
}
impl GraphConn {
    pub fn new(endpoint: &str, mailbox: &str, token: &str, options: ConnectOptions) -> Result<Self> {
        let url = Url::parse(endpoint)?;
        if !url.https
            && !(options.allow_plaintext_auth
                && matches!(url.host.as_str(), "127.0.0.1" | "localhost" | "::1"))
        {
            return Err(NetError::Insecure("Graph requires HTTPS".into()));
        }
        if mailbox.is_empty()
            || mailbox.chars().any(char::is_control)
            || token.is_empty()
            || token.chars().any(char::is_control)
        {
            return Err(NetError::Invalid("Graph needs a mailbox address and an OAuth token".into()));
        }
        if url.path.contains('?') {
            return Err(NetError::Invalid("Graph endpoint must not contain a query".into()));
        }
        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').into(),
            mailbox: mailbox.into(),
            token: token.into(),
            options,
        })
    }
    pub fn set_bearer(&mut self, token: &str) {
        self.token = token.into();
    }
    pub fn connect(&self) -> Result<GraphClient> {
        GraphClient::new(self.clone())
    }
}

pub struct GraphClient {
    conn: GraphConn,
    http: HttpClient,
    root: String,
}
#[derive(Clone, Debug, Default)]
pub struct GraphPage {
    pub values: Vec<Value>,
    pub delta: Option<String>,
    /// Continuation of a single-page download.
    pub next: Option<String>,
    /// Total HTTP payload bytes across every page.
    pub bytes_received: usize,
}
impl GraphClient {
    fn new(conn: GraphConn) -> Result<Self> {
        let mut http = HttpClient::new(conn.options.clone());
        http.max_redirects = 0;
        http.max_body = 64 * 1024 * 1024;
        let root = format!("{}/users/{}", conn.endpoint, percent_encode(&conn.mailbox));
        Ok(Self { conn, http, root })
    }
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.root)
    }
    fn checked_url(&self, url: &str) -> Result<()> {
        if url.len() > 64 * 1024 {
            return Err(NetError::TooLarge("Graph continuation URL".into()));
        }
        let decoded = percent_decode(&Url::parse(url)?.path, false);
        if decoded.split('/').any(|part| matches!(part, "." | "..")) {
            return Err(NetError::Invalid("Graph URL contains path traversal".into()));
        }
        let base = Url::parse(&self.root)?;
        let target = Url::parse(url)?;
        if !base.same_origin(&target)
            || !percent_decode(&target.path, false)
                .starts_with(&format!("{}/", percent_decode(&base.path, false)))
        {
            return Err(NetError::Invalid("Graph continuation URL escaped the mailbox".into()));
        }
        Ok(())
    }
    fn request(&self, method: &str, url: &str, content_type: &str, body: Option<&[u8]>) -> Result<Vec<u8>> {
        self.request_version(method, url, content_type, body, None)
    }
    fn request_version(
        &self,
        method: &str,
        url: &str,
        content_type: &str,
        body: Option<&[u8]>,
        etag: Option<&str>,
    ) -> Result<Vec<u8>> {
        self.checked_url(url)?;
        if etag.is_some_and(|v| v.is_empty() || v.len() > 2048 || v.chars().any(char::is_control)) {
            return Err(NetError::Invalid("Invalid Graph item version".into()));
        }
        let auth = format!("Bearer {}", self.conn.token);
        let mut headers = vec![
            ("Authorization", auth.as_str()),
            ("Content-Type", content_type),
            ("Prefer", "IdType=\"ImmutableId\", outlook.timezone=\"UTC\""),
            ("Accept", content_type),
        ];
        if let Some(etag) = etag {
            headers.push(("If-Match", etag));
        }
        let mailbox_key = pace::key(&self.conn.endpoint, &self.conn.mailbox);
        pace::check(&mailbox_key)?;
        let response = self.http.request(method, url, &headers, body)?;
        if response.status == 429 || (response.status == 503 && response.header("Retry-After").is_some()) {
            pace::pause(&mailbox_key, response.header("Retry-After"))?;
        }
        if !response.is_success() {
            let value: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
            let code = value.pointer("/error/code").and_then(Value::as_str).unwrap_or("Graph request failed");
            // Do not echo response bodies that might contain credentials or message contents.
            let help = match response.status {
                401 => "Sign in again with a Microsoft Graph authorization grant.",
                403 => "The signed-in user or application lacks permission for this mailbox operation.",
                429 => "Microsoft Graph is throttling requests; try again later.",
                412 => "The item changed on Microsoft 365. Refresh and review your proposal before retrying.",
                _ => "The server could not complete the operation.",
            };
            return Err(NetError::Http {
                status: response.status,
                body: format!("{}: {help}", code.chars().take(120).collect::<String>()),
            });
        }
        Ok(response.body)
    }
    fn json(&self, method: &str, path: &str, value: Option<&Value>) -> Result<Value> {
        let body = value.map(serde_json::to_vec).transpose().map_err(|e| NetError::Invalid(e.to_string()))?;
        let bytes = self.request(method, &self.url(path), "application/json", body.as_deref())?;
        if bytes.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&bytes).map_err(|e| NetError::Protocol(e.to_string()))
        }
    }
    pub fn collection(&self, path: &str, continuation: Option<&str>) -> Result<GraphPage> {
        let mut next = continuation.map(str::to_owned).unwrap_or_else(|| self.url(path));
        let mut seen = BTreeSet::new();
        let mut result = GraphPage::default();
        let mut bytes_received = 0usize;
        for _ in 0..MAX_PAGES {
            if !seen.insert(next.clone()) {
                return Err(NetError::Protocol("Graph repeated a continuation URL".into()));
            }
            let bytes = self.request("GET", &next, "application/json", None)?;
            bytes_received = bytes_received.saturating_add(bytes.len());
            if bytes_received > 64 * 1024 * 1024 {
                return Err(NetError::TooLarge("Graph collection bytes".into()));
            }
            let doc: Value = serde_json::from_slice(&bytes).map_err(|e| NetError::Protocol(e.to_string()))?;
            let values = doc
                .get("value")
                .and_then(Value::as_array)
                .ok_or_else(|| NetError::Protocol("Graph collection has no value array".into()))?;
            if result.values.len().saturating_add(values.len()) > MAX_ITEMS {
                return Err(NetError::TooLarge("Graph collection".into()));
            }
            result.values.extend(values.iter().cloned());
            if let Some(url) = doc.get("@odata.nextLink").and_then(Value::as_str) {
                next = url.into();
                continue;
            }
            if let Some(url) = doc.get("@odata.deltaLink").and_then(Value::as_str) {
                self.checked_url(url)?;
                result.delta = Some(url.into());
            }
            result.bytes_received = bytes_received;
            return Ok(result);
        }
        Err(NetError::TooLarge("Graph pagination limit".into()))
    }
    /// Conditional updates and meeting responses. Neither the caller nor transport retries mutations.
    pub fn write_connected(
        &self,
        kind: &str,
        id: &str,
        action: &str,
        etag: &str,
        payload: &Value,
    ) -> Result<()> {
        if kind == "event" && action == "create" {
            let bytes = serde_json::to_vec(payload).map_err(|e| NetError::Invalid(e.to_string()))?;
            if id.is_empty()
                || bytes.len() > 32 * 1024
                || payload
                    .get("transactionId")
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.is_empty() || s.len() > 128)
            {
                return Err(NetError::Invalid("Creation needs a calendar and stable transaction ID".into()));
            }
            let calendar = self.json("GET", &format!("calendars/{}", percent_encode(id)), None)?;
            if calendar.get("canEdit").and_then(Value::as_bool) != Some(true) {
                return Err(NetError::Refused("This calendar does not permit creation.".into()));
            }
            return match self.request_version(
                "POST",
                &self.url(&format!("calendars/{}/events", percent_encode(id))),
                "application/json",
                Some(&bytes),
                None,
            ) {
                Ok(body) => {
                    let result: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    if result.get("id").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
                        Ok(())
                    } else {
                        Err(NetError::Refused("Outcome uncertain. The server returned no event identity; check the calendar before retrying.".into()))
                    }
                }
                Err(error)
                    if error.is_connection_lost()
                        || matches!(error, NetError::Http { status: 500..=599, .. }) =>
                {
                    Err(NetError::Refused(
                        "Outcome uncertain. Check the calendar before retrying this event.".into(),
                    ))
                }
                Err(error) => Err(error),
            };
        }
        if !["event", "contact"].contains(&kind)
            || !(action == "patch"
                || kind == "event" && ["accept", "tentativelyAccept", "decline", "delete"].contains(&action))
        {
            return Err(NetError::Invalid("Unsupported connected operation".into()));
        }
        if etag.is_empty() {
            return Err(NetError::Invalid("Refresh this item to obtain its server version".into()));
        }
        let path = format!(
            "{}/{}{}",
            if kind == "event" { "events" } else { "contacts" },
            percent_encode(id),
            if action == "patch" || action == "delete" { String::new() } else { format!("/{action}") }
        );

        if action != "patch" {
            let current = self.json("GET", &format!("events/{}", percent_encode(id)), None)?;
            if action == "delete"
                && (current.get("isOrganizer").and_then(Value::as_bool) != Some(true)
                    || current.get("type").and_then(Value::as_str) != Some("singleInstance")
                    || current.get("attendees").and_then(Value::as_array).is_none_or(|v| !v.is_empty()))
            {
                return Err(NetError::Refused(
                    "Deletion is limited to single appointments without attendees; refresh first.".into(),
                ));
            }
            if current.get("@odata.etag").and_then(Value::as_str) != Some(etag) {
                return Err(NetError::Http {
                    status: 412,
                    body: "The meeting changed. Refresh and review its current details before responding."
                        .into(),
                });
            }
        }
        let mut payload = payload.clone();
        // Email arrays replace the whole collection. Preserve the existing address display name when
        // changing a single address; a stale server version is refused before constructing that patch.
        if kind == "contact"
            && payload.get("emailAddresses").and_then(Value::as_array).is_some_and(|v| v.len() == 1)
        {
            let current = self.json("GET", &format!("contacts/{}", percent_encode(id)), None)?;
            if current.get("@odata.etag").and_then(Value::as_str) != Some(etag) {
                return Err(NetError::Http {status:412,body:"The contact changed. Refresh and review its current details before changing the address.".into()});
            }
            if current.get("emailAddresses").and_then(Value::as_array).is_some_and(|v| v.len() > 1) {
                return Err(NetError::Refused(
                    "This contact now has multiple addresses. Refresh and edit them in Outlook.".into(),
                ));
            }
            if let Some(name) = current
                .get("emailAddresses")
                .and_then(Value::as_array)
                .and_then(|v| v.first())
                .and_then(|v| v.get("name"))
                .and_then(Value::as_str)
                && let Some(address) = payload
                    .get_mut("emailAddresses")
                    .and_then(Value::as_array_mut)
                    .and_then(|v| v.first_mut())
                    .and_then(Value::as_object_mut)
            {
                address.insert("name".into(), json!(name));
            }
        }
        let bytes = serde_json::to_vec(&payload).map_err(|e| NetError::Invalid(e.to_string()))?;
        if bytes.len() > 32 * 1024 {
            return Err(NetError::TooLarge("Connected change".into()));
        }
        match self.request_version(if action=="patch" {"PATCH"}else if action=="delete" {"DELETE"}else{"POST"},&self.url(&path),"application/json",if action=="delete" {None}else{Some(&bytes)},Some(etag)) {
            Ok(_)=>Ok(()),
            Err(error) if error.is_connection_lost() || matches!(error,NetError::Http{status:500..=599,..})=>Err(NetError::Refused("Outcome uncertain. Microsoft may have accepted the change; check Outlook before retrying.".into())),
            Err(error)=>Err(error),
        }
    }

    /// Expanded occurrences and exceptions are supplied by Graph; callers must not expand them again.
    pub fn calendar_view(&self, calendar: &str, start: &str, end: &str) -> Result<GraphPage> {
        self.collection(
            &format!(
                "calendars/{}/calendarView?startDateTime={}&endDateTime={}&$top=100",
                percent_encode(calendar),
                percent_encode(start),
                percent_encode(end)
            ),
            None,
        )
    }
    pub fn calendars(&self) -> Result<GraphPage> {
        self.collection("calendars?$top=100", None)
    }
    pub fn contacts(&self, folder: Option<&str>) -> Result<GraphPage> {
        let path = match folder {
            Some(id) => format!("contactFolders/{}/contacts?$top=100", percent_encode(id)),
            None => "contacts?$top=100".into(),
        };
        self.collection(&path, None)
    }
    pub fn folders(&self) -> Result<Vec<Value>> {
        let mut folders = self.collection("mailFolders?$top=100", None)?.values;
        let mut cursor = 0;
        let mut visited = BTreeSet::new();
        while let Some(folder) = folders.get(cursor).cloned() {
            cursor += 1;
            let id = folder
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| NetError::Protocol("Graph folder has no id".into()))?;
            if !visited.insert(id.to_owned()) {
                return Err(NetError::Protocol("Graph repeated a folder id".into()));
            }
            if folder.get("childFolderCount").and_then(Value::as_u64).unwrap_or(0) > 0 {
                let children = self
                    .collection(&format!("mailFolders/{}/childFolders?$top=100", percent_encode(id)), None)?
                    .values;
                if folders.len().saturating_add(children.len()) > 10_000 {
                    return Err(NetError::TooLarge("Graph folder tree".into()));
                }
                folders.extend(children);
            }
        }
        Ok(folders)
    }
    pub fn well_known_folder(&self, name: &str) -> Result<Value> {
        self.json("GET", &format!("mailFolders/{}?$select=id", percent_encode(name)), None)
    }
    pub fn messages_delta(&self, folder: &str, cursor: Option<&str>) -> Result<GraphPage> {
        self.collection(
            &format!(
                "mailFolders/{}/messages/delta?$select=id,parentFolderId,isRead,isDraft,flag&$top=100",
                percent_encode(folder)
            ),
            cursor,
        )
    }

    /// One message delta page. Callers checkpoint it before following the next link.
    pub fn messages_delta_page(&self, folder: &str, cursor: Option<&str>) -> Result<GraphPage> {
        let url=cursor.map(str::to_owned).unwrap_or_else(||self.url(&format!("mailFolders/{}/messages/delta?$select=id,parentFolderId,isRead,isDraft,flag,receivedDateTime,subject,from,toRecipients,ccRecipients,replyTo,internetMessageId,bodyPreview,hasAttachments&$orderby=receivedDateTime%20desc&$top=100",percent_encode(folder))));
        self.checked_url(&url)?;
        let bytes = self.request("GET", &url, "application/json", None)?;
        let doc: Value = serde_json::from_slice(&bytes).map_err(|e| NetError::Protocol(e.to_string()))?;
        let values = doc
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| NetError::Protocol("Graph page has no message array".into()))?;
        if values.len() > 1000 {
            return Err(NetError::TooLarge("Graph message page".into()));
        }
        let link = |key: &str| -> Result<Option<String>> {
            match doc.get(key) {
                None => Ok(None),
                Some(v) => {
                    let link =
                        v.as_str().ok_or_else(|| NetError::Protocol("Invalid Graph continuation".into()))?;
                    self.checked_url(link)?;
                    Ok(Some(link.into()))
                }
            }
        };
        let next = link("@odata.nextLink")?;
        let delta = link("@odata.deltaLink")?;
        if next.is_some() == delta.is_some() || next.as_deref() == Some(&url) {
            return Err(NetError::Protocol("Graph page has invalid continuation state".into()));
        }
        Ok(GraphPage { values: values.clone(), delta, next, bytes_received: bytes.len() })
    }
    pub fn mime(&self, id: &str) -> Result<Vec<u8>> {
        self.request("GET", &self.url(&format!("messages/{}/$value", percent_encode(id))), "text/plain", None)
    }
    pub fn update(&self, id: &str, read: bool, flag_status: &str) -> Result<()> {
        if !matches!(flag_status, "flagged" | "notFlagged" | "complete") {
            return Err(NetError::Invalid("unknown Graph flag status".into()));
        }
        self.json(
            "PATCH",
            &format!("messages/{}", percent_encode(id)),
            Some(&json!({"isRead":read,"flag":{"flagStatus":flag_status}})),
        )?;
        Ok(())
    }
    pub fn move_message(&self, id: &str, folder: &str) -> Result<()> {
        self.json(
            "POST",
            &format!("messages/{}/move", percent_encode(id)),
            Some(&json!({"destinationId":folder})),
        )?;
        Ok(())
    }
    pub fn delete(&self, id: &str) -> Result<()> {
        self.json("DELETE", &format!("messages/{}", percent_encode(id)), None)?;
        Ok(())
    }
    pub fn send_mime(&self, mime: &[u8]) -> Result<()> {
        use base64::Engine;
        if mime.len() > 3 * 1024 * 1024 {
            return Err(NetError::TooLarge("Graph direct MIME sending currently supports messages up to 3 MiB; larger messages need an upload session.".into()));
        }
        let raw = base64::engine::general_purpose::STANDARD.encode(mime);
        if let Err(error) = self.request("POST", &self.url("sendMail"), "text/plain", Some(raw.as_bytes())) {
            if error.is_connection_lost() || matches!(error, NetError::Http { status: 500..=599, .. }) {
                return Err(NetError::Refused("Delivery status unknown. Microsoft may have accepted the message; check the server Sent folder before resending.".into()));
            }
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testserver::{json as response, serve};
    use serde_json::json;

    #[test]
    fn throttle_pauses_other_clients_without_replaying_send() {
        let (base, thread, seen) = serve(1, |req| {
            assert_eq!(req.method, "POST");
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec()
        });
        let conn = GraphConn::new(
            &format!("{base}/v1.0"),
            "throttle@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap();
        assert!(matches!(
            conn.connect().unwrap().send_mime(b"Subject: Test\r\n\r\nTest"),
            Err(NetError::Http { status: 429, .. })
        ));
        thread.join().unwrap();
        // The fixture is closed. A second client must defer locally, not attempt a connection.
        let other = conn.connect().unwrap();
        assert!(matches!(other.collection("mailFolders", None), Err(NetError::Http { status: 429, .. })));
        assert!(matches!(
            other.send_mime(b"Subject: Test\r\n\r\nTest"),
            Err(NetError::Http { status: 429, .. })
        ));
        assert_eq!(seen.try_iter().count(), 1);
    }

    #[test]
    fn message_pages_stop_at_checkpoint_and_resume_opaque_url() {
        let (base, thread, seen) = serve(2, |req| {
            let host = req.header("host").unwrap();
            if req.path.contains("opaque=token") {
                response(
                    200,
                    &json!({"value":[],"@odata.deltaLink":format!("http://{host}/v1.0/users/owner%40example.net/delta?opaque=done")})
                        .to_string(),
                )
            } else {
                assert!(req.path.contains("receivedDateTime%20desc"));
                response(200, &json!({"value":[{"id":"m1"}],"@odata.nextLink":format!("http://{host}/v1.0/users/owner%40example.net/next?opaque=token")}).to_string())
            }
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "owner@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        let first = client.messages_delta_page("inbox", None).unwrap();
        assert_eq!(first.values.len(), 1);
        assert!(first.delta.is_none());
        let second = client.messages_delta_page("inbox", first.next.as_deref()).unwrap();
        assert!(second.next.is_none());
        assert!(second.delta.is_some());
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 2);
    }

    #[test]
    fn message_page_rejects_missing_or_ambiguous_checkpoints() {
        let (base, thread, _) = serve(2, |req| {
            let host = req.header("host").unwrap();
            if req.path.contains("missing") {
                response(200, r#"{"value":[]}"#)
            } else {
                response(200, &json!({"value":[],"@odata.nextLink":format!("http://{host}/v1.0/users/owner%40example.net/next"),"@odata.deltaLink":format!("http://{host}/v1.0/users/owner%40example.net/delta")}).to_string())
            }
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "owner@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        assert!(client.messages_delta_page("missing", None).is_err());
        assert!(client.messages_delta_page("both", None).is_err());
        thread.join().unwrap();
    }

    #[test]
    fn appointment_create_is_calendar_scoped_and_carries_the_stable_transaction() {
        let (base, thread, seen) = serve(2, |req| {
            if req.method == "GET" {
                assert!(req.path.ends_with("/calendars/team"));
                response(200, r#"{"canEdit":true}"#)
            } else {
                assert_eq!(req.method, "POST");
                assert!(req.path.ends_with("/calendars/team/events"));
                assert_eq!(serde_json::from_slice::<Value>(&req.body).unwrap()["transactionId"], "stable-id");
                response(201, r#"{"id":"new-event"}"#)
            }
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        client
            .write_connected(
                "event",
                "team",
                "create",
                "",
                &json!({"transactionId":"stable-id","subject":"Planning"}),
            )
            .unwrap();
        // A request without a stable identifier never reaches the server.
        assert!(client.write_connected("event", "team", "create", "", &json!({})).is_err());
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 2);
    }
    #[test]
    fn meeting_creation_preserves_attendee_roles_and_transaction_in_one_post() {
        let (base, thread, seen) = serve(2, |req| {
            if req.method == "GET" {
                return response(200, r#"{"canEdit":true}"#);
            }
            assert_eq!(req.method, "POST");
            assert!(req.path.ends_with("/calendars/team/events"));
            let data: Value = serde_json::from_slice(&req.body).unwrap();
            assert_eq!(data["attendees"][0]["emailAddress"]["address"], "maya@example.net");
            assert_eq!(data["attendees"][1]["type"], "optional");
            assert_eq!(data["transactionId"], "meeting-fixture");
            response(201, r#"{"id":"new-meeting"}"#)
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        client.write_connected("event","team","create","",&serde_json::json!({"subject":"Review","transactionId":"meeting-fixture","attendees":[{"emailAddress":{"address":"maya@example.net"},"type":"required"},{"emailAddress":{"address":"alex@example.net"},"type":"optional"}]})).unwrap();
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 2);
    }
    #[test]
    fn delete_preflights_shape_and_sends_a_conditional_empty_delete() {
        let (base, thread, seen) = serve(2, |req| {
            if req.method == "GET" {
                response(
                    200,
                    r#"{"@odata.etag":"v1","type":"singleInstance","isOrganizer":true,"attendees":[]}"#,
                )
            } else {
                assert_eq!(req.method, "DELETE");
                assert!(req.body.is_empty());
                assert_eq!(req.header("if-match"), Some("v1"));
                response(204, "")
            }
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        client.write_connected("event", "event", "delete", "v1", &json!({})).unwrap();
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 2);
    }
    #[test]
    fn lifecycle_preflights_refuse_read_only_calendars_and_changed_or_meeting_events() {
        for (action, body) in [
            ("create", json!({"canEdit":false})),
            ("delete", json!({"@odata.etag":"v2","type":"singleInstance","isOrganizer":true,"attendees":[]})),
            (
                "delete",
                json!({"@odata.etag":"v1","type":"singleInstance","isOrganizer":true,"attendees":[{"emailAddress":{"address":"a@example.net"}}]}),
            ),
            ("delete", json!({"@odata.etag":"v1","type":"seriesMaster","isOrganizer":true,"attendees":[]})),
        ] {
            let (base, thread, seen) = serve(1, move |req| {
                assert_eq!(req.method, "GET");
                response(200, &body.to_string())
            });
            let client = GraphConn::new(
                &format!("{base}/v1.0"),
                "team@example.net",
                "fixture",
                ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
            )
            .unwrap()
            .connect()
            .unwrap();
            assert!(
                client
                    .write_connected("event", "target", action, "v1", &json!({"transactionId":"stable"}))
                    .is_err()
            );
            thread.join().unwrap();
            assert_eq!(seen.try_iter().count(), 1);
        }
    }
    #[test]
    fn creation_without_a_confirmed_identity_is_held_not_replayed() {
        let (base, thread, seen) = serve(2, |req| {
            response(
                if req.method == "GET" { 200 } else { 201 },
                if req.method == "GET" { r#"{"canEdit":true}"# } else { "{}" },
            )
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        assert!(
            client
                .write_connected("event", "calendar", "create", "", &json!({"transactionId":"stable"}))
                .unwrap_err()
                .to_string()
                .contains("Outcome uncertain")
        );
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 2);
    }
    #[test]
    fn changing_a_contact_address_preserves_its_server_display_name() {
        let (base, thread, _) = serve(2, |req| {
            if req.method == "GET" {
                response(200,&json!({"@odata.etag":"W/\"one\"","emailAddresses":[{"address":"old@example.net","name":"Maya (work)"}]}).to_string())
            } else {
                assert_eq!(
                    serde_json::from_slice::<Value>(&req.body).unwrap()["emailAddresses"],
                    json!([{"address":"new@example.net","name":"Maya (work)"}])
                );
                response(200, "{}")
            }
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        client
            .write_connected(
                "contact",
                "c",
                "patch",
                "W/\"one\"",
                &json!({"emailAddresses":[{"address":"new@example.net"}]}),
            )
            .unwrap();
        thread.join().unwrap();
    }

    #[test]
    fn connected_writes_are_conditional_and_responses_check_the_current_version() {
        let (base, thread, seen) = serve(5, |req| match (req.method.as_str(), req.path.as_str()) {
            ("PATCH", p) if p.contains("/contacts/") => {
                assert_eq!(req.header("if-match"), Some("W/\"one\""));
                let data: Value = serde_json::from_slice(&req.body).unwrap();
                assert_eq!(data, json!({"companyName":"Studio"}));
                response(200, "{}")
            }
            ("PATCH", _) => response(412, "{\"error\":{\"code\":\"ErrorIrresolvableConflict\"}}"),
            ("GET", p) => response(
                200,
                &json!({"@odata.etag":if p.ends_with("/stale") {"W/\"two\""}else{"W/\"one\""}}).to_string(),
            ),
            ("POST", p) => {
                assert!(p.ends_with("/accept"));
                assert_eq!(req.header("if-match"), Some("W/\"one\""));
                assert_eq!(serde_json::from_slice::<Value>(&req.body).unwrap()["sendResponse"], true);
                response(202, "")
            }
            _ => panic!("unexpected request"),
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        client
            .write_connected("contact", "c", "patch", "W/\"one\"", &json!({"companyName":"Studio"}))
            .unwrap();
        assert!(matches!(
            client.write_connected("event", "e", "patch", "W/\"one\"", &json!({"subject":"Review"})),
            Err(NetError::Http { status: 412, .. })
        ));
        client
            .write_connected(
                "event",
                "e",
                "accept",
                "W/\"one\"",
                &json!({"comment":"Thanks","sendResponse":true}),
            )
            .unwrap();
        assert!(matches!(
            client.write_connected("event", "stale", "decline", "W/\"one\"", &json!({"sendResponse":true})),
            Err(NetError::Http { status: 412, .. })
        ));
        thread.join().unwrap();
        assert_eq!(seen.try_iter().filter(|r| r.method == "POST").count(), 1);
    }
    #[test]
    fn lost_connected_write_response_is_uncertain_and_is_never_replayed() {
        let (base, thread, seen) = serve(1, |_| Vec::new());
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        let error = client
            .write_connected("contact", "c", "patch", "W/\"one\"", &json!({"companyName":"Studio"}))
            .unwrap_err();
        assert!(error.to_string().contains("Outcome uncertain"));
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 1);
        assert!(client.write_connected("event", "e", "delete", "W/\"one\"", &json!({})).is_err());
    }

    #[test]
    fn connected_collections_page_with_utc_and_discard_partial_results_on_permission_errors() {
        let (base, thread, seen) = serve(4, |request| {
            assert!(request.header("prefer").unwrap().contains("outlook.timezone=\"UTC\""));
            let root = format!("http://{}/v1.0/users/team%40example.net", request.header("host").unwrap());
            let path = &request.path;
            if path.contains("contacts") && path.contains("page=2") {
                response(403, "{\"error\":{\"code\":\"ErrorAccessDenied\"}}")
            } else if path.contains("page=2") {
                response(200, "{\"value\":[{\"id\":\"second\"}]}")
            } else {
                response(200,&json!({"value":[{"id":"first"}],"@odata.nextLink":format!("{root}/{}?page=2", if path.contains("contacts") {"contacts"}else{"calendars/cal/calendarView"})}).to_string())
            }
        });
        let client = GraphConn::new(
            &format!("{base}/v1.0"),
            "team@example.net",
            "fixture",
            ConnectOptions { allow_plaintext_auth: true, ..Default::default() },
        )
        .unwrap()
        .connect()
        .unwrap();
        assert_eq!(
            client.calendar_view("cal", "2026-10-01T00:00:00Z", "2026-11-01T00:00:00Z").unwrap().values.len(),
            2
        );
        assert!(matches!(client.contacts(None), Err(NetError::Http { status: 403, .. })));
        thread.join().unwrap();
        assert!(seen.try_iter().next().unwrap().path.contains("startDateTime=2026-10-01T00%3A00%3A00Z"));
    }

    #[test]
    fn uncertain_send_and_large_payload_are_not_silently_retried() {
        let (base, thread, _) = serve(1, |_| Vec::new());
        let options = ConnectOptions { allow_plaintext_auth: true, ..Default::default() };
        let client = GraphConn::new(&format!("{base}/v1.0"), "team@example.net", "fixture-token", options)
            .unwrap()
            .connect()
            .unwrap();
        let error = client.send_mime(b"Subject: Fixture\r\n\r\nBody").unwrap_err().to_string();
        assert!(error.contains("Delivery status unknown"));
        thread.join().unwrap();
        assert!(matches!(client.send_mime(&vec![b'x'; 3 * 1024 * 1024 + 1]), Err(NetError::TooLarge(_))));
    }

    #[test]
    fn local_fixture_exercises_paging_mime_mutations_and_send() {
        let (base, thread, requests) = serve(7, |request| {
            assert_eq!(request.header("authorization"), Some("Bearer fixture-token"));
            assert!(request.header("prefer").unwrap().contains("ImmutableId"));
            let root = format!("http://{}/v1.0/users/team%40example.net", request.header("host").unwrap());
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", path) if path.contains("skip=second") => response(200, &json!({"value":[{"id":"m2"}], "@odata.deltaLink":format!("{root}/mailFolders/inbox/messages/delta?delta=next")}).to_string()),
                ("GET", path) if path.contains("messages/delta") => response(200, &json!({"value":[{"id":"m1"}], "@odata.nextLink":format!("{root}/mailFolders/inbox/messages/delta?skip=second")}).to_string()),
                ("GET", path) if path.ends_with("/$value") => response(200, "Subject: Fixture\r\n\r\nSynthetic mail"),
                ("PATCH", _) => { let body:Value=serde_json::from_slice(&request.body).unwrap(); assert_eq!(body["isRead"],true); assert_eq!(body["flag"]["flagStatus"],"complete"); response(200,"{}") },
                ("POST", path) if path.ends_with("/move") => { let body:Value=serde_json::from_slice(&request.body).unwrap(); assert_eq!(body["destinationId"],"sent"); response(201,"{}") },
                ("POST", path) if path.ends_with("/sendMail") => { assert_eq!(request.header("content-type"),Some("text/plain")); use base64::Engine; let mime=base64::engine::general_purpose::STANDARD.decode(&request.body).unwrap(); assert!(String::from_utf8_lossy(&mime).contains("Bcc: hidden@example.net")); response(202,"") },
                ("DELETE", _) => response(204,""),
                _ => response(400,"{}"),
            }
        });
        let options = ConnectOptions { allow_plaintext_auth: true, ..Default::default() };
        let client = GraphConn::new(&format!("{base}/v1.0"), "team@example.net", "fixture-token", options)
            .unwrap()
            .connect()
            .unwrap();
        let page = client.messages_delta("inbox", None).unwrap();
        assert_eq!(page.values.len(), 2);
        assert!(page.delta.unwrap().contains("delta=next"));
        assert!(client.mime("m1").unwrap().starts_with(b"Subject: Fixture"));
        client.update("m1", true, "complete").unwrap();
        client.move_message("m1", "sent").unwrap();
        client
            .send_mime(
                b"From: team@example.net\r\nTo: shown@example.net\r\nBcc: hidden@example.net\r\n\r\nFixture",
            )
            .unwrap();
        client.delete("m2").unwrap();
        thread.join().unwrap();
        assert_eq!(requests.try_iter().count(), 7);
    }

    #[test]
    fn malicious_paging_and_permission_body_do_not_leak_token() {
        let (base, thread, _) = serve(1, |_| {
            response(200, &json!({"value":[],"@odata.nextLink":"https://attacker.example/steal"}).to_string())
        });
        let options = ConnectOptions { allow_plaintext_auth: true, ..Default::default() };
        let client =
            GraphConn::new(&format!("{base}/v1.0"), "team@example.net", "private-token", options.clone())
                .unwrap()
                .connect()
                .unwrap();
        assert!(matches!(client.messages_delta("inbox", None), Err(NetError::Invalid(_))));
        thread.join().unwrap();
        let (base, thread, _) = serve(1, |_| {
            response(403, r#"{"error":{"code":"ErrorAccessDenied","message":"private-token"}}"#)
        });
        let client = GraphConn::new(&format!("{base}/v1.0"), "team@example.net", "private-token", options)
            .unwrap()
            .connect()
            .unwrap();
        let error = client.well_known_folder("inbox").unwrap_err().to_string();
        assert!(error.contains("permission"));
        assert!(!error.contains("private-token"));
        thread.join().unwrap();
    }

    #[test]
    fn token_is_redacted_and_continuations_are_mailbox_scoped() {
        let conn =
            GraphConn::new(ENDPOINT, "team@example.net", "private-token", ConnectOptions::default()).unwrap();
        assert!(!format!("{conn:?}").contains("private-token"));
        let client = conn.connect().unwrap();
        assert!(
            client.checked_url("https://attacker.example/v1.0/users/team%40example.net/messages").is_err()
        );
        assert!(
            client
                .checked_url("https://graph.microsoft.com/v1.0/users/other%40example.net/messages")
                .is_err()
        );
        assert!(client.checked_url(&client.url("messages/delta?$skiptoken=abc")).is_ok());
        assert!(
            GraphConn::new("http://example.net/v1.0", "x@example.net", "token", ConnectOptions::default())
                .is_err()
        );
    }
}
