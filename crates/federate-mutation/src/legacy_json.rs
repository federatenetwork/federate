//! The pre-redb JSON file layout (state.json + JSONL logs + snapshots).
//!
//! This format is NO LONGER a production backend. It exists only as a
//! read-only migration source for `federate registry migrate-json-to-redb`
//! and as a writer for migration tests. The redb database is the
//! authoritative registry store.

use crate::audit::AuditRecord;
use crate::store::AppliedMutation;
use federate_core::{FederateError, Result};
use federate_registry::TldRegistry;
use federate_root::RootZone;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// On-disk shape of the legacy state.json.
#[derive(Serialize, Deserialize)]
struct PersistedState {
    zone: RootZone,
    /// tld -> exact signed registry JSON bytes, hex-encoded
    registries: BTreeMap<String, String>,
    /// target key ("domain:x.y" | "tld:z") -> last accepted mutation version
    target_versions: BTreeMap<String, u64>,
}

/// Fully loaded and validated legacy state, ready to import.
pub struct LegacyJsonState {
    pub zone: RootZone,
    /// tld -> (exact signed bytes, parsed + verified registry)
    pub registries: BTreeMap<String, (Vec<u8>, TldRegistry)>,
    pub target_versions: BTreeMap<String, u64>,
    pub applied: Vec<AppliedMutation>,
    pub audit: Vec<AuditRecord>,
    /// Older signed zone versions found under snapshots/ (version -> bytes).
    pub snapshot_zones: BTreeMap<u64, Vec<u8>>,
}

/// True when a legacy JSON registry lives in this directory.
pub fn exists(dir: &Path) -> bool {
    dir.join("state.json").is_file()
}

/// Load and FULLY validate a legacy JSON registry: zone signature against
/// the expected root key, every delegated registry against its operator
/// key, every audit event against the root key. Any failure aborts the
/// load; migration must never import forged state.
pub fn load(dir: &Path, expected_root_key: &str) -> Result<LegacyJsonState> {
    let bytes = std::fs::read(dir.join("state.json"))?;
    let state: PersistedState = serde_json::from_slice(&bytes)?;
    state.zone.validate()?;
    state.zone.verify(expected_root_key)?;

    let mut registries = BTreeMap::new();
    for (tld, hex_bytes) in state.registries {
        let raw = hex::decode(&hex_bytes).map_err(|_| {
            FederateError::InvalidRoot(format!("persisted .{tld} registry is not hex"))
        })?;
        let parsed: TldRegistry = serde_json::from_slice(&raw)?;
        let record = state.zone.lookup_tld(&tld).ok_or_else(|| {
            FederateError::InvalidRoot(format!(
                "persisted registry for .{tld} has no TLD record in the zone"
            ))
        })?;
        parsed.verify(&tld, &record.operator_public_key)?;
        registries.insert(tld, (raw, parsed));
    }

    let mut applied = Vec::new();
    for line in read_lines(&dir.join("mutations.jsonl"))? {
        applied.push(serde_json::from_str::<AppliedMutation>(&line).map_err(|e| {
            FederateError::InvalidRoot(format!("unreadable mutation history line: {e}"))
        })?);
    }
    let mut audit = Vec::new();
    for line in read_lines(&dir.join("audit.jsonl"))? {
        let event: AuditRecord = serde_json::from_str(&line)
            .map_err(|e| FederateError::InvalidRoot(format!("unreadable audit line: {e}")))?;
        event.verify(expected_root_key)?;
        audit.push(event);
    }

    // Import older zone versions from snapshot files when they parse and
    // verify; skip anything else (they are convenience copies, not
    // authority).
    let mut snapshot_zones = BTreeMap::new();
    let snapshot_dir = dir.join("snapshots");
    if snapshot_dir.is_dir() {
        for entry in std::fs::read_dir(&snapshot_dir)?.filter_map(|e| e.ok()) {
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            let Ok(zone) = serde_json::from_slice::<RootZone>(&bytes) else {
                continue;
            };
            if zone.verify(expected_root_key).is_ok() {
                snapshot_zones.insert(zone.root_version, bytes);
            }
        }
    }

    Ok(LegacyJsonState {
        zone: state.zone,
        registries,
        target_versions: state.target_versions,
        applied,
        audit,
        snapshot_zones,
    })
}

/// Move the legacy files into a backup subdirectory after a successful
/// migration, so the directory no longer looks like a JSON registry.
pub fn backup_files(dir: &Path) -> Result<PathBuf> {
    let backup = dir.join("legacy-json-backup");
    std::fs::create_dir_all(&backup)?;
    for name in ["state.json", "mutations.jsonl", "audit.jsonl"] {
        let src = dir.join(name);
        if src.is_file() {
            std::fs::rename(&src, backup.join(name))?;
        }
    }
    Ok(backup)
}

/// Write a legacy-format registry. Used ONLY by migration tests and
/// tooling that needs to fabricate the old layout; production never writes
/// this format anymore.
pub fn write(
    dir: &Path,
    zone: &RootZone,
    registries: &BTreeMap<String, (Vec<u8>, TldRegistry)>,
    target_versions: &BTreeMap<String, u64>,
    applied: &[AppliedMutation],
    audit: &[AuditRecord],
) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let state = PersistedState {
        zone: zone.clone(),
        registries: registries
            .iter()
            .map(|(tld, (bytes, _))| (tld.clone(), hex::encode(bytes)))
            .collect(),
        target_versions: target_versions.clone(),
    };
    std::fs::write(dir.join("state.json"), serde_json::to_vec_pretty(&state)?)?;
    write_jsonl(&dir.join("mutations.jsonl"), applied)?;
    write_jsonl(&dir.join("audit.jsonl"), audit)?;
    Ok(())
}

fn write_jsonl<T: Serialize>(path: &Path, items: &[T]) -> Result<()> {
    let mut out = Vec::new();
    for item in items {
        out.extend(serde_json::to_vec(item)?);
        out.push(b'\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

fn read_lines(path: &Path) -> Result<Vec<String>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    Ok(std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}
