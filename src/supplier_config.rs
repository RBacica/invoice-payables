use crate::db::{DbPool, Supplier};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Payment term types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TermType {
    #[serde(rename = "EOM")]
    Eom,
    #[serde(rename = "NetDays")]
    NetDays,
}

impl Default for TermType {
    fn default() -> Self {
        TermType::Eom
    }
}

/// Order fulfilment frequency
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderType {
    #[serde(rename = "Weekly")]
    Weekly,
    #[serde(rename = "Monthly")]
    Monthly,
}

impl Default for OrderType {
    fn default() -> Self {
        OrderType::Monthly
    }
}

/// Supplier payment method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentType {
    #[serde(rename = "DC")]
    DirectCredit,
    #[serde(rename = "DD")]
    DirectDebit,
}

impl Default for PaymentType {
    fn default() -> Self {
        PaymentType::DirectCredit
    }
}

/// Payment terms for a single supplier, as stored in the config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermConfig {
    #[serde(default)]
    pub term_type: TermType,
    #[serde(default = "default_days")]
    pub term_days: u16,
    #[serde(default)]
    pub order_type: OrderType,
    #[serde(default)]
    pub payment_type: PaymentType,
    #[serde(default)]
    pub configured: bool,
}

fn default_days() -> u16 {
    20
}

/// Full config file shape: map of supplier_code -> TermConfig
pub type SupplierConfigMap = HashMap<String, TermConfig>;

/// A supplier config entry returned to the frontend (includes label).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierConfigEntry {
    pub code: String,
    pub label: String,
    pub term_type: String,
    pub term_days: u16,
    pub order_type: String,
    pub payment_type: String,
    pub configured: bool,
}

/// Manages the supplier payment-terms config file (`suppliers-config.toml`).
/// Uses interior mutability (RwLock) so it can be shared via `web::Data`.
pub struct SupplierConfigManager {
    config_map: RwLock<SupplierConfigMap>,
    file_path: String,
}

impl SupplierConfigManager {
    /// Load the supplier config from `suppliers-config.toml`, merge with
    /// the live DB supplier list, write back, and return the manager.
    pub async fn new(pool: &DbPool) -> Result<Self, Box<dyn std::error::Error>> {
        let file_path = "suppliers-config.toml".to_string();
        let db_suppliers = pool.get_suppliers().await?;
        let config_map = Self::load_or_create(&file_path, &db_suppliers)?;
        Ok(Self {
            config_map: RwLock::new(config_map),
            file_path,
        })
    }

    /// Load existing config, merge with current DB supplier list.
    /// New suppliers get defaults; existing keep their values.
    /// Writes the merged result back.
    fn load_or_create(
        path: &str,
        db_suppliers: &[Supplier],
    ) -> Result<SupplierConfigMap, Box<dyn std::error::Error>> {
        let existing: SupplierConfigMap = if std::path::Path::new(path).exists() {
            let raw = std::fs::read_to_string(path)?;
            toml::from_str(&raw).unwrap_or_default()
        } else {
            HashMap::new()
        };

        let mut merged = existing.clone();

        // Add any new suppliers from the DB that aren't in the config yet
        for supplier in db_suppliers {
            if !merged.contains_key(&supplier.code) {
                merged.insert(
                    supplier.code.clone(),
                    TermConfig {
                        term_type: TermType::Eom,
                        term_days: 20,
                        order_type: OrderType::Monthly,
                        payment_type: PaymentType::DirectCredit,
                        configured: false,
                    },
                );
            }
        }

        // Write merged config back
        let toml_str = toml::to_string_pretty(&merged)?;
        std::fs::write(path, toml_str)?;

        Ok(merged)
    }

