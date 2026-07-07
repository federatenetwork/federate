//! redb implementation of the registry storage backend: the authoritative
//! local registry database (`registry.redb`), transactional and crash-safe.
//!
//! Logical tables: tld_records, domain_records, root_zone_versions,
//! mutations, audit_events, snapshots, nonces, registry_metadata,
//! delegated_registries, target_versions.
//!
//! Records are stored as JSON values (they are signed JSON documents by
//! design; the database provides durability and atomicity, signatures
//! provide integrity and authority). Private keys are NEVER stored here.

use crate::audit::AuditRecord;
use crate::backend::{CommitBatch, InitialState, RegistryBackend, SnapshotMeta};
use crate::store::AppliedMutation;
use federate_core::{FederateError, Result};
use federate_naming::DomainRecord;
use federate_root::TldRecord;
use redb::{
    Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
    WriteTransaction,
};
use std::path::{Path, PathBuf};

const TLD_RECORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("tld_records");
const DOMAIN_RECORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("domain_records");
const ROOT_ZONE_VERSIONS: TableDefinition<u64, &[u8]> = TableDefinition::new("root_zone_versions");
const MUTATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("mutations");
const AUDIT_EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("audit_events");
const SNAPSHOTS: TableDefinition<u64, &[u8]> = TableDefinition::new("snapshots");
const NONCES: TableDefinition<&str, i64> = TableDefinition::new("nonces");
const REGISTRY_METADATA: TableDefinition<&str, &str> = TableDefinition::new("registry_metadata");
const DELEGATED_REGISTRIES: TableDefinition<&str, &[u8]> =
    TableDefinition::new("delegated_registries");
const TARGET_VERSIONS: TableDefinition<&str, u64> = TableDefinition::new("target_versions");

/// Filename of the registry database inside the registry dir.
pub const REGISTRY_DB_FILE: &str = "registry.redb";

pub(crate) fn db_err(e: impl std::fmt::Display) -> FederateError {
    FederateError::Storage(e.to_string())
}

pub struct RedbRegistryStore {
    db: Database,
    path: PathBuf,
}

impl RedbRegistryStore {
    pub fn db_path(dir: &Path) -> PathBuf {
        dir.join(REGISTRY_DB_FILE)
    }

    /// Create a fresh database (all tables present, empty). Refuses to
    /// clobber an existing one.
    pub fn create(dir: &Path) -> Result<Self> {
        let path = Self::db_path(dir);
        if path.exists() {
            return Err(FederateError::Storage(format!(
                "database already exists at {}",
                path.display()
            )));
        }
        std::fs::create_dir_all(dir)?;
        let db = Database::create(&path).map_err(db_err)?;
        let store = Self { db, path };
        store.ensure_tables()?;
        Ok(store)
    }

    /// Open an existing database.
    pub fn open(dir: &Path) -> Result<Self> {
        let path = Self::db_path(dir);
        let db = Database::open(&path).map_err(db_err)?;
        let store = Self { db, path };
        store.ensure_tables()?;
        Ok(store)
    }

