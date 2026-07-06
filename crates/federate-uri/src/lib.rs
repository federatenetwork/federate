//! federate-uri: the native Federate addressing format.
//!
//! `fed://<label>.<tld>[/path][?query]`
//!
//! This is the canonical way to name anything on the Federate Network.
//! `http://home.fed` in a normal browser is a *compatibility* spelling; the
//! gateway translates it to `fed://home.fed` and resolves that. Every
//! consumer (native client, gateway, CLI, future browser) parses addresses
//! through this crate so the rules exist exactly once.
//!
//! Grammar (deliberately small):
//! - scheme is exactly `fed`
//! - authority is exactly one label + one TLD (`home.fed`); validity of the
//!   *name syntax* is decided by `federate-naming`, existence by the signed
//!   root zone at resolution time
//! - no port, no userinfo, no IP literals: Federate names never resolve to
//!   transport addresses in the URI itself
//! - optional absolute path (defaults to `/`)
//! - optional query string (kept verbatim; meaning is up to the site/app)
//! - optional fragment is accepted and discarded (client-side concern)

use federate_core::{FederateError, Result};
use federate_naming::FederateDomain;
use serde::{Deserialize, Serialize};

pub const SCHEME: &str = "fed";

/// A parsed, validated Federate URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederateUri {
    pub domain: FederateDomain,
    /// Absolute path, always starting with `/`.
    pub path: String,
    /// Raw query string without the leading `?`.
    pub query: Option<String>,
}

impl FederateUri {
    /// Parse a `fed://...` string.
    pub fn parse(input: &str) -> Result<Self> {
        let s = input.trim();
        let rest = s
            .strip_prefix("fed://")
            .ok_or_else(|| bad(s, "must start with fed://"))?;
        if rest.is_empty() {
            return Err(bad(s, "missing domain"));
        }
        // Split authority from path/query/fragment.
        let (authority, tail) = match rest.find(['/', '?', '#']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        if authority.contains('@') {
            return Err(bad(s, "userinfo is not allowed in Federate URIs"));
        }
        if authority.contains(':') {
            return Err(bad(
                s,
                "ports are not allowed; Federate names never carry transport addresses",
            ));
        }
        let domain = FederateDomain::parse(authority)?;

        // Strip fragment first (client-side only), then split query.
        let tail = tail.split('#').next().unwrap_or("");
        let (path, query) = match tail.split_once('?') {
            Some((p, q)) => (p, (!q.is_empty()).then(|| q.to_string())),
            None => (tail, None),
        };
        let path = if path.is_empty() { "/" } else { path };
        if !path.starts_with('/') {
            return Err(bad(s, "path must be absolute"));
        }
        if path.len() > MAX_PATH_LEN {
            return Err(bad(s, "path too long"));
        }
        Ok(Self {
            domain,
            path: path.to_string(),
            query,
        })
    }

    /// Build a Federate URI from a compatibility HTTP request: Host header +
    /// path-and-query. This is the gateway's translation step; after it, the
    /// HTTP request and a native `fed://` request are indistinguishable.
    pub fn from_http(host: &str, path_and_query: &str) -> Result<Self> {
        let domain = FederateDomain::parse(host)?;
        let (path, query) = match path_and_query.split_once('?') {
            Some((p, q)) => (p, (!q.is_empty()).then(|| q.to_string())),
            None => (path_and_query, None),
        };
        let path = if path.is_empty() { "/" } else { path };
        if !path.starts_with('/') {
            return Err(bad(path_and_query, "path must be absolute"));
        }
        Ok(Self {
            domain,
            path: path.to_string(),
            query,
        })
    }

    pub fn fqdn(&self) -> String {
        self.domain.fqdn()
    }
}

/// Longest path we accept in a URI (matches the gateway's request cap).
pub const MAX_PATH_LEN: usize = 2048;

fn bad(input: &str, reason: &str) -> FederateError {
    FederateError::NotFederateDomain(format!("invalid Federate URI '{input}': {reason}"))
}

impl std::fmt::Display for FederateUri {
    /// Canonical form: root path is omitted (`fed://home.fed`), any other
    /// path is printed, query follows when present.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fed://{}", self.domain.fqdn())?;
        if self.path != "/" {
            write!(f, "{}", self.path)?;
        }
        if let Some(q) = &self.query {
            write!(f, "?{q}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for FederateUri {
    type Err = FederateError;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_official_tld_generically() {
        // No TLD is special-cased: any syntactically valid label + TLD parses.
        for (tld, _) in federate_naming::FEDERATE_TLDS {
            let uri = FederateUri::parse(&format!("fed://site.{tld}")).unwrap();
            assert_eq!(uri.domain.tld, *tld);
            assert_eq!(uri.path, "/");
        }
        // Unknown-but-valid TLDs parse too; existence is the root zone's call.
        assert!(FederateUri::parse("fed://store.femboy").is_ok());
    }

    #[test]
    fn parses_paths_and_queries() {
        let u = FederateUri::parse("fed://joao.pagina/about").unwrap();
        assert_eq!(u.fqdn(), "joao.pagina");
        assert_eq!(u.path, "/about");
        assert_eq!(u.query, None);

        let u = FederateUri::parse("fed://fed.busca/?q=manifesto").unwrap();
        assert_eq!(u.path, "/");
        assert_eq!(u.query.as_deref(), Some("q=manifesto"));

        let u = FederateUri::parse("fed://arcade.mosca/play?level=2#top").unwrap();
        assert_eq!(u.path, "/play");
        assert_eq!(u.query.as_deref(), Some("level=2"));
    }

    #[test]
    fn canonical_display_roundtrip() {
        for s in [
            "fed://home.fed",
            "fed://joao.pagina/about",
            "fed://fed.busca?q=manifesto",
            "fed://fotolia.rosa/galeria/2026",
        ] {
            let u = FederateUri::parse(s).unwrap();
            assert_eq!(u.to_string(), s);
            assert_eq!(FederateUri::parse(&u.to_string()).unwrap(), u);
        }
        // Root path normalizes away.
        assert_eq!(
            FederateUri::parse("fed://home.fed/").unwrap().to_string(),
            "fed://home.fed"
        );
    }

    #[test]
    fn rejects_invalid_uris() {
        for bad in [
            "http://home.fed",     // wrong scheme
            "fed://",              // no domain
            "fed://fed",           // no label
            "fed://a.b.fed",       // subdomains not supported yet
            "fed://home.fed:8080", // ports forbidden
            "fed://user@home.fed", // userinfo forbidden
            "fed://home.f_d",      // invalid tld chars
            "fed://-x.fed",        // invalid label
            "fed:/home.fed",       // malformed scheme separator
            "home.fed",            // missing scheme
            "fed://home.fed\u{0}", // control garbage in tld
        ] {
            assert!(FederateUri::parse(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn http_compat_maps_to_same_uri() {
        // The gateway translation: Host + path must equal the native URI.
        let via_http = FederateUri::from_http("joao.pagina:80", "/about?x=1").unwrap();
        let native = FederateUri::parse("fed://joao.pagina/about?x=1").unwrap();
        assert_eq!(via_http, native);

        let root = FederateUri::from_http("home.fed", "/").unwrap();
        assert_eq!(root.to_string(), "fed://home.fed");

        assert!(FederateUri::from_http("not_a_host!!", "/").is_err());
    }
}
