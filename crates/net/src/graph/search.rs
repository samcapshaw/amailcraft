//! Explicit, bounded Graph message search. The query is sent only on request.
use super::*;

impl GraphClient {
    /// One page of Microsoft 365 search results. `query` uses Graph's mail-search syntax.
    pub fn search_messages(
        &self,
        query: &str,
        folder: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<GraphPage> {
        if query.trim().is_empty() || query.len() > 2000 || query.chars().any(char::is_control) {
            return Err(NetError::Invalid("Use a nonempty server search of at most 2000 bytes".into()));
        }
        let path = match folder {
            Some(id) if !id.is_empty() && id.len() <= 4096 => {
                format!("mailFolders/{}/messages", percent_encode(id))
            }
            Some(_) => return Err(NetError::Invalid("Invalid server search folder".into())),
            None => "messages".into(),
        };
        let quoted = format!("\"{}\"", query.replace('\\', "\\\\").replace('"', "\\\""));
        let fields = "id,parentFolderId,subject,from,toRecipients,ccRecipients,replyTo,internetMessageId,bodyPreview,receivedDateTime,isRead,isDraft,flag,hasAttachments";
        let url = cursor.map(str::to_owned).unwrap_or_else(|| {
            self.url(&format!("{path}?$search={}&$select={fields}&$top=50", percent_encode(&quoted)))
        });
        let check = |url: &str| -> Result<()> {
            self.checked_url(url)?;
            let actual = Url::parse(url)?;
            let expected = Url::parse(&self.url(&path))?;
            if percent_decode(actual.path.split('?').next().unwrap_or_default(), false)
                != percent_decode(&expected.path, false)
            {
                return Err(NetError::Invalid("Search continuation escaped its folder".into()));
            }
            Ok(())
        };
        check(&url)?;
        let mut bounded = Self::new(self.conn.clone())?;
        bounded.http.max_body = 2 * 1024 * 1024;
        let bytes = bounded.request("GET", &url, "application/json", None)?;
        let doc: Value = serde_json::from_slice(&bytes)
            .map_err(|_| NetError::Protocol("Invalid server search response".into()))?;
        let values = doc
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| NetError::Protocol("Search response has no messages".into()))?;
        if values.len() > 200 {
            return Err(NetError::TooLarge("Server search page".into()));
        }
        let next = match doc.get("@odata.nextLink") {
            None => None,
            Some(value) => {
                let next =
                    value.as_str().ok_or_else(|| NetError::Protocol("Invalid search continuation".into()))?;
                check(next)?;
                Some(next.to_owned())
            }
        };
        Ok(GraphPage { values: values.clone(), next, bytes_received: bytes.len(), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testserver::{json as response, serve};

    #[test]
    fn server_search_targets_shared_mailbox_and_checks_continuations() {
        let (base, thread, seen) = serve(1, |req| {
            let path = percent_decode(&req.path, false);
            assert!(path.starts_with("/v1.0/users/team@example.net/mailFolders/inbox/messages?"));
            assert!(path.contains("$search=\"from:maya@example.net\""));
            assert!(!path.contains("$orderby") && !path.contains("$filter"));
            response(200, r#"{"value":[{"id":"one","hasAttachments":true}]}"#)
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
        let page = client.search_messages("from:maya@example.net", Some("inbox"), None).unwrap();
        assert_eq!(page.values.len(), 1);
        assert!(
            client
                .search_messages(
                    "test",
                    Some("inbox"),
                    Some(&format!("{base}/v1.0/users/owner%40example.net/messages"))
                )
                .is_err()
        );
        assert!(
            client
                .search_messages(
                    "test",
                    Some("inbox"),
                    Some(&format!("{base}/v1.0/users/team%40example.net/mailFolders/other/messages"))
                )
                .is_err()
        );
        assert!(client.search_messages("", None, None).is_err());
        thread.join().unwrap();
        assert_eq!(seen.try_iter().count(), 1);
    }
}
