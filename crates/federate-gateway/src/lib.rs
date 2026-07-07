//! federate-gateway: the HTTP *compatibility adapter* for normal browsers.
//!
//! Federate's native addressing is `fed://domain/path` and its native
//! surface is the Federate protocol; a plain browser can speak neither. This
//! gateway is the bridge: it translates `Host: joao.pagina` + `/about` into
//! `fed://joao.pagina/about` (via `federate-uri`) and hands that to the SAME
//! resolution engine every native consumer uses. It never resolves names on
//! its own and cannot bypass signature/hash verification: there is no other
//! code path.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::Router;
use federate_resolution::{Resolved, Resolver};
use std::net::SocketAddr;
use std::sync::Arc;

pub fn router(resolver: Arc<Resolver>) -> Router {
    Router::new().fallback(handle).with_state(resolver)
}

/// Bind the gateway. Port 80 needs privileges on most systems; we return a
/// human explanation instead of a bare EACCES.
pub async fn serve(resolver: Arc<Resolver>, addr: SocketAddr) -> federate_core::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if addr.port() == 80 {
            eprintln!(
                "\nfederated could not bind to {addr}: {e}\n\n\
                 Port 80 is required so browsers can open portless URLs like http://home.fed\n\
                 Fixes:\n\
                 - Linux:   sudo setcap 'cap_net_bind_service=+ep' ./target/release/federated\n\
                            (or run via the provided systemd unit; see deploy/systemd/)\n\
                 - macOS:   pf redirect (no root needed):\n\
                            echo \"rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 80 -> 127.0.0.1 port 8787\" | sudo pfctl -ef -\n\
                            then: federated --gateway-addr 127.0.0.1:8787\n\
                            (do NOT run federated itself with sudo; it root-owns your data dir)\n\
                 - Windows: run the terminal as Administrator\n\
                 - Dev:     federated --gateway-addr 127.0.0.1:8787 (fallback port, not the main flow)\n\
                 See docs/en-US/port-80-setup.md\n"
            );
        }
        e
    })?;
    tracing::info!("gateway listening on http://{addr}");
    axum::serve(listener, router(resolver))
        .await
        .map_err(|e| federate_core::FederateError::Network(e.to_string()))?;
    Ok(())
}

/// Longest request path we will try to resolve. Manifest lookups are map
/// reads, but there is no reason to process megabyte-long URIs.
const MAX_PATH_LEN: usize = 2048;

