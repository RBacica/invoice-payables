use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;

/// A single paid-invoice record, used for the audit trail in `paid.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaidEntry {
    /// Canonical invoice key: `{branch}|{supplier_code}|{invoice_number}`.
    pub key: String,
    /// Human-readable description for the audit trail.
    pub label: String,
    /// ISO timestamp of when it was marked paid.
    pub paid_at: String,
}

/// Shape of `paid.json`: a list of paid entries (chronological order).
pub type PaidList = Vec<PaidEntry>;

/// Manages the paid-invoice ledger (`paid.json`).
/// Uses interior mutability (RwLock) so it can be shared via `web::Data`.
pub struct PaidLedger {
    entries: RwLock<PaidList>,
    file_path: String,
}

impl PaidLedger {
    /// Load the ledger from `paid.json` (creates an empty one if missing).
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let file_path = "paid.json".to_string();
        let entries = Self::load(&file_path)?;
        Ok(Self {
            entries: RwLock::new(entries),
            file_path,
        })
    }

    fn load(path: &str) -> Result<PaidList, Box<dyn std::error::Error>> {
        if std::path::Path::new(path).exists() {
            let raw = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&raw).unwrap_or_default())
        } else {
            Ok(Vec::new())
        }
    }

    fn persist(&self, entries: &PaidList) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(entries)?;
        std::fs::write(&self.file_path, json)?;
        Ok(())
    }

    /// Mark a set of invoices as paid. Appends an audit entry for each new key.
    /// Returns the number of invoices newly marked paid (already-paid ones are skipped).
    pub fn mark_paid(&self, rows: &[crate::server::PayRow]) -> Result<usize, Box<dyn std::error::Error>> {
        let mut entries = self.entries.write().map_err(|e| format!("RwLock poisoned: {}", e))?;

        // Build a set of already-paid keys (owned Strings so borrows stay valid).
        let mut existing: HashSet<String> = entries.iter().map(|e| e.key.clone()).collect();

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut added = 0usize;

        for row in rows {
            let key = format!("{}|{}|{}", row.branch, row.supplier_code, row.invoice_number);
            if existing.contains(&key) {
                continue;
            }
            entries.push(PaidEntry {
                key: key.clone(),
                label: row.label.clone(),
                paid_at: now.clone(),
            });
            existing.insert(key);
            added += 1;
        }

        self.persist(&entries)?;
        Ok(added)
    }

    /// All keys currently marked paid (as a set, for quick lookup).
    pub fn paid_keys(&self) -> HashSet<String> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .map(|e| e.key.clone())
            .collect()
    }

    /// Number of paid invoices in the ledger.
    pub fn count(&self) -> usize {
        self.entries.read().unwrap().len()
    }
}
