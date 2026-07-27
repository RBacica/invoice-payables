use deadpool_tiberius::Manager;
use futures_util::StreamExt;
use tiberius::{EncryptionLevel, QueryItem};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Data Structures ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Supplier {
    pub code: String,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Branch {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Invoice {
    pub branch: String,
    pub supplier_code: String,
    pub invoice_number: String,
    pub description: String,
    pub invoice_date: String,
    pub invoice_amount: f64,
    pub po_number: String,
    pub tax_amount: f64,
    pub logged: String,
    pub due_date: String,
}

#[derive(Debug, Deserialize)]
pub struct InvoiceQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub branch: Option<String>,
    pub supplier: Option<String>,
}

// ── DB Error ───────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Query failed: {0}")]
    Query(String),
}

// ── Cell coercion helpers ──────────────────────────────────────

fn cell_to_string(row: &tiberius::Row, idx: usize) -> String {
    if let Ok(Some(s)) = row.try_get::<&str, _>(idx) {
        return s.trim().to_string();
    }
    if let Ok(Some(v)) = row.try_get::<i32, _>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<i16, _>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<i64, _>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<u8, _>(idx) {
        return v.to_string();
    }
    String::new()
}

fn cell_to_f64(row: &tiberius::Row, idx: usize) -> f64 {
    if let Ok(Some(v)) = row.try_get::<f64, _>(idx) {
        return v;
    }
    if let Ok(Some(v)) = row.try_get::<f32, _>(idx) {
        return v as f64;
    }
    if let Ok(Some(v)) = row.try_get::<i64, _>(idx) {
        return v as f64;
    }
    if let Ok(Some(v)) = row.try_get::<i32, _>(idx) {
        return v as f64;
    }
    if let Ok(Some(v)) = row.try_get::<i16, _>(idx) {
        return v as f64;
    }
    if let Ok(Some(s)) = row.try_get::<&str, _>(idx) {
        if let Ok(v) = s.trim().parse::<f64>() {
            return v;
        }
    }
    0.0
}

// ── Connection Manager ─────────────────────────────────────────

pub fn build_manager(conn_string: &str) -> Result<Manager, DbError> {
    let lower = conn_string.to_lowercase();

    if lower.contains("driver=") || lower.contains("server={") {
        return Manager::from_ado_string(conn_string)
            .map_err(|e| DbError::Connection(e.to_string()));
    }

    let mut manager = Manager::new();
    let mut username: Option<String> = None;
    let mut password: Option<String> = None;

    for pair in conn_string.split(';') {
        let pair = pair.trim();
        if pair.is_empty() { continue; }
        let kv: Vec<&str> = pair.splitn(2, '=').collect();
        if kv.len() != 2 { continue; }
        let key = kv[0].trim().to_lowercase();
        let val = kv[1].trim();
        match key.as_str() {
            "server" | "host" => {
                let s = val.trim_start_matches("tcp:").trim();
                if let Some(comma_pos) = s.find(',') {
                    let host = s[..comma_pos].trim().trim_matches('[').trim_matches(']').to_string();
                    let port: u16 = s[comma_pos+1..].trim().parse().unwrap_or(1433);
                    manager = manager.host(host).port(port);
                } else {
                    manager = manager.host(s.trim_matches('[').trim_matches(']'));
                }
            }
            "port" => { manager = manager.port(val.parse().unwrap_or(1433)); }
            "uid" | "user" | "username" => { username = Some(val.to_string()); }
            "pwd" | "password" => { password = Some(val.to_string()); }
            "database" | "db" => { manager = manager.database(val); }
            "encrypt" => {
                let on = val.to_lowercase() != "off" && val.to_lowercase() != "false" && val.to_lowercase() != "no";
                manager = manager.encryption(if on { EncryptionLevel::Required } else { EncryptionLevel::Off });
            }
            "trust_cert" => {
                if val.to_lowercase() == "true" || val.to_lowercase() == "yes" {
                    manager = manager.trust_cert();
                }
            }
            _ => {}
        }
    }

    if let (Some(u), Some(p)) = (username, password) {
        manager = manager.basic_authentication(u, p);
    }

    Ok(manager)
}

// ── DB Pool ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DbPool {
    pool: deadpool_tiberius::Pool,
}

impl DbPool {
    pub fn new(conn_string: &str) -> Result<Self, DbError> {
        let manager = build_manager(conn_string)?;
        let pool = manager
            .max_size(10)
            .trust_cert()
            .create_pool()
            .map_err(|e| DbError::Connection(e.to_string()))?;
        Ok(Self { pool })
    }

