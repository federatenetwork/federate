//! federate-naming: domain/TLD parsing, naming rules, record types and
//! statuses for the Federate TLD hierarchy:
//!
//! Federate Root Registry → TLD Operator → Domain Registrant → Site/Manifest Owner

use federate_core::{FederateError, Result};
use serde::{Deserialize, Serialize};

/// Default official Federate TLDs (root-managed) and their purposes.
/// The authoritative TLD set lives in the signed root zone; this constant
/// seeds it and documents the defaults.
pub const FEDERATE_TLDS: &[(&str, &str)] = &[
    // Core namespaces
    ("fed", "Official Federate namespace: specs, protocol docs, registry, status, root info, governance, official tools."),
    ("busca", "Federate search and discovery services (e.g. fed.busca). Official, root-managed."),
    // People and communities
    ("pagina", "Personal sites, blogs, portfolios, small homepages, essays, public profiles (Portuguese namespace)."),
    ("pages", "Personal sites, blogs, portfolios, small homepages, essays, public profiles (English namespace)."),
    ("cara", "Identity, profiles, personal presence, people pages, creator pages, public cards."),
    ("comu", "Communities, groups, collectives, clubs, shared spaces."),
    ("oi", "Casual hellos, lightweight personal pages, contact cards."),
    ("weblog", "Blogs, journals, diaries, running logs of any kind."),
    // Creative
    ("rosa", "Creative, visual, poetic, aesthetic, soft, personal, art-oriented spaces."),
    ("mosca", "Weird internet, experiments, memes, small games, odd projects, underground pages, strange communities."),
    ("tipos", "Typography, type design, lettering, fonts, and written-form craft (Portuguese namespace)."),
    ("types", "Typography, type design, lettering, fonts, and written-form craft (English namespace)."),
    // Media
    ("foto", "Photography, photo essays, galleries (Portuguese namespace)."),
    ("pic", "Images, illustration, visual snippets, galleries (English namespace)."),
    ("vid", "Video pages, channels, screening rooms."),
    ("sound", "Audio, music, sound art, radio, podcasts."),
    ("records", "Music labels, discographies, archives, collections."),
    // Colors: thematic creative/personal spaces
    ("amarelo", "Color namespace (yellow): thematic creative and personal spaces."),
    ("azul", "Color namespace (blue): thematic creative and personal spaces."),
    ("verde", "Color namespace (green): thematic creative and personal spaces."),
    ("preto", "Color namespace (black): thematic creative and personal spaces."),
    ("branco", "Color namespace (white): thematic creative and personal spaces."),
    ("blau", "Color namespace (blue): thematic creative and personal spaces."),
];

pub fn is_default_official_tld(tld: &str) -> bool {
    FEDERATE_TLDS.iter().any(|(t, _)| *t == tld)
}

// ---------------------------------------------------------------------------
// Statuses / modes / types
// ---------------------------------------------------------------------------

/// Lifecycle status of a TLD in the Federate Root Registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TldStatus {
    /// Operated by Federate Network itself.
    Official,
    /// Operated by a user/operator under delegation.
    Delegated,
    /// Cannot be purchased: needed for infrastructure/governance/future use.
    Reserved,
    /// Cannot be created: public DNS/IANA collision, brand, phishing, policy.
    Blocked,
    /// Exists but temporarily not resolvable.
    Disabled,
    /// Application exists, not yet approved.
    Pending,
    /// Ownership/lease expired.
    Expired,
    /// Removed by root governance (abuse, nonpayment, legal, emergency).
    Revoked,
}

impl TldStatus {
    pub fn is_resolvable(self) -> bool {
        matches!(self, TldStatus::Official | TldStatus::Delegated)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            TldStatus::Official => "official",
            TldStatus::Delegated => "delegated",
            TldStatus::Reserved => "reserved",
            TldStatus::Blocked => "blocked",
            TldStatus::Disabled => "disabled",
            TldStatus::Pending => "pending",
            TldStatus::Expired => "expired",
            TldStatus::Revoked => "revoked",
        }
    }
}

/// Operational mode of a TLD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TldMode {
    Official,
    Delegated,
    Reserved,
    Blocked,
}

/// Where the domain registry for a TLD lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryType {
    /// Domains live in the Federate root zone itself (official TLDs).
    RootManaged,
    /// Registry published as a content-addressed signed registry manifest
    /// (`registry_manifest_hash` in the TLD record). Immutable per root zone
    /// version: updating the registry means the root re-signs the TLD record
    /// with the new hash.
    DelegatedManifest,
    /// Registry served live by native Federate registry providers
    /// (`registry_providers` in the TLD record, host:port). The operator can
    /// update domains without a root re-sign; clients verify the operator
    /// signature on the registry and enforce version rollback protection.
    DelegatedNative,
    /// Registry served by an operator HTTP endpoint (`registry_endpoint`).
    /// Compatibility twin of `DelegatedNative`: same signed registry
    /// document, fetched over HTTP instead of the native protocol.
    DelegatedHttp,
    /// Manually administered offline registry (future).
    OfflineManual,
}