    /// Save the current config map back to disk. Takes &self (interior mutability).
    pub fn save_all(&self, entries: &[SupplierConfigEntry]) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = self.config_map.write().map_err(|e| format!("RwLock poisoned: {}", e))?;
        for entry in entries {
            map.insert(
                entry.code.clone(),
                TermConfig {
                    term_type: match entry.term_type.to_lowercase().as_str() {
                        "netdays" => TermType::NetDays,
                        _ => TermType::Eom,
                    },
                    term_days: entry.term_days,
                    order_type: match entry.order_type.to_lowercase().as_str() {
                        "weekly" => OrderType::Weekly,
                        _ => OrderType::Monthly,
                    },
                    payment_type: match entry.payment_type.to_lowercase().as_str() {
                        "dd" => PaymentType::DirectDebit,
                        _ => PaymentType::DirectCredit,
                    },
                    configured: true,
                },
            );
        }
        let toml_str = toml::to_string_pretty(&*map)?;
        std::fs::write(&self.file_path, toml_str)?;
        Ok(())
    }

    /// Save a single supplier's terms. Takes &self (interior mutability).
    pub fn save_one(
        &self,
        code: &str,
        term_type: &str,
        term_days: u16,
        order_type: &str,
        payment_type: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = self.config_map.write().map_err(|e| format!("RwLock poisoned: {}", e))?;
        map.insert(
            code.to_string(),
            TermConfig {
                term_type: match term_type.to_lowercase().as_str() {
                    "netdays" => TermType::NetDays,
                    _ => TermType::Eom,
                },
                term_days,
                order_type: match order_type.to_lowercase().as_str() {
                    "weekly" => OrderType::Weekly,
                    _ => OrderType::Monthly,
                },
                payment_type: match payment_type.to_lowercase().as_str() {
                    "dd" => PaymentType::DirectDebit,
                    _ => PaymentType::DirectCredit,
                },
                configured: true,
            },
        );
        let toml_str = toml::to_string_pretty(&*map)?;
        std::fs::write(&self.file_path, toml_str)?;
        Ok(())
    }

    /// Get all suppliers with their config (for the settings page).
    pub fn get_all_entries(&self, db_suppliers: &[Supplier]) -> Vec<SupplierConfigEntry> {
        let map = self.config_map.read().unwrap();
        db_suppliers
            .iter()
            .map(|s| {
                let cfg = map.get(&s.code);
                let (term_type, term_days, order_type, payment_type, configured) = match cfg {
                    Some(c) => (
                        match c.term_type {
                            TermType::Eom => "EOM".to_string(),
                            TermType::NetDays => "NetDays".to_string(),
                        },
                        c.term_days,
                        match c.order_type {
                            OrderType::Weekly => "Weekly".to_string(),
                            OrderType::Monthly => "Monthly".to_string(),
                        },
                        match c.payment_type {
                            PaymentType::DirectCredit => "DC".to_string(),
                            PaymentType::DirectDebit => "DD".to_string(),
                        },
                        c.configured,
                    ),
                    None => ("EOM".to_string(), 20u16, "Monthly".to_string(), "DC".to_string(), false),
                };
                SupplierConfigEntry {
                    code: s.code.clone(),
                    label: s.label.clone(),
                    term_type,
                    term_days,
                    order_type,
                    payment_type,
                    configured,
                }
            })
            .collect()
    }

    /// Calculate the due date for an invoice based on its supplier's payment terms.
    /// Returns a formatted date string (YYYY-MM-DD).
    pub fn calculate_due_date(&self, supplier_code: &str, invoice_date_str: &str) -> String {
        let map = self.config_map.read().unwrap();
        let cfg = map
            .get(supplier_code)
            .cloned()
            .unwrap_or_else(|| TermConfig {
                term_type: TermType::Eom,
                term_days: 20,
                order_type: OrderType::Monthly,
                payment_type: PaymentType::DirectCredit,
                configured: false,
            });
        drop(map); // release read lock before heavy work

        // Parse the invoice date — try common formats
        let inv_date = match Self::parse_date(invoice_date_str) {
            Some(d) => d,
            None => return "???".to_string(),
        };

        let due_date = match cfg.term_type {
            TermType::Eom => {
                // Last day of invoice month + term_days
                let next_month = if inv_date.month() == 12 {
                    chrono::NaiveDate::from_ymd_opt(inv_date.year() + 1, 1, 1)
                } else {
                    chrono::NaiveDate::from_ymd_opt(inv_date.year(), inv_date.month() + 1, 1)
                };
                match next_month {
                    Some(nd) => {
                        let eom = nd.pred_opt().unwrap_or(inv_date);
                        eom + chrono::Duration::days(cfg.term_days as i64)
                    }
                    None => inv_date + chrono::Duration::days(cfg.term_days as i64),
                }
            }
            TermType::NetDays => inv_date + chrono::Duration::days(cfg.term_days as i64),
        };

        due_date.format("%Y-%m-%d").to_string()
    }

    /// Number of suppliers in the config
    pub fn count(&self) -> usize {
        self.config_map.read().unwrap().len()
    }

    fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
        let s = s.trim();
        // Try YYYY-MM-DD HH:MM:SS.mmm
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
            return Some(dt.date());
        }
        // Try YYYY-MM-DD
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Some(d);
        }
        // Try DD/MM/YYYY
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%d/%m/%Y") {
            return Some(d);
        }
        // Try MM/DD/YYYY
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%m/%d/%Y") {
            return Some(d);
        }
        None
    }
}
