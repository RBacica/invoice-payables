use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::db::{self, DbPool};
use crate::supplier_config::{SupplierConfigEntry, SupplierConfigManager};

/// Output directory for saved payables reports.
#[derive(Clone)]
pub struct OutputDir(pub String);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/api/suppliers").route(web::get().to(get_suppliers)))
        .service(web::resource("/api/branches").route(web::get().to(get_branches)))
        .service(web::resource("/api/invoices").route(web::get().to(get_invoices)))
        .service(web::resource("/api/supplier-config").route(web::get().to(get_supplier_config))
                                                      .route(web::post().to(save_supplier_config)))
        .service(web::resource("/api/supplier-config/bulk").route(web::post().to(save_supplier_config_bulk)))
        .service(web::resource("/api/save").route(web::post().to(save_payables)));
}

// ── Handlers ───────────────────────────────────────────────────

async fn get_suppliers(pool: web::Data<DbPool>) -> HttpResponse {
    match pool.get_suppliers().await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(e) => {
            eprintln!("Failed to get suppliers: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error: {}", e)
            }))
        }
    }
}

async fn get_branches(pool: web::Data<DbPool>) -> HttpResponse {
    match pool.get_branches().await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(e) => {
            eprintln!("Failed to get branches: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error: {}", e)
            }))
        }
    }
}

// GET /api/invoices?from=YYYY-MM-DD&to=YYYY-MM-DD&branch=&supplier=
async fn get_invoices(
    pool: web::Data<DbPool>,
    supplier_cfg: web::Data<SupplierConfigManager>,
    query: web::Query<db::InvoiceQuery>,
) -> HttpResponse {
    let from = query.from.clone().unwrap_or_default();
    let to = query.to.clone().unwrap_or_default();
    let branch = query.branch.clone().unwrap_or_else(|| "ALL".to_string());
    let supplier = query.supplier.clone().unwrap_or_else(|| "ALL".to_string());

    if from.is_empty() || to.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Missing 'from' and 'to' query parameters (YYYY-MM-DD format)"
        }));
    }

    match pool.get_invoices(&from, &to, &branch, &supplier).await {
        Ok(mut invoices) => {
            // Calculate due dates for each invoice
            for inv in &mut invoices {
                inv.due_date = supplier_cfg.calculate_due_date(&inv.supplier_code, &inv.invoice_date);
            }
            HttpResponse::Ok().json(invoices)
        }
        Err(e) => {
            eprintln!("Failed to get invoices: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error: {}", e)
            }))
        }
    }
}

// GET /api/supplier-config — returns full supplier list with config
async fn get_supplier_config(
    pool: web::Data<DbPool>,
    supplier_cfg: web::Data<SupplierConfigManager>,
) -> HttpResponse {
    match pool.get_suppliers().await {
        Ok(suppliers) => {
            let entries = supplier_cfg.get_all_entries(&suppliers);
            HttpResponse::Ok().json(entries)
        }
        Err(e) => {
            eprintln!("Failed to get suppliers for config: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error: {}", e)
            }))
        }
    }
}

// POST /api/supplier-config — save one supplier's terms
#[derive(Deserialize)]
struct SaveOneConfig {
    supplier_code: String,
    term_type: String,
    term_days: u16,
}

async fn save_supplier_config(
    supplier_cfg: web::Data<SupplierConfigManager>,
    body: web::Json<SaveOneConfig>,
) -> HttpResponse {
    let cfg = supplier_cfg.into_inner();
    let req = body.into_inner();

    match cfg.save_one(&req.supplier_code, &req.term_type, req.term_days) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })),
        Err(e) => {
            eprintln!("Failed to save supplier config: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to save config: {}", e)
            }))
        }
    }
}

// POST /api/supplier-config/bulk — save all supplier configs at once
#[derive(Deserialize)]
struct BulkSaveConfig {
    suppliers: Vec<SupplierConfigEntry>,
}

async fn save_supplier_config_bulk(
    supplier_cfg: web::Data<SupplierConfigManager>,
    body: web::Json<BulkSaveConfig>,
) -> HttpResponse {
    let cfg = supplier_cfg.into_inner();
    let req = body.into_inner();

    match cfg.save_all(&req.suppliers) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })),
        Err(e) => {
            eprintln!("Failed to bulk save supplier config: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to save config: {}", e)
            }))
        }
    }
}

// POST /api/save — save the selected payables rows
#[derive(Deserialize)]
pub struct SaveRow {
    pub branch: String,
    pub supplier_code: String,
    pub invoice_number: String,
    pub description: String,
    pub invoice_date: String,
    pub invoice_amount: f64,
    pub po_number: String,
    pub tax_amount: f64,
    pub due_date: String,
    pub selected: bool,
}

#[derive(Deserialize)]
pub struct SaveRequest {
    pub rows: Vec<SaveRow>,
}

async fn save_payables(
    output_dir: web::Data<OutputDir>,
    body: web::Json<SaveRequest>,
) -> HttpResponse {
    let req = body.into_inner();
    let selected: Vec<SaveRow> = req.rows.into_iter().filter(|r| r.selected).collect();

    if selected.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "status": "error",
            "message": "No rows selected to save"
        }));
    }

    match db::save_payables_report(&output_dir.0, &selected) {
        Ok(path) => {
            let path_str = path.display().to_string();
            println!("💾 Saved {} payable rows to {}", selected.len(), path_str);
            HttpResponse::Ok().json(serde_json::json!({
                "status": "ok",
                "message": format!("Saved {} payables", selected.len()),
                "rows": selected.len(),
                "file": path_str,
            }))
        }
        Err(e) => {
            eprintln!("Failed to save payables: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to save: {}", e)
            }))
        }
    }
}