async fn handle(
    State(resolver): State<Arc<Resolver>>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    // Content gateway is read-only: only GET/HEAD ever make sense.
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
            "method not allowed\n",
        )
            .into_response();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let path = uri.path();
    if path.len() > MAX_PATH_LEN {
        return (StatusCode::URI_TOO_LONG, "uri too long\n").into_response();
    }

    // Compatibility translation: HTTP request -> native Federate URI. From
    // here on this request is indistinguishable from a native fed:// fetch.
    let fed_uri = match federate_uri::FederateUri::from_http(host, path) {
        Ok(u) => u,
        Err(_) => return (StatusCode::NOT_FOUND, "404 not found\n").into_response(),
    };

    match resolver.resolve_uri(&fed_uri).await {
        Ok(Resolved::Content {
            bytes, mime, hash, ..
        }) => {
            // Content is addressed by its BLAKE3 hash, so the hash is a
            // perfect strong ETag: same hash = byte-identical content.
            let etag = format!("\"{hash}\"");
            if headers
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|inm| etag_matches(inm, &etag))
            {
                return Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .header(header::ETAG, etag)
                    .header(header::CACHE_CONTROL, "public, max-age=60")
                    .body(Body::empty())
                    .unwrap();
            }
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                // Serve the declared MIME verbatim; never let a browser sniff a
                // block into an executable type it wasn't published as.
                .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                .header(header::REFERRER_POLICY, "no-referrer")
                // Content is verified per manifest version; short TTL keeps
                // browsers fresh after a site republish, and the ETag turns
                // revalidations into 304s.
                .header(header::CACHE_CONTROL, "public, max-age=60")
                .header(header::ETAG, etag)
                .header("x-federate-gateway", "federated")
                .body(Body::from(bytes))
                .unwrap()
        }
        Ok(Resolved::NotFederate { .. }) => {
            (StatusCode::NOT_FOUND, "404 not found\n").into_response()
        }
        Ok(Resolved::TldNotFound { tld }) => styled_error(
            StatusCode::NOT_FOUND,
            "TLD not found in Federate Network",
            &format!(
                "<code>.{}</code> does not exist in the Federate root registry.",
                esc(&tld)
            ),
        ),
        Ok(Resolved::TldUnavailable { tld, status }) => styled_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "TLD not resolvable",
            &format!(
                "<code>.{}</code> exists but is currently <strong>{}</strong> and cannot be resolved.",
                esc(&tld),
                esc(&status)
            ),
        ),
        Ok(Resolved::DelegatedUnavailable {
            domain,
            tld,
            reason,
        }) => styled_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Delegated registry unavailable",
            &format!(
                "<code>.{}</code> is delegated to an operator, but its registry cannot be \
                 used right now, so <code>{}</code> cannot be resolved:<br><code>{}</code>",
                esc(&tld),
                esc(&domain),
                esc(&reason)
            ),
        ),
        Ok(Resolved::DomainNotFound { domain }) => styled_error(
            StatusCode::NOT_FOUND,
            "Domain not found in Federate Network",
            &format!(
                "<code>{}</code> is a valid Federate name, but no record exists for it in its TLD registry yet.",
                esc(&domain)
            ),
        ),
        Ok(Resolved::DomainUnavailable { domain, status }) => styled_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Domain not resolvable",
            &format!(
                "<code>{}</code> exists but is currently <strong>{}</strong>.",
                esc(&domain),
                esc(&status)
            ),
        ),
        Ok(Resolved::PathNotFound { domain, path }) => styled_error(
            StatusCode::NOT_FOUND,
            "404 - page not found",
            &format!(
                "<code>{}</code> has no content at <code>{}</code>.",
                esc(&domain),
                esc(&path)
            ),
        ),
        Ok(Resolved::SecurityFailure { domain, layer, reason }) => styled_error(
            StatusCode::BAD_GATEWAY,
            "Federate security verification failed",
            &format!(
                "Verification failed at the <strong>{}</strong> layer while resolving \
                 <code>{}</code>:<br><code>{}</code><br><br>\
                 The content was <strong>not served</strong>. This can mean tampering, a \
                 corrupted cache, or a misconfigured trust anchor. \
                 Run <code>federate doctor</code> and check your pinned root key.",
                esc(&layer),
                esc(&domain),
                esc(&reason)
            ),
        ),
        Err(e) => styled_error(
            StatusCode::BAD_GATEWAY,
            "Federate resolution error",
            &format!(
                "<code>{}</code><br>Node 1 may be unreachable and this site is not cached yet. Try <code>federate doctor</code>.",
                esc(&e.to_string())
            ),
        ),
    }
}

/// Does an If-None-Match header value match our ETag? Handles the `*`
/// wildcard, comma-separated lists, and weak validators (`W/"..."`): weak
/// comparison is fine here because equal hashes mean byte-identical content.
fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match.split(',').any(|t| {
        let t = t.trim();
        t == "*" || t == etag || t.strip_prefix("W/") == Some(etag)
    })
}

/// HTML-escape untrusted values (request paths, upstream errors) before
/// interpolating them into error pages.
fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#x27;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// The official Federate font, embedded as a data URI so error pages are
/// fully self-contained: an error page renders precisely when the failing
/// host CANNOT serve assets, so a relative font URL would never load and
/// the page would silently fall back to a system serif.
const AVERIA_400_B64: &str = include_str!("../assets/averia-serif-libre-400.woff2.b64");

fn styled_error(status: StatusCode, title: &str, detail: &str) -> Response {
    (status, Html(error_html(title, detail))).into_response()
}