/// Lifecycle status of a domain inside a TLD registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DomainStatus {
    Active,
    Reserved,
    Pending,
    Suspended,
    Expired,
    Revoked,
    Transferred,
}

impl DomainStatus {
    pub fn is_resolvable(self) -> bool {
        matches!(self, DomainStatus::Active)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            DomainStatus::Active => "active",
            DomainStatus::Reserved => "reserved",
            DomainStatus::Pending => "pending",
            DomainStatus::Suspended => "suspended",
            DomainStatus::Expired => "expired",
            DomainStatus::Revoked => "revoked",
            DomainStatus::Transferred => "transferred",
        }
    }
}

/// What a domain record points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetType {
    /// Content manifest (the only fully implemented target in the MVP).
    Manifest,
    Service,
    Node,
    Redirect,
    Placeholder,
}

// ---------------------------------------------------------------------------
// Expiration
// ---------------------------------------------------------------------------

/// Whether a record's `expires_at` (RFC 3339) has passed at `now`.
/// Fail closed: an unparseable timestamp counts as expired; a signed record
/// with a corrupt expiry must never keep resolving forever.
pub fn expired_at(expires_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> bool {
    match expires_at {
        None => false,
        Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
            Ok(t) => t <= now,
            Err(_) => true,
        },
    }
}

/// `expired_at` against the current wall clock.
pub fn expired(expires_at: Option<&str>) -> bool {
    expired_at(expires_at, chrono::Utc::now())
}

// ---------------------------------------------------------------------------
// Naming rules
// ---------------------------------------------------------------------------

/// Normalize + validate a TLD name.
/// MVP rules: lowercase ASCII a-z only, length 2-32, no hyphen/dot/whitespace,
/// no punycode. Blocklist checks live in federate-root (they need the lists).
pub fn validate_tld_name(input: &str) -> Result<String> {
    let tld = input.trim().trim_start_matches('.').to_ascii_lowercase();
    let reject = |reason: &str| {
        Err(FederateError::InvalidTldName {
            name: input.to_string(),
            reason: reason.to_string(),
        })
    };
    if tld.len() < 2 || tld.len() > 32 {
        return reject("length must be 2-32 characters");
    }
    if !tld.chars().all(|c| c.is_ascii_lowercase()) {
        return reject(
            "only lowercase ASCII letters a-z are allowed (no digits, hyphens, dots, or unicode)",
        );
    }
    Ok(tld)
}

/// Validate a domain label (the part before the TLD).
/// MVP rules: a-z, 0-9, hyphen; length 1-63; no leading/trailing hyphen.
pub fn validate_label(input: &str) -> Result<String> {
    let label = input.trim().to_ascii_lowercase();
    let reject = |reason: &str| {
        Err(FederateError::NotFederateDomain(format!(
            "invalid label '{input}': {reason}"
        )))
    };
    if label.is_empty() || label.len() > 63 {
        return reject("length must be 1-63 characters");
    }
    if !label
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return reject("only a-z, 0-9 and hyphen are allowed");
    }
    if label.starts_with('-') || label.ends_with('-') {
        return reject("hyphen not allowed at start or end");
    }
    Ok(label)
}

/// A parsed Federate domain: exactly one label plus one TLD (`joao.pagina`).
/// Subdomains are future work. Parsing is purely syntactic; whether the TLD
/// actually exists/resolves is decided against the signed root zone.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FederateDomain {
    pub name: String,
    pub tld: String,
}

impl FederateDomain {
    /// Parse a host header value. Strips any port.
    pub fn parse(host: &str) -> Result<Self> {
        let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
        let mut parts = host.splitn(2, '.');
        let name = parts.next().unwrap_or_default().to_string();
        let tld = parts
            .next()
            .ok_or_else(|| FederateError::NotFederateDomain(host.clone()))?
            .to_string();
        if tld.contains('.') {
            return Err(FederateError::NotFederateDomain(format!(
                "{host}: subdomains are not supported yet (exactly one label + one TLD)"
            )));
        }
        let name = validate_label(&name)?;
        let tld = validate_tld_name(&tld)?;
        Ok(Self { name, tld })
    }

    pub fn fqdn(&self) -> String {
        format!("{}.{}", self.name, self.tld)
    }
}

impl std::fmt::Display for FederateDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fqdn())
    }
}

// ---------------------------------------------------------------------------
// Domain record
// ---------------------------------------------------------------------------

/// A domain record inside a TLD registry. Domains resolve to identities
/// (manifest hash, future service/node ids), never directly to public IPs.
///
/// Signed by the TLD operator key (`operator_public_key` in the TldRecord).
/// The signature covers the canonical JSON with `signature: null`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRecord {
    pub domain: String,
    pub tld: String,
    pub label: String,
    /// Key authorized to publish/update this domain's manifest.
    pub owner_public_key: String,
    pub target_type: TargetType,
    /// Hash of the current signed manifest (when target_type = manifest).
    pub manifest_hash: String,
    /// Placeholder for a future service identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    /// Placeholder for a future node identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub status: DomainStatus,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Renewal metadata placeholder (future).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renewal: Option<serde_json::Value>,
    /// Pricing metadata placeholder (future marketplace; no payments yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<serde_json::Value>,
    pub signature_algorithm: String,
    /// Ed25519 signature (hex) by the TLD operator key over canonical bytes.
    #[serde(default)]
    pub signature: Option<String>,
}

