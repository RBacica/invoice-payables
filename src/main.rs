mod config;
mod db;
mod server;
mod supplier_config;

use actix_files::Files;
use actix_web::{middleware::Logger, web, App, HttpServer};
use std::io::Write;

#[actix_web::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!();
        eprintln!("❌ Invoice Payables failed to start:");
        eprintln!("   {}", e);
        eprintln!();
        pause_before_exit();
        std::process::exit(1);
    }
}

fn pause_before_exit() {
    print!("Press Enter to close this window... ");
    let _ = std::io::stdout().flush();
    let mut _buf = String::new();
    let _ = std::io::stdin().read_line(&mut _buf);
}

fn anchor_cwd_to_exe() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = std::env::set_current_dir(dir);
        }
    }
}

async fn run() -> std::io::Result<()> {
    anchor_cwd_to_exe();

    // 1) Load config
    let cfg = config::load()?;

    if cfg.connection_string.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "connection_string is empty. Edit config.toml in \
             the same folder as this program and set [database] connection_string \
             to your SQL Server details, then run it again.",
        ));
    }

    // 2) Build DB pool
    let pool = db::DbPool::new(&cfg.connection_string).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to build database pool: {}", e),
        )
    })?;

    // 3) Load / initialise supplier config
    let supplier_cfg = supplier_config::SupplierConfigManager::new(&pool).await.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to initialise supplier config: {}", e),
        )
    })?;

    println!("🚀 Invoice Payables starting...");
    println!("   📡 Listening on: http://{}:{}", cfg.host, cfg.port);
    println!("   🗄️  Database: configured");
    println!("   💾 Save directory: {}", cfg.output_dir);
    println!("   📋 Supplier config: {} suppliers loaded", supplier_cfg.count());

    let pool_data = web::Data::new(pool);
    let output_dir = web::Data::new(server::OutputDir(cfg.output_dir.clone()));
    let supplier_cfg_data = web::Data::new(supplier_cfg);

    println!("✅ Server ready. Open the address above in a browser.");
    println!("   (Database connects on first use — the UI loads even if SQL Server is down.)");

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(pool_data.clone())
            .app_data(output_dir.clone())
            .app_data(supplier_cfg_data.clone())
            .configure(server::configure)
            .service(Files::new("/", "web").index_file("index.html"))
    })
    .bind(format!("{}:{}", cfg.host, cfg.port))?
    .run()
    .await
}