    fn ensure_tables(&self) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        txn.open_table(TLD_RECORDS).map_err(db_err)?;
        txn.open_table(DOMAIN_RECORDS).map_err(db_err)?;
        txn.open_table(ROOT_ZONE_VERSIONS).map_err(db_err)?;
        txn.open_table(MUTATIONS).map_err(db_err)?;
        txn.open_table(AUDIT_EVENTS).map_err(db_err)?;
        txn.open_table(SNAPSHOTS).map_err(db_err)?;
        txn.open_table(NONCES).map_err(db_err)?;
        txn.open_table(REGISTRY_METADATA).map_err(db_err)?;
        txn.open_table(DELEGATED_REGISTRIES).map_err(db_err)?;
        txn.open_table(TARGET_VERSIONS).map_err(db_err)?;
        txn.commit().map_err(db_err)
    }

    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
    ) -> Result<Option<T>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(table).map_err(db_err)?;
        match t.get(key).map_err(db_err)? {
            Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
            None => Ok(None),
        }
    }

    fn list_json<T: serde::de::DeserializeOwned>(
        &self,
        table: TableDefinition<&str, &[u8]>,
    ) -> Result<Vec<T>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(table).map_err(db_err)?;
        let mut out = Vec::new();
        for entry in t.iter().map_err(db_err)? {
            let (_, v) = entry.map_err(db_err)?;
            out.push(serde_json::from_slice(v.value())?);
        }
        Ok(out)
    }

    fn put_json<T: serde::Serialize>(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
        value: &T,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        let txn = self.db.begin_write().map_err(db_err)?;
        {
            let mut t = txn.open_table(table).map_err(db_err)?;
            t.insert(key, bytes.as_slice()).map_err(db_err)?;
        }
        txn.commit().map_err(db_err)
    }

    /// Rewrite the record tables so they mirror the new zone exactly.
    fn write_record_tables(
        txn: &WriteTransaction,
        tlds: &[TldRecord],
        domains: &[DomainRecord],
    ) -> Result<()> {
        txn.delete_table(TLD_RECORDS).map_err(db_err)?;
        txn.delete_table(DOMAIN_RECORDS).map_err(db_err)?;
        let mut t = txn.open_table(TLD_RECORDS).map_err(db_err)?;
        for rec in tlds {
            t.insert(rec.tld.as_str(), serde_json::to_vec(rec)?.as_slice())
                .map_err(db_err)?;
        }
        let mut d = txn.open_table(DOMAIN_RECORDS).map_err(db_err)?;
        for rec in domains {
            d.insert(rec.domain.as_str(), serde_json::to_vec(rec)?.as_slice())
                .map_err(db_err)?;
        }
        Ok(())
    }

    fn write_zone_pointer(
        txn: &WriteTransaction,
        version: u64,
        zone_json: &[u8],
        state_hash: &str,
    ) -> Result<()> {
        let mut zones = txn.open_table(ROOT_ZONE_VERSIONS).map_err(db_err)?;
        zones.insert(version, zone_json).map_err(db_err)?;
        drop(zones);
        let mut meta = txn.open_table(REGISTRY_METADATA).map_err(db_err)?;
        meta.insert("current_version", version.to_string().as_str())
            .map_err(db_err)?;
        meta.insert("state_hash", state_hash).map_err(db_err)?;
        Ok(())
    }

    fn write_snapshot_meta(txn: &WriteTransaction, meta: &SnapshotMeta) -> Result<()> {
        let mut t = txn.open_table(SNAPSHOTS).map_err(db_err)?;
        t.insert(meta.root_version, serde_json::to_vec(meta)?.as_slice())
            .map_err(db_err)?;
        Ok(())
    }

    fn next_audit_seq(txn: &WriteTransaction) -> Result<u64> {
        let t = txn.open_table(AUDIT_EVENTS).map_err(db_err)?;
        let seq = t
            .last()
            .map_err(db_err)?
            .map(|(k, _)| k.value() + 1)
            .unwrap_or(0);
        drop(t);
        Ok(seq)
    }
}

impl RegistryBackend for RedbRegistryStore {
    fn get_tld(&self, tld: &str) -> Result<Option<TldRecord>> {
        self.get_json(TLD_RECORDS, tld)
    }

    fn put_tld(&self, record: &TldRecord) -> Result<()> {
        self.put_json(TLD_RECORDS, &record.tld.clone(), record)
    }

    fn list_tlds(&self) -> Result<Vec<TldRecord>> {
        self.list_json(TLD_RECORDS)
    }

    fn get_domain(&self, fqdn: &str) -> Result<Option<DomainRecord>> {
        self.get_json(DOMAIN_RECORDS, fqdn)
    }

    fn put_domain(&self, record: &DomainRecord) -> Result<()> {
        self.put_json(DOMAIN_RECORDS, &record.domain.clone(), record)
    }

    fn list_domains(&self) -> Result<Vec<DomainRecord>> {
        self.list_json(DOMAIN_RECORDS)
    }