impl DomainRecord {
    /// Whether this record's lease has expired (status alone is not enough:
    /// an old signed record stays valid crypto-wise after its expiry passes).
    pub fn is_expired(&self) -> bool {
        expired(self.expires_at.as_deref())
    }

    /// Bytes covered by the signature: canonical JSON with signature = None.
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        federate_core::canonical::canonical_bytes(&unsigned)
    }

    /// Verify this record is signed by the given TLD operator key and is
    /// internally consistent with its TLD.
    pub fn verify(&self, operator_public_key: &str) -> Result<()> {
        let fail = |reason: &str| {
            Err(FederateError::VerificationFailed {
                layer: "domain".into(),
                subject: self.domain.clone(),
                reason: reason.to_string(),
            })
        };
        if self.domain != format!("{}.{}", self.label, self.tld) {
            return fail("domain does not match label + tld");
        }
        let Some(sig) = &self.signature else {
            return fail("record is unsigned");
        };
        if !federate_identity::verify_signature(operator_public_key, &self.signable_bytes()?, sig) {
            return fail("signature is not from the authorized TLD operator key");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_tld_names_pass() {
        assert_eq!(validate_tld_name("fed").unwrap(), "fed");
        assert_eq!(validate_tld_name(".FEMBOY ").unwrap(), "femboy");
        assert_eq!(validate_tld_name("ab").unwrap(), "ab");
    }

    #[test]
    fn invalid_tld_names_fail() {
        assert!(validate_tld_name("a").is_err()); // too short
        assert!(validate_tld_name(&"a".repeat(33)).is_err()); // too long
        assert!(validate_tld_name("fe-d").is_err()); // hyphen
        assert!(validate_tld_name("fe.d").is_err()); // dot
        assert!(validate_tld_name("fed1").is_err()); // digit
        assert!(validate_tld_name("fé").is_err()); // unicode
        assert!(validate_tld_name("f d").is_err()); // whitespace
    }

    #[test]
    fn label_rules() {
        assert_eq!(validate_label("Joao").unwrap(), "joao");
        assert_eq!(validate_label("a-1").unwrap(), "a-1");
        assert!(validate_label("-a").is_err());
        assert!(validate_label("a-").is_err());
        assert!(validate_label("").is_err());
        assert!(validate_label(&"a".repeat(64)).is_err());
    }

    #[test]
    fn parses_federate_domains() {
        let d = FederateDomain::parse("home.fed:80").unwrap();
        assert_eq!(d.fqdn(), "home.fed");
        assert!(FederateDomain::parse("eu.femboy").is_ok()); // syntax OK, existence decided by root zone
        assert!(FederateDomain::parse("a.b.fed").is_err()); // subdomain
        assert!(FederateDomain::parse("fed").is_err());
    }

    #[test]
    fn expiry_rules() {
        let now = chrono::Utc::now();
        // no expiry -> never expires
        assert!(!expired_at(None, now));
        // future -> not expired
        let future = (now + chrono::Duration::days(30)).to_rfc3339();
        assert!(!expired_at(Some(&future), now));
        // past -> expired
        let past = (now - chrono::Duration::days(1)).to_rfc3339();
        assert!(expired_at(Some(&past), now));
        // garbage timestamp -> fail closed (expired)
        assert!(expired_at(Some("not a date"), now));
        assert!(expired_at(Some(""), now));
    }

    #[test]
    fn domain_record_sign_verify() {
        let dir = std::env::temp_dir().join(format!("fed-naming-test-{}", std::process::id()));
        let op = federate_identity::NodeIdentity::load_or_create(&dir).unwrap();
        let mut rec = DomainRecord {
            domain: "joao.pagina".into(),
            tld: "pagina".into(),
            label: "joao".into(),
            owner_public_key: "00".repeat(32),
            target_type: TargetType::Manifest,
            manifest_hash: "abc".into(),
            service_id: None,
            node_id: None,
            status: DomainStatus::Active,
            created_at: "t".into(),
            updated_at: "t".into(),
            expires_at: None,
            renewal: None,
            pricing: None,
            signature_algorithm: "ed25519".into(),
            signature: None,
        };
        rec.signature = Some(op.sign(&rec.signable_bytes().unwrap()));
        assert!(rec.verify(&op.node_id()).is_ok());
        // wrong operator key fails
        assert!(rec.verify(&"11".repeat(32)).is_err());
        // tampering fails
        let mut bad = rec.clone();
        bad.manifest_hash = "evil".into();
        assert!(bad.verify(&op.node_id()).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