    pub async fn get_suppliers(&self) -> Result<Vec<Supplier>, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Connection(e.to_string()))?;
        let mut stream = conn.query(
            "SELECT [Code], [LastName], [FirstName] FROM [Customers] WHERE [CustType] = 'R' AND [InActive] = '0' ORDER BY [LastName] ASC",
            &[],
        ).await.map_err(|e| DbError::Query(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let item = item.map_err(|e| DbError::Query(e.to_string()))?;
            if let QueryItem::Row(row) = item {
                let code = cell_to_string(&row, 0);
                if code.is_empty() { continue; }
                let last_name = cell_to_string(&row, 1);
                let first_name = cell_to_string(&row, 2);
                let label = match (last_name.is_empty(), first_name.is_empty()) {
                    (false, false) => format!("{} - {} ({})", code, last_name, first_name),
                    (false, true) => format!("{} - {}", code, last_name),
                    (true, false) => format!("{} - ({})", code, first_name),
                    (true, true) => code.clone(),
                };
                results.push(Supplier { code, label });
            }
        }
        Ok(results)
    }

    pub async fn get_branches(&self) -> Result<Vec<Branch>, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Connection(e.to_string()))?;
        let mut stream = conn.query(
            "SELECT [ID], [Name] FROM [Branches] WHERE [IsHO] = '0' ORDER BY [Name] ASC",
            &[],
        ).await.map_err(|e| DbError::Query(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let item = item.map_err(|e| DbError::Query(e.to_string()))?;
            if let QueryItem::Row(row) = item {
                let id = cell_to_string(&row, 0);
                if id.is_empty() { continue; }
                let name = cell_to_string(&row, 1);
                results.push(Branch { id, name });
            }
        }
        Ok(results)
    }

    pub async fn get_invoices(
        &self,
        from_date: &str,
        to_date: &str,
        branch: &str,
        supplier: &str,
    ) -> Result<Vec<Invoice>, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Connection(e.to_string()))?;

        let safe_from = from_date.replace('\'', "''");
        let safe_to = to_date.replace('\'', "''");

        let branch_clause = if !branch.is_empty() && branch != "ALL" {
            format!(" AND [Branch] = '{}'", branch.replace('\'', "''"))
        } else {
            String::new()
        };

        let supplier_clause = if !supplier.is_empty() && supplier != "ALL" {
            format!(" AND [SupplierCode] = '{}'", supplier.replace('\'', "''"))
        } else {
            String::new()
        };

        let query = format!(
            "SELECT [Branch], [SupplierCode], [InvoiceNumber], [Description], \
                    [InvoiceDate], [InvoiceAmount], [PONumber], [TaxAmount1], [Logged] \
             FROM [APInv] \
             WHERE [InvoiceDate] >= '{}' AND [InvoiceDate] < '{}' {} {} \
             ORDER BY [InvoiceDate] DESC",
            safe_from, safe_to, branch_clause, supplier_clause
        );

        let mut stream = conn.query(&query, &[])
            .await
            .map_err(|e| DbError::Query(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let item = item.map_err(|e| DbError::Query(e.to_string()))?;
            if let QueryItem::Row(row) = item {
                results.push(Invoice {
                    branch: cell_to_string(&row, 0),
                    supplier_code: cell_to_string(&row, 1),
                    invoice_number: cell_to_string(&row, 2),
                    description: cell_to_string(&row, 3),
                    invoice_date: cell_to_string(&row, 4),
                    invoice_amount: cell_to_f64(&row, 5),
                    po_number: cell_to_string(&row, 6),
                    tax_amount: cell_to_f64(&row, 7),
                    logged: cell_to_string(&row, 8),
                    // due_date will be filled in by the caller (server.rs)
                    due_date: String::new(),
                });
            }
        }
        Ok(results)
    }
}

/// Save selected payables to a timestamped .txt file.
pub fn save_payables_report(
    output_dir: &str,
    rows: &[crate::server::SaveRow],
) -> Result<std::path::PathBuf, std::io::Error> {
    use std::io::Write;

    std::fs::create_dir_all(output_dir)?;

    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d-%H-%M-%S");
    let fname = format!("payables-{}.txt", timestamp);
    let path = std::path::Path::new(output_dir).join(&fname);

    let mut out = String::with_capacity(rows.len() * 128 + 256);

    // Header
    out.push_str("Branch\tSupplier\tInvoice#\tDate\tDescription\tAmount\tTax\tPO#\tDue Date\tStatus\n");
    out.push_str(&format!("--- Payables Report — {} selected invoices — Generated {}\n\n",
        rows.len(), now.format("%Y-%m-%d %H:%M:%S")));

    // Totals
    let total_amount: f64 = rows.iter().map(|r| r.invoice_amount).sum();
    let total_tax: f64 = rows.iter().map(|r| r.tax_amount).sum();
    out.push_str(&format!("Total Amount:\t{:.2}\nTotal Tax:\t{:.2}\n\n", total_amount, total_tax));

    // Data rows
    for r in rows {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{}\t{}\tTo Pay\n",
            r.branch, r.supplier_code, r.invoice_number, r.invoice_date,
            r.description.replace('\t', " "), r.invoice_amount, r.tax_amount,
            r.po_number, r.due_date,
        ));
    }

    let mut f = std::fs::File::create(&path)?;
    f.write_all(out.as_bytes())?;
    println!("💾 Wrote payables report with {} rows", rows.len());

    Ok(path)
}