    fn get_root_zone_version(&self, version: u64) -> Result<Option<Vec<u8>>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(ROOT_ZONE_VERSIONS).map_err(db_err)?;
        Ok(t.get(version).map_err(db_err)?.map(|v| v.value().to_vec()))
    }

    fn put_root_zone_version(&self, version: u64, zone_json: &[u8]) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        {
            let mut t = txn.open_table(ROOT_ZONE_VERSIONS).map_err(db_err)?;
            t.insert(version, zone_json).map_err(db_err)?;
        }
        txn.commit().map_err(db_err)
    }

    fn current_root_zone(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let Some(version) = self.get_meta("current_version")? else {
            return Ok(None);
        };
        let version: u64 = version
            .parse()
            .map_err(|_| FederateError::Storage("corrupt current_version metadata".into()))?;
        Ok(self
            .get_root_zone_version(version)?
            .map(|bytes| (version, bytes)))
    }

    fn append_mutation(&self, applied: &AppliedMutation) -> Result<()> {
        self.put_json(MUTATIONS, &applied.mutation.mutation_id.clone(), applied)
    }

    fn get_mutation(&self, mutation_id: &str) -> Result<Option<AppliedMutation>> {
        self.get_json(MUTATIONS, mutation_id)
    }

    fn list_mutations(&self) -> Result<Vec<AppliedMutation>> {
        self.list_json(MUTATIONS)
    }

    fn append_audit_event(&self, event: &AuditRecord) -> Result<()> {
        let bytes = serde_json::to_vec(event)?;
        let txn = self.db.begin_write().map_err(db_err)?;
        {
            let seq = Self::next_audit_seq(&txn)?;
            let mut t = txn.open_table(AUDIT_EVENTS).map_err(db_err)?;
            t.insert(seq, bytes.as_slice()).map_err(db_err)?;
        }
        txn.commit().map_err(db_err)
    }

    fn list_audit_events(&self) -> Result<Vec<AuditRecord>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(AUDIT_EVENTS).map_err(db_err)?;
        let mut out = Vec::new();
        for entry in t.iter().map_err(db_err)? {
            let (_, v) = entry.map_err(db_err)?;
            out.push(serde_json::from_slice(v.value())?);
        }
        Ok(out)
    }

    fn reserve_nonce(&self, nonce: &str, expires_at: i64) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        {
            let mut t = txn.open_table(NONCES).map_err(db_err)?;
            // Opportunistic prune of expired challenges (now = expires_at
            // minus the TTL is unknown here; prune strictly older ones).
            let expired: Vec<String> = t
                .iter()
                .map_err(db_err)?
                .filter_map(|e| e.ok())
                .filter(|(_, exp)| exp.value() < expires_at - 2 * crate::NONCE_TTL_SECS)
                .map(|(k, _)| k.value().to_string())
                .collect();
            for key in expired {
                t.remove(key.as_str()).map_err(db_err)?;
            }
            t.insert(nonce, expires_at).map_err(db_err)?;
        }
        txn.commit().map_err(db_err)
    }

    fn consume_nonce(&self, nonce: &str, now: i64) -> Result<bool> {
        let txn = self.db.begin_write().map_err(db_err)?;
        let valid;
        {
            let mut t = txn.open_table(NONCES).map_err(db_err)?;
            valid = match t.remove(nonce).map_err(db_err)? {
                Some(expires) => expires.value() > now,
                None => false,
            };
        }
        txn.commit().map_err(db_err)?;
        Ok(valid)
    }

    fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(REGISTRY_METADATA).map_err(db_err)?;
        Ok(t.get(key).map_err(db_err)?.map(|v| v.value().to_string()))
    }

    fn put_meta(&self, key: &str, value: &str) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        {
            let mut t = txn.open_table(REGISTRY_METADATA).map_err(db_err)?;
            t.insert(key, value).map_err(db_err)?;
        }
        txn.commit().map_err(db_err)
    }

    fn get_target_version(&self, target_key: &str) -> Result<u64> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(TARGET_VERSIONS).map_err(db_err)?;
        Ok(t.get(target_key)
            .map_err(db_err)?
            .map(|v| v.value())
            .unwrap_or(0))
    }

    fn put_target_version(&self, target_key: &str, version: u64) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        {
            let mut t = txn.open_table(TARGET_VERSIONS).map_err(db_err)?;
            t.insert(target_key, version).map_err(db_err)?;
        }
        txn.commit().map_err(db_err)
    }

    fn list_target_versions(&self) -> Result<Vec<(String, u64)>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(TARGET_VERSIONS).map_err(db_err)?;
        let mut out = Vec::new();
        for entry in t.iter().map_err(db_err)? {
            let (k, v) = entry.map_err(db_err)?;
            out.push((k.value().to_string(), v.value()));
        }
        Ok(out)
    }

    fn get_delegated_registry(&self, tld: &str) -> Result<Option<Vec<u8>>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(DELEGATED_REGISTRIES).map_err(db_err)?;
        Ok(t.get(tld).map_err(db_err)?.map(|v| v.value().to_vec()))
    }

    fn list_delegated_registries(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(DELEGATED_REGISTRIES).map_err(db_err)?;
        let mut out = Vec::new();
        for entry in t.iter().map_err(db_err)? {
            let (k, v) = entry.map_err(db_err)?;
            out.push((k.value().to_string(), v.value().to_vec()));
        }
        Ok(out)
    }

    fn create_snapshot(&self, meta: &SnapshotMeta) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        Self::write_snapshot_meta(&txn, meta)?;
        txn.commit().map_err(db_err)
    }

    fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let t = txn.open_table(SNAPSHOTS).map_err(db_err)?;
        let mut out = Vec::new();
        for entry in t.iter().map_err(db_err)? {
            let (_, v) = entry.map_err(db_err)?;
            out.push(serde_json::from_slice(v.value())?);
        }
        Ok(out)
    }

    fn commit_initial(&self, state: &InitialState) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        Self::write_record_tables(&txn, &state.tlds, &state.domains)?;
        Self::write_zone_pointer(
            &txn,
            state.root_version,
            &state.zone_json,
            &state.state_hash,
        )?;
        {
            let mut t = txn.open_table(DELEGATED_REGISTRIES).map_err(db_err)?;
            for (tld, bytes) in &state.delegated_registries {
                t.insert(tld.as_str(), bytes.as_slice()).map_err(db_err)?;
            }
        }
        Self::write_snapshot_meta(&txn, &state.snapshot)?;
        txn.commit().map_err(db_err)
    }

    fn commit_mutation(&self, batch: &CommitBatch) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_err)?;
        Self::write_record_tables(&txn, &batch.tlds, &batch.domains)?;
        Self::write_zone_pointer(
            &txn,
            batch.root_version,
            &batch.zone_json,
            &batch.state_hash,
        )?;
        {
            let mut t = txn.open_table(TARGET_VERSIONS).map_err(db_err)?;
            t.insert(batch.target_key.as_str(), batch.target_version)
                .map_err(db_err)?;
        }
        {
            let bytes = serde_json::to_vec(&batch.applied)?;
            let mut t = txn.open_table(MUTATIONS).map_err(db_err)?;
            t.insert(
                batch.applied.mutation.mutation_id.as_str(),
                bytes.as_slice(),
            )
            .map_err(db_err)?;
        }
        {
            let seq = Self::next_audit_seq(&txn)?;
            let bytes = serde_json::to_vec(&batch.audit)?;
            let mut t = txn.open_table(AUDIT_EVENTS).map_err(db_err)?;
            t.insert(seq, bytes.as_slice()).map_err(db_err)?;
        }
        if let Some((tld, bytes)) = &batch.delegated_registry {
            let mut t = txn.open_table(DELEGATED_REGISTRIES).map_err(db_err)?;
            t.insert(tld.as_str(), bytes.as_slice()).map_err(db_err)?;
        }
        Self::write_snapshot_meta(&txn, &batch.snapshot)?;
        txn.commit().map_err(db_err)
    }

    fn stats(&self) -> Result<serde_json::Value> {
        let txn = self.db.begin_read().map_err(db_err)?;
        let count = |name: &str| -> Result<u64> {
            match name {
                "tld_records" => Ok(txn
                    .open_table(TLD_RECORDS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "domain_records" => Ok(txn
                    .open_table(DOMAIN_RECORDS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "root_zone_versions" => Ok(txn
                    .open_table(ROOT_ZONE_VERSIONS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "mutations" => Ok(txn
                    .open_table(MUTATIONS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "audit_events" => Ok(txn
                    .open_table(AUDIT_EVENTS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "snapshots" => Ok(txn
                    .open_table(SNAPSHOTS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "nonces" => Ok(txn
                    .open_table(NONCES)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "registry_metadata" => Ok(txn
                    .open_table(REGISTRY_METADATA)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "delegated_registries" => Ok(txn
                    .open_table(DELEGATED_REGISTRIES)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                "target_versions" => Ok(txn
                    .open_table(TARGET_VERSIONS)
                    .map_err(db_err)?
                    .len()
                    .map_err(db_err)?),
                _ => Ok(0),
            }
        };
        let mut tables = serde_json::Map::new();
        for name in [
            "tld_records",
            "domain_records",
            "root_zone_versions",
            "mutations",
            "audit_events",
            "snapshots",
            "nonces",
            "registry_metadata",
            "delegated_registries",
            "target_versions",
        ] {
            tables.insert(name.to_string(), serde_json::json!(count(name)?));
        }
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        Ok(serde_json::json!({
            "backend": "redb",
            "path": self.path.display().to_string(),
            "file_bytes": size,
            "tables": tables,
        }))
    }
}
