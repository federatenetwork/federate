//! Local certificate authority for HTTPS on Federate names.
//!
//! Public CAs cannot issue for `.fed` (not an ICANN TLD), so the green
//! lock has to come from a CA the machine itself controls, mkcert-style:
//!
//! - the CA keypair is generated ON this machine and its private key
//!   never leaves it (world-readable dirs get a 0600 key file). A shared
//!   network-wide CA would let its holder impersonate any HTTPS site for
//!   every installed user; per-machine keys make that structurally
//!   impossible.
//! - the CA certificate (public half) is added to the system trust store
//!   by `federate setup` / removed by `federate dns uninstall`.
//! - the local gateway mints a short-lived leaf certificate per SNI name
//!   on first use, signed by this CA, cached in memory.
//!
//! TLS here is only browser transport on loopback. Content integrity
//! never depends on it: every byte is still verified against the signed
//! root zone -> TLD -> domain -> manifest -> block chain before serving.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const CA_CERT_FILE: &str = "ca.pem";
pub const CA_KEY_FILE: &str = "ca.key";
const CA_NAME: &str = "Federate Local CA";
/// Leaf certificates are re-minted on service restart anyway; keep their
/// lifetime short so a copied leaf key ages out fast.
const LEAF_DAYS: u64 = 30;
const CA_DAYS: u64 = 3650;

/// Where the CA lives: next to the resolver service data, owned by the
/// service user (root/SYSTEM), so the minting side can read the key and
/// ordinary processes cannot.
pub fn ca_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/Federate/ca")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(r"C:\ProgramData\Federate\ca")
    } else {
        PathBuf::from("/var/lib/federate/ca")
    }
}

pub struct LocalCa {
    issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
    // rcgen::Issuer has no Debug; manual impl below keeps derive users happy.
    /// PEM of the CA certificate (for trust stores and clients).
    pub cert_pem: String,
    /// name -> (cert der chain, key der), minted on demand.
    minted: Mutex<std::collections::HashMap<String, MintedLeaf>>,
}

#[derive(Clone)]
pub struct MintedLeaf {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

fn ca_params(hostname: &str) -> rcgen::CertificateParams {
    let mut params = rcgen::CertificateParams::default();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, format!("{CA_NAME} ({hostname})"));
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Federate Network (local)");
    let now = std::time::SystemTime::now();
    params.not_before = now.into();
    params.not_after = (now + std::time::Duration::from_secs(CA_DAYS * 86400)).into();
    params
}

impl LocalCa {
    /// Create the CA keypair + certificate at `dir` if absent; always
    /// returns a ready-to-mint CA. The key file is created 0600 on unix.
    pub fn load_or_create(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let cert_path = dir.join(CA_CERT_FILE);
        let key_path = dir.join(CA_KEY_FILE);
        if cert_path.exists() && key_path.exists() {
            let cert_pem = std::fs::read_to_string(&cert_path)
                .map_err(|e| format!("cannot read {}: {e}", cert_path.display()))?;
            let key_pem = std::fs::read_to_string(&key_path)
                .map_err(|e| format!("cannot read {}: {e}", key_path.display()))?;
            let key = rcgen::KeyPair::from_pem(&key_pem).map_err(|e| format!("bad CA key: {e}"))?;
            let issuer = rcgen::Issuer::from_ca_cert_pem(&cert_pem, key)
                .map_err(|e| format!("bad CA cert: {e}"))?;
            return Ok(Self {
                issuer,
                cert_pem,
                minted: Mutex::new(std::collections::HashMap::new()),
            });
        }
        let hostname = hostname();
        let key = rcgen::KeyPair::generate().map_err(|e| format!("keygen failed: {e}"))?;
        let params = ca_params(&hostname);
        let cert = params
            .self_signed(&key)
            .map_err(|e| format!("CA self-sign failed: {e}"))?;
        let cert_pem = cert.pem();
        std::fs::write(&cert_path, &cert_pem)
            .map_err(|e| format!("cannot write {}: {e}", cert_path.display()))?;
        write_private(&key_path, key.serialize_pem().as_bytes())?;
        let issuer = rcgen::Issuer::new(ca_params(&hostname), key);
        Ok(Self {
            issuer,
            cert_pem,
            minted: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Leaf certificate for one DNS name, minted on first use.
    pub fn leaf_for(&self, name: &str) -> Result<MintedLeaf, String> {
        if let Some(hit) = self.minted.lock().expect("ca cache lock").get(name) {
            return Ok(hit.clone());
        }
        let key = rcgen::KeyPair::generate().map_err(|e| format!("leaf keygen failed: {e}"))?;
        let mut params = rcgen::CertificateParams::new(vec![name.to_string()])
            .map_err(|e| format!("bad SAN {name}: {e}"))?;
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, name);
        let now = std::time::SystemTime::now();
        params.not_before = now.into();
        params.not_after = (now + std::time::Duration::from_secs(LEAF_DAYS * 86400)).into();
        params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let cert = params
            .signed_by(&key, &self.issuer)
            .map_err(|e| format!("leaf signing failed: {e}"))?;
        let leaf = MintedLeaf {
            cert_der: cert.der().to_vec(),
            key_der: key.serialize_der(),
        };
        self.minted
            .lock()
            .expect("ca cache lock")
            .insert(name.to_string(), leaf.clone());
        Ok(leaf)
    }

    /// DER of the CA certificate (appended to served chains so clients
    /// that want the full chain get it).
    pub fn ca_der(&self) -> Result<Vec<u8>, String> {
        pem_to_der(&self.cert_pem).ok_or_else(|| "unparseable CA PEM".to_string())
    }
}

impl std::fmt::Debug for LocalCa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCa").finish_non_exhaustive()
    }
}

fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(body).ok()
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot chmod {}: {e}", path.display()))?;
    }
    Ok(())
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "this machine".to_string())
}

// ---------------------------------------------------------------------------
// system trust store
// ---------------------------------------------------------------------------

/// Add the CA certificate to the system trust store. Requires root/admin
/// (callers run under `federate setup`). Best-effort NSS handling for
/// browsers that keep their own store on Linux.
pub fn trust_install(dir: &Path) -> Result<(), String> {
    let cert = dir.join(CA_CERT_FILE);
    if !cert.exists() {
        return Err(format!("{} does not exist", cert.display()));
    }
    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("security")
            .args(["add-trusted-cert", "-d", "-r", "trustRoot", "-k"])
            .arg("/Library/Keychains/System.keychain")
            .arg(&cert)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err("security add-trusted-cert failed".into());
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let dest = "/usr/local/share/ca-certificates/federate-local-ca.crt";
        std::fs::copy(&cert, dest).map_err(|e| format!("cannot copy CA to {dest}: {e}"))?;
        let ok = std::process::Command::new("update-ca-certificates")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err(
                "update-ca-certificates failed (is the ca-certificates package installed?)".into(),
            );
        }
        // Chrome/Firefox on Linux read NSS, not the system bundle. Add for
        // the invoking user when certutil exists; skip silently otherwise.
        if let Ok(user) = std::env::var("SUDO_USER") {
            let _ = std::process::Command::new("sudo")
                .args(["-u", &user, "sh", "-c"])
                .arg(format!(
                    "command -v certutil >/dev/null && certutil -d sql:$HOME/.pki/nssdb -A -t C,, -n 'Federate Local CA' -i {} || true",
                    cert.display()
                ))
                .status();
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let ok = std::process::Command::new("certutil")
            .args(["-addstore", "-f", "Root"])
            .arg(&cert)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err("certutil -addstore Root failed".into());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = cert;
        Err("trust store installation is not supported on this OS yet".into())
    }
}

/// Remove the CA certificate from the system trust store.
pub fn trust_uninstall(dir: &Path) {
    let cert = dir.join(CA_CERT_FILE);
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("security")
            .args(["remove-trusted-cert", "-d"])
            .arg(&cert)
            .status();
        let _ = std::process::Command::new("security")
            .args(["delete-certificate", "-c", CA_NAME])
            .arg("/Library/Keychains/System.keychain")
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = cert;
        let _ = std::fs::remove_file("/usr/local/share/ca-certificates/federate-local-ca.crt");
        let _ = std::process::Command::new("update-ca-certificates")
            .arg("--fresh")
            .status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = cert;
        let _ = std::process::Command::new("certutil")
            .args(["-delstore", "Root", CA_NAME])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CA creation is idempotent and minted leaves chain to it.
    #[test]
    fn ca_creates_once_and_mints_verifiable_leaves() {
        let dir = std::env::temp_dir().join(format!("fed-ca-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ca = LocalCa::load_or_create(&dir).expect("create CA");
        let pem_first = ca.cert_pem.clone();
        // Reload: same CA, not a new one.
        let ca2 = LocalCa::load_or_create(&dir).expect("reload CA");
        assert_eq!(pem_first, ca2.cert_pem, "reload must not regenerate the CA");

        let leaf = ca.leaf_for("home.fed").expect("mint leaf");
        assert!(!leaf.cert_der.is_empty() && !leaf.key_der.is_empty());
        // Cache: second mint returns the identical leaf.
        let leaf2 = ca.leaf_for("home.fed").expect("mint cached");
        assert_eq!(leaf.cert_der, leaf2.cert_der);

        // The leaf must verify against the CA through rustls's own stack.
        let ca_der = ca.ca_der().expect("ca der");
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls_pki_types::CertificateDer::from(ca_der))
            .expect("CA accepted as root");
        let verifier = rustls::client::WebPkiServerVerifier::builder(std::sync::Arc::new(roots))
            .build()
            .expect("verifier");
        use rustls::client::danger::ServerCertVerifier as _;
        let end = rustls_pki_types::CertificateDer::from(leaf.cert_der.clone());
        let name = rustls_pki_types::ServerName::try_from("home.fed").unwrap();
        verifier
            .verify_server_cert(&end, &[], &name, &[], rustls_pki_types::UnixTime::now())
            .expect("leaf chains to local CA for home.fed");
        std::fs::remove_dir_all(&dir).ok();
    }
}