fn error_html(title: &str, detail: &str) -> String {
    let font = AVERIA_400_B64.trim();
    format!(
        r##"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title>
<style>
@font-face{{font-family:'Averia Serif Libre';font-style:normal;font-weight:400;font-display:swap;src:url(data:font/woff2;base64,{font}) format('woff2')}}
:root{{--bg:#FBF6F0;--fg:#21211F;--surface:#E8DEC5;--muted:#544329}}
body{{font-family:'Averia Serif Libre',Georgia,serif;background:var(--bg);color:var(--fg);display:grid;place-items:center;min-height:100vh;margin:0}}
main{{max-width:38rem;padding:2rem;text-align:center}}
h1{{font-weight:normal}}code{{background:var(--surface);padding:.15em .4em;border-radius:4px}}
.mark{{font-size:2.5rem}}p.f{{color:var(--muted);font-size:.85rem;margin-top:3rem}}
</style></head><body><main>
<div class="mark"><svg viewBox="181 0 80 80" width="56" height="56" aria-label="Federate Network"><path d="M224.761 21.0424C224.48 25.8256 224.339 28.2172 225.654 28.7618C226.968 29.3065 228.56 27.5161 231.744 23.9353L242.932 11.3517C244.253 9.86503 244.914 9.12171 245.777 9.0964C246.639 9.0711 247.343 9.77441 248.749 11.181L249.82 12.2521C251.227 13.6589 251.931 14.3623 251.905 15.225C251.88 16.0878 251.136 16.7486 249.649 18.0703L237.066 29.2544C233.484 32.4378 231.693 34.0296 232.237 35.3444C232.782 36.6592 235.174 36.5186 239.958 36.2373L256.765 35.249C258.751 35.1322 259.744 35.0738 260.372 35.6659C261 36.258 261 37.2527 261 39.2421V40.7576C261 42.7471 261 43.7419 260.372 44.334C259.744 44.9261 258.751 44.8676 256.765 44.7507L239.957 43.7614C235.173 43.4799 232.782 43.3391 232.237 44.6538C231.692 45.9686 233.483 47.5604 237.064 50.7441L249.649 61.9312C251.136 63.2531 251.88 63.9141 251.905 64.7768C251.93 65.6396 251.227 66.3429 249.82 67.7496L248.748 68.8214C247.341 70.2278 246.638 70.931 245.775 70.9056C244.912 70.8802 244.252 70.1368 242.93 68.65L231.744 56.0654C228.561 52.4836 226.969 50.6927 225.654 51.2373C224.339 51.7819 224.48 54.1739 224.761 58.9578L225.751 75.765C225.868 77.751 225.926 78.7441 225.334 79.372C224.742 80 223.747 80 221.758 80H220.242C218.253 80 217.258 80 216.666 79.3721C216.074 78.7442 216.132 77.7512 216.249 75.7652L217.237 58.9576C217.519 54.1737 217.659 51.7818 216.344 51.2372C215.03 50.6927 213.438 52.4837 210.254 56.0656L199.07 68.6493C197.749 70.1364 197.088 70.8799 196.225 70.9053C195.362 70.9307 194.659 70.2272 193.252 68.8204L192.181 67.7494C190.774 66.3428 190.071 65.6395 190.096 64.7768C190.122 63.9142 190.865 63.2533 192.352 61.9316L204.935 50.7438C208.516 47.5602 210.307 45.9684 209.762 44.6537C209.217 43.3391 206.826 43.4798 202.042 43.7614L185.235 44.7507C183.249 44.8676 182.256 44.9261 181.628 44.334C181 43.7419 181 42.7471 181 40.7576V39.2421C181 37.2527 181 36.258 181.628 35.6659C182.256 35.0738 183.249 35.1322 185.235 35.249L202.041 36.2373C206.825 36.5186 209.217 36.6592 209.761 35.3444C210.306 34.0297 208.515 32.4379 204.933 29.2545L192.351 18.0707C190.864 16.7488 190.12 16.0879 190.095 15.2251C190.07 14.3623 190.773 13.6589 192.18 12.2522L193.252 11.1805C194.659 9.7741 195.362 9.07092 196.225 9.09633C197.088 9.12173 197.748 9.86508 199.07 11.3518L210.255 23.9342C213.438 27.5154 215.03 29.3061 216.344 28.7615C217.659 28.2169 217.519 25.8252 217.237 21.0418L216.249 4.2348C216.132 2.24883 216.074 1.25584 216.666 0.627922C217.258 0 218.253 0 220.242 0H221.758C223.747 0 224.742 0 225.334 0.627966C225.926 1.25593 225.868 2.24897 225.751 4.23504L224.761 21.0424Z" fill="#406A66"/></svg></div><h1>{title}</h1><p>{detail}</p>
<p class="f">Federate Network: a human web, built by people.</p>
</main></body></html>"##
    )
}

#[cfg(test)]
mod tests {
    use super::{esc, etag_matches, handle};
    use axum::extract::State;
    use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
    use federate_identity::NodeIdentity;
    use federate_resolution::Resolver;
    use federate_root::{RootCache, RootZone, TldRecord, SIGNATURE_ALGORITHM};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    /// Offline resolver over a signed multi-TLD zone (no domain records:
    /// the record layer answers 404, which proves the full generic chain
    /// root -> TLD -> domain lookup ran for every TLD).
    fn offline_resolver(tlds: &[&str]) -> Arc<Resolver> {
        let dir = std::env::temp_dir().join(format!(
            "fed-gw-adapter-{}-{}",
            std::process::id(),
            tlds.len()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let root = NodeIdentity::load_or_create(&dir.join("root")).unwrap();
        let mut zone_tlds = BTreeMap::new();
        for tld in tlds {
            let mut rec = TldRecord {
                tld: tld.to_string(),
                status: federate_naming::TldStatus::Official,
                mode: federate_naming::TldMode::Official,
                owner_public_key: root.node_id(),
                operator_public_key: root.node_id(),
                operator_name: "test".into(),
                registry_type: federate_naming::RegistryType::RootManaged,
                registry_endpoint: None,
                registry_manifest_hash: None,
                registry_providers: Vec::new(),
                policy_hash: None,
                pricing: None,
                created_at: "t".into(),
                updated_at: "t".into(),
                expires_at: None,
                notes: None,
                signature_algorithm: SIGNATURE_ALGORITHM.into(),
                signature: None,
            };
            rec.signature = Some(root.sign(&rec.signable_bytes().unwrap()));
            zone_tlds.insert(tld.to_string(), rec);
        }
        let mut zone = RootZone {
            network: "federate".into(),
            root_version: 1,
            generated_at: "t".into(),
            root_public_key: root.node_id(),
            tlds: zone_tlds,
            domains: BTreeMap::new(),
            audit: vec![],
            signature_algorithm: SIGNATURE_ALGORITHM.into(),
            signature: None,
        };
        zone.signature = Some(root.sign(&zone.signable_bytes().unwrap()));
        RootCache::new(&dir).store(&zone).unwrap();
        Arc::new(
            Resolver::new(
                federate_client::NodeClient::new("http://127.0.0.1:1"),
                &dir,
                Some(root.node_id()),
            )
            .unwrap(),
        )
    }

    async fn get(resolver: Arc<Resolver>, host: &str, path: &str) -> StatusCode {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        let uri: Uri = path.parse().unwrap();
        handle(State(resolver), Method::GET, headers, uri)
            .await
            .status()
    }

    /// HTTP requests for ANY official TLD go through the same native path:
    /// each answers 404 domain-not-found (the generic chain ran), never a
    /// TLD error, never a special case for one hardcoded domain.
    #[tokio::test]
    async fn http_adapter_serves_every_tld_through_native_path() {
        let tlds = ["fed", "pagina", "rosa", "cara", "mosca", "busca"];
        let resolver = offline_resolver(&tlds);
        for tld in tlds {
            let status = get(resolver.clone(), &format!("site.{tld}"), "/").await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "site.{tld}: expected domain-not-found through the generic path"
            );
        }
        // Unknown TLD: structured 404 via the same engine.
        assert_eq!(
            get(resolver.clone(), "x.nowhere", "/").await,
            StatusCode::NOT_FOUND
        );
        // Non-Federate host: rejected at URI translation, no resolution.
        assert_eq!(
            get(resolver.clone(), "not_a_host!!", "/").await,
            StatusCode::NOT_FOUND
        );
        // Write methods have no place on a content gateway.
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("site.fed"));
        let resp = handle(State(resolver), Method::POST, headers, "/".parse().unwrap()).await;
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn error_pages_embed_the_official_font() {
        // Error pages render exactly when the failing host cannot serve
        // assets, so the font must travel inside the page itself: a data
        // URI, never a relative asset URL that could not resolve.
        let html = super::error_html("TLD not found in Federate Network", "test detail");
        assert!(
            html.contains("data:font/woff2;base64,d09G"),
            "woff2 magic in base64"
        );
        assert!(!html.contains("url('/fonts/"), "no relative font URLs");
        assert!(html.contains("'Averia Serif Libre'"));
    }

    #[test]
    fn etag_matching_rules() {
        let etag = "\"abc123\"";
        assert!(etag_matches("\"abc123\"", etag));
        assert!(etag_matches("*", etag));
        assert!(etag_matches("\"zzz\", \"abc123\"", etag));
        assert!(etag_matches("W/\"abc123\"", etag));
        assert!(!etag_matches("\"other\"", etag));
        assert!(!etag_matches("abc123", etag)); // unquoted is not a valid match
        assert!(!etag_matches("", etag));
    }

    #[test]
    fn esc_neutralizes_xss_payloads() {
        let out = esc(r#"<script>alert('xss')</script>"#);
        assert!(!out.contains('<') && !out.contains('>'));
        assert!(!out.contains('\''));
        assert_eq!(
            esc(r#"<img src=x onerror="alert(1)">"#),
            "&lt;img src=x onerror=&quot;alert(1)&quot;&gt;"
        );
        // ampersand escaped first so no double-escaping surprises
        assert_eq!(esc("a&b"), "a&amp;b");
    }
}
