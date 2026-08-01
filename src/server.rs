use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::db::{self, DbPool};
use crate::paid_ledger::PaidLedger;
use crate::supplier_config::{SupplierConfigEntry, SupplierConfigManager};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/api/suppliers").route(web::get().to(get_suppliers)))
        .service(web::resource("/api/branches").route(web::get().to(get_branches)))
        .service(web::resource("/api/invoices").route(web::get().to(get_invoices)))
        .service(web::resource("/api/supplier-config").route(web::get().to(get_supplier_config))
                                                      .route(web::post().to(save_supplier_config)))
        .service(web::resource("/api/supplier-config/bulk").route(web::post().to(save_supplier_config_bulk)))
        .service(web::resource("/api/paid").route(web::get().to(get_paid)))
        .service(web::resource("/api/pay").route(web::post().to(mark_paid)));
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
    #[serde(default = "default_order_type")]
    order_type: String,
    #[serde(default = "default_payment_type")]
    payment_type: String,
}

fn default_order_type() -> String {
    "Monthly".to_string()
}

fn default_payment_type() -> String {
    "DC".to_string()
}

async fn save_supplier_config(
    supplier_cfg: web::Data<SupplierConfigManager>,
    body: web::Json<SaveOneConfig>,
) -> HttpResponse {
    let cfg = supplier_cfg.into_inner();
    let req = body.into_inner();

    match cfg.save_one(
        &req.supplier_code,
        &req.term_type,
        req.term_days,
        &req.order_type,
        &req.payment_type,
    ) {
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

// POST /api/pay — mark selected invoices as paid
#[derive(Deserialize)]
pub struct PayRow {
    pub branch: String,
    pub supplier_code: String,
    pub invoice_number: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Deserialize)]
pub struct PayRequest {
    pub rows: Vec<PayRow>,
}

async fn mark_paid(
    ledger: web::Data<PaidLedger>,
    body: web::Json<PayRequest>,
) -> HttpResponse {
    let req = body.into_inner();
    let rows: Vec<PayRow> = req.rows.into_iter().filter(|r| !r.invoice_number.is_empty()).collect();

    if rows.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "status": "error",
            "message": "No invoices selected to mark as paid"
        }));
    }

    match ledger.mark_paid(&rows) {
        Ok(added) => {
            println!("✅ Marked {} invoices as paid", added);
            HttpResponse::Ok().json(serde_json::json!({
                "status": "ok",
                "message": format!("Marked {} invoices as paid", added),
                "rows": added,
            }))
        }
        Err(e) => {
            eprintln!("Failed to mark paid: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to mark paid: {}", e)
            }))
        }
    }
}

// GET /api/paid — return the set of paid invoice keys
async fn get_paid(ledger: web::Data<PaidLedger>) -> HttpResponse {
    let keys: Vec<String> = ledger.paid_keys().into_iter().collect();
    HttpResponse::Ok().json(serde_json::json!({ "paid": keys }))
}
