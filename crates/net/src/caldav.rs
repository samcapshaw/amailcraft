//! Read-only CalDAV discovery (RFC 4791 and RFC 5397). Credentials never cross origins.
use crate::{
    NetError, Result,
    autoconfig::{Element, parse_xml},
    http::{HttpClient, Url},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::time::Duration;

const PROPS: &str = r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:current-user-principal/><c:calendar-home-set/><d:displayname/><d:resourcetype/><c:supported-calendar-component-set/></d:prop></d:propfind>"#;
const MAX_CALENDARS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Calendar {
    pub url: String,
    pub name: String,
    /// Empty means the server did not advertise component support.
    pub components: Vec<String>,
}

fn invalid(message: &str) -> NetError {
    NetError::Invalid(message.into())
}

/// Validates before any connection or credential use. No userinfo, query, fragment or plaintext.
pub fn validate(endpoint: &str, username: &str, password: &str) -> Result<()> {
    endpoint_url(endpoint, false)?;
    if username.is_empty()
        || username.len() > 1024
        || username.contains(':')
        || username.chars().any(char::is_control)
    {
        return Err(invalid("Enter a valid CalDAV username."));
    }
    if password.is_empty() || password.len() > 16 * 1024 || password.chars().any(char::is_control) {
        return Err(invalid("Enter a valid CalDAV app password."));
    }
    Ok(())
}
fn endpoint_url(endpoint: &str, fixture: bool) -> Result<Url> {
    if endpoint.len() > 8192 || endpoint.contains(['?', '#', '\\']) || endpoint.chars().any(char::is_control)
    {
        return Err(invalid("Enter an HTTPS CalDAV endpoint without a query or fragment."));
    }
    let url = Url::parse(endpoint).map_err(|_| invalid("Enter a valid HTTPS CalDAV endpoint."))?;
    if !url.https && !(fixture && url.host == "127.0.0.1") {
        return Err(invalid("CalDAV discovery requires HTTPS."));
    }
    Ok(url)
}
fn href(base: &Url, value: &str, fixture: bool) -> Result<Url> {
    if value.len() > 8192 || value.contains(['?', '#', '\\']) || value.chars().any(char::is_control) {
        return Err(invalid("The server returned an invalid calendar address."));
    }
    let url = base.join(value)?;
    endpoint_url(&url.to_string(), fixture)?;
    if !base.same_origin(&url) {
        return Err(invalid(
            "The server returned a calendar on another host. Use that host's CalDAV endpoint explicitly; credentials were not forwarded.",
        ));
    }
    Ok(url)
}
fn successful_props(response: &Element) -> impl Iterator<Item = &Element> {
    response
        .all("propstat")
        .filter(|p| p.child_text("status").and_then(|s| s.split_whitespace().nth(1)) == Some("200"))
        .filter_map(|p| p.child("prop"))
}
fn propfind(client: &HttpClient, url: &Url, auth: &str, depth: &str) -> Result<Element> {
    let r = client.request(
        "PROPFIND",
        &url.to_string(),
        &[("Authorization", auth), ("Depth", depth), ("Content-Type", "application/xml; charset=utf-8")],
        Some(PROPS.as_bytes()),
    )?;
    match r.status {
        207 => {}
        401 | 403 => {
            return Err(invalid("CalDAV sign-in was rejected. Check the username and app password."));
        }
        301 | 302 | 303 | 307 | 308 => {
            return Err(invalid(
                "The endpoint redirects. Enter the provider's final CalDAV endpoint; credentials were not forwarded.",
            ));
        }
        _ => return Err(NetError::Protocol(format!("CalDAV discovery returned HTTP {}.", r.status))),
    }
    let body = std::str::from_utf8(&r.body).map_err(|_| invalid("CalDAV returned non-UTF-8 XML."))?;
    let root = parse_xml(body)?;
    if root.name != "multistatus" {
        return Err(invalid("CalDAV returned an invalid discovery document."));
    }
    Ok(root)
}
fn property_href(root: &Element, name: &str) -> Option<String> {
    root.all("response")
        .flat_map(successful_props)
        .find_map(|p| p.child(name).and_then(|e| e.child_text("href")))
        .map(str::to_owned)
}

/// Three-stage discovery. No writes, redirects, secret persistence or remote event downloads.
pub fn discover(endpoint: &str, username: &str, password: &str) -> Result<Vec<Calendar>> {
    validate(endpoint, username, password)?;
    discover_inner(endpoint, username, password, false)
}
fn discover_inner(endpoint: &str, username: &str, password: &str, fixture: bool) -> Result<Vec<Calendar>> {
    let base = endpoint_url(endpoint, fixture)?;
    let client = HttpClient {
        max_body: 1024 * 1024,
        max_redirects: 0,
        options: crate::ConnectOptions {
            allow_plaintext_auth: fixture,
            connect_timeout: Duration::from_secs(10),
            io_timeout: Duration::from_secs(15),
            ..Default::default()
        },
        ..Default::default()
    };
    let auth = format!("Basic {}", STANDARD.encode(format!("{username}:{password}")));
    let initial = propfind(&client, &base, &auth, "0")?;
    let home = if let Some(home) = property_href(&initial, "calendar-home-set") {
        href(&base, &home, fixture)?
    } else if let Some(principal) = property_href(&initial, "current-user-principal") {
        let principal = href(&base, &principal, fixture)?;
        let props = propfind(&client, &principal, &auth, "0")?;
        let home = property_href(&props, "calendar-home-set")
            .ok_or_else(|| invalid("No calendar home was advertised for this account."))?;
        href(&principal, &home, fixture)?
    } else {
        // Explicit calendar-home endpoints may omit principal properties.
        base.clone()
    };
    let listing = propfind(&client, &home, &auth, "1")?;
    let mut calendars = Vec::new();
    for response in listing.all("response") {
        for prop in successful_props(response) {
            if !prop.child("resourcetype").is_some_and(|r| r.child("calendar").is_some()) {
                continue;
            }
            let Some(address) = response.child_text("href") else {
                continue;
            };
            let url = href(&home, address, fixture)?.to_string();
            if calendars.iter().any(|c: &Calendar| c.url == url) {
                continue;
            }
            if calendars.len() >= MAX_CALENDARS {
                return Err(invalid("The server advertised too many calendars."));
            }
            let name = prop
                .child_text("displayname")
                .filter(|s| !s.is_empty())
                .unwrap_or("Calendar")
                .chars()
                .take(256)
                .collect();
            let components = prop
                .child("supported-calendar-component-set")
                .map(|set| {
                    set.all("comp")
                        .filter_map(|c| c.attr("name"))
                        .take(16)
                        .map(|v| v.chars().take(32).collect())
                        .collect()
                })
                .unwrap_or_default();
            calendars.push(Calendar { url, name, components });
        }
    }
    Ok(calendars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testserver::{json, serve};
    fn multi(props: &str, path: &str) -> String {
        format!(
            r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:response><d:href>{path}</d:href><d:propstat><d:prop>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#
        )
    }
    #[test]
    fn discovers_principal_home_and_calendar_with_only_propfind() {
        let (url, thread, requests) = serve(3, |r| {
            assert_eq!(r.method, "PROPFIND");
            assert!(r.header("Authorization").is_some());
            let props = match r.path.as_str() {
                "/" => "<d:current-user-principal><d:href>/principal/</d:href></d:current-user-principal>",
                "/principal/" => "<c:calendar-home-set><d:href>/home/</d:href></c:calendar-home-set>",
                _ => {
                    "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Team &amp; plans</d:displayname><c:supported-calendar-component-set><c:comp name='VEVENT'/></c:supported-calendar-component-set>"
                }
            };
            json(207, &multi(props, "/home/team/"))
        });
        let result = discover_inner(&url, "person@example.net", "synthetic", true).unwrap();
        thread.join().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.first().unwrap().name, "Team & plans");
        assert_eq!(result.first().unwrap().components, vec!["VEVENT"]);
        assert_eq!(
            requests.try_iter().map(|r| r.header("Depth").unwrap().to_owned()).collect::<Vec<_>>(),
            vec!["0", "0", "1"]
        );
    }
    #[test]
    fn rejects_cross_origin_discovery_before_a_second_request() {
        let (url, thread, _) = serve(1, |_| {
            json(
                207,
                &multi(
                    "<d:current-user-principal><d:href>https://other.example.net/principal/</d:href></d:current-user-principal>",
                    "/",
                ),
            )
        });
        assert!(discover_inner(&url, "person@example.net", "synthetic", true).is_err());
        thread.join().unwrap();
    }
    #[test]
    fn rejects_redirects_and_auth_errors_without_another_request() {
        for status in [302, 401, 403] {
            let (url, thread, requests) = serve(1, move |_| {
                format!("HTTP/1.1 {status} Test\r\nLocation: https://other.example.net/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
            });
            assert!(discover_inner(&url, "person@example.net", "synthetic", true).is_err());
            thread.join().unwrap();
            assert_eq!(requests.try_iter().count(), 1);
        }
    }
    #[test]
    fn refuses_bad_endpoints_and_credentials_before_network() {
        for url in [
            "http://example.net",
            "https://u:p@example.net",
            "https://example.net/?token=secret",
            "https://example.net/#x",
        ] {
            assert!(validate(url, "person", "synthetic").is_err());
        }
        assert!(validate("https://calendar.example.net", "bad:user", "synthetic").is_err());
        assert!(validate("https://calendar.example.net", "person", "").is_err());
    }
    #[test]
    fn failed_properties_are_not_used() {
        let root = parse_xml("<multistatus><response><propstat><prop><calendar-home-set><href>/bad</href></calendar-home-set></prop><status>HTTP/1.1 403 Forbidden</status></propstat></response></multistatus>").unwrap();
        assert!(property_href(&root, "calendar-home-set").is_none());
    }
}
