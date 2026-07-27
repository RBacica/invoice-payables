# Invoice Payables — Build Plan

## Overview

A browser-based accounts-payable review application. Select a month, see all invoices with calculated due dates, and visually determine what needs paying. Follows the same Rust + Actix-Web + single-page HTML architecture as stocktake-v3.

## Database

MS SQL Server (Infinity Back Office) — same DB the stocktake app connects to.

### Tables Referenced

| Table | What For |
|-------|----------|
| `APInv` | Invoice data: Branch, SupplierCode, InvoiceNumber, Description, InvoiceDate, InvoiceAmount, PONumber, TaxAmount, Logged |
| `Branches` | Branch list (excludes Head Office via `IsHO = '0'`) |
| `Customers` | Supplier list (`CustType = 'R'`, `InActive = '0'`) |

### Key Filtering

- Invoices filtered by date range (month)
- Can optionally filter by branch and/or supplier
- Date params supplied by the month picker in the UI
- **Month picker defaults to the previous month** on first load

---

## Phase 1 — Project Skeleton & Config

| Step | Detail |
|------|--------|
| 1.1 | `cargo init` — name `invoice-payables`, create `Cargo.toml` with same deps as stocktake-v3 (actix-web, tiberius, deadpool-tiberius, serde, toml, chrono, serde_json, etc.) |
| 1.2 | Config module (`config.rs`) — load `config.toml` with `[server]` (host, port) and `[database]` (connection_string, output_dir) |
| 1.3 | Main entry (`main.rs`) — anchor CWD to exe dir, load config, init DB pool, bind HTTP server |

---

## Phase 2 — Database Layer (`db.rs`)

| Step | Detail |
|------|--------|
| 2.1 | Connection pool using deadpool-tiberius (same `build_manager` from stocktake) |
| 2.2 | **`get_suppliers()`** — returns `Vec<Supplier>` from `Customers` table `CustType='R'` + `InActive='0'`, formatted as `Code - LastName (FirstName)` |
| 2.3 | **`get_branches()`** — returns `Vec<Branch>` from `Branches` where `IsHO = '0'` |
| 2.4 | **`get_invoices(from_date, to_date, branch?, supplier?)`** — queries `APInv` filtered by date range, with optional branch and supplier filters. Returns `Vec<Invoice>` |

### Data Structures

```
Supplier { code: String, label: String }
Branch { id: String, name: String }
Invoice {
  branch: String,
  supplier_code: String,
  invoice_number: String,
  description: String,
  invoice_date: NaiveDateTime,
  invoice_amount: f64,
  po_number: String,
  tax_amount: f64,
  logged: NaiveDateTime,
  due_date: NaiveDateTime,   // ← Calculated from supplier payment terms
}
```

---

## Phase 3 — Supplier Config Module (`supplier_config.rs`)

Manages a persistent supplier payment-terms config file in the working directory.

### Config File Format

Stored as `suppliers-config.toml` in the application's working directory.

```toml
# Each supplier has a payment term setting
[001]
term_type = "EOM"       # "NetDays" or "EOM"
term_days = 20          # number of days

[010]
term_type = "NetDays"
term_days = 30
```

### Default Behavior

- **If no config file exists** — create it with all suppliers, each defaulting to `EOM 20` but marked as `configured = false`
- **If config file exists but some suppliers are missing** — add new suppliers with defaults, existing ones keep their saved values
- **Unconfigured suppliers** — display "Not Set" in the settings UI; runtime fallback = EOM 20

### Logic

| Method | What It Does |
|--------|-------------|
| `load_or_create(path, full_supplier_list)` | Loads existing config, merges with current DB supplier list (adds new, keeps existing), writes back, returns `HashMap<code, TermConfig>` |
| `save(path, config_map)` | Writes the full config to `suppliers-config.toml` |
| `calculate_due_date(invoice_date, term_config)` | Returns the due date for a given invoice date based on terms |

### Due Date Calculation

- **NetDays + NNN** → `invoice_date + NNN days`
- **EOM + NNN** → `last day of invoice_date's month + NNN days`
  - e.g. Invoice 15 Feb → 28/29 Feb + 20 days = 20/21 Mar

---

## Phase 4 — API Layer (`server.rs`)

| Endpoint | Method | Params | Returns |
|----------|--------|--------|---------|
| `/api/suppliers` | GET | — | `[Supplier]` |
| `/api/branches` | GET | — | `[Branch]` |
| `/api/invoices` | GET | `from`, `to`, `branch?`, `supplier?` | `[Invoice]` with calculated `due_date` |
| `/api/supplier-config` | GET | — | `{ suppliers: [{ code, label, term_type, term_days, configured }] }` |
| `/api/supplier-config` | POST | `{ supplier_code, term_type, term_days }` | Save one supplier's terms |
| `/api/supplier-config/bulk` | POST | `{ suppliers: [{ code, term_type, term_days }] }` | Save all supplier configs at once |
| `/api/save` | POST | `{ rows: [...] }` | Save selected payables list |

### Invoice Due Date — Backend Computation

When `GET /api/invoices` returns results, each invoice's `due_date` is computed on the server using the loaded supplier config. This keeps the calculation in one place and ensures consistency.

---

## Phase 5 — Frontend (`web/index.html`)

### Navigation

Two views accessible via a nav bar or tab system:
1. **Invoice Search** (main page, default)
2. **Settings** (supplier payment terms config)

### 5A — Invoice Search Page

```
┌─ Nav: [🔍 Invoice Search] [⚙ Settings] ──┐
├─ Month Picker ──────────────────────────────┤
│  [Month ▼] [Year ▼]  [Search]               │
│  Defaults to previous month on first load    │
├─ Filters ────────────────────────────────────┤
│  Branch: [All ▼]    Supplier: [All ▼]        │
├─ Summary Bar ─────────────────────────────────┤
│  Invoices: 47   Total $24,582.30             │
│  Tax $2,150.40   Selected $18,400.00         │
├─ Invoice Table ───────────────────────────────┤
│  ☐ | Branch | Supplier | Inv# | Date         │
│     | Desc | Amount | Tax | PO#              │
│     | Invoice Due | Status                    │
│  ☐ | ...                                      │
│  ☐ | ...                                      │
├─ Toolbar ─────────────────────────────────────┤
│  [Select All] [Clear All] [Export Payables]   │
└───────────────────────────────────────────────┘
```

#### Invoice Due Column

- Displays the calculated due date based on the supplier's payment terms
- Uses the supplier config loaded at app start
- If a supplier has no configured terms → falls back to EOM + 20 days but shows a subtle indicator that terms are unconfigured (e.g. italicised or an asterisk)
- Format: same date format as the Invoice Date column

#### Key UI Components/Behaviors

| Component | Behavior |
|-----------|----------|
| **Month picker** | Dropdown pairs (month + year). **Defaults to previous month**. |
| **Branch filter** | Populated from `/api/branches`. "All Branches" default. |
| **Supplier filter** | Populated from `/api/suppliers`. "All Suppliers" default. |
| **Invoice table** | Columns: checkbox, Branch, Supplier, Invoice#, Date, Description, Amount, Tax, PO#, Invoice Due, Status. Sortable by column. |
| **Invoice Due** | Server-calculated from supplier terms. Fallback = EOM+20 for unconfigured suppliers. |
| **Checkbox** | Select/deselect for payment. Select All / Clear All in toolbar. |
| **Summary bar** | Live-updating: total invoices count, sum of amounts, sum of tax, sum of selected amounts. |
| **Status indicator** | Visual badge for each invoice: "To Pay" (green), "Skip" (grey), "Review" (amber) |
| **Export** | Saves selected invoices to a timestamped file with full details including due date. |

### 5B — Settings Page

```
┌─ Nav: [🔍 Invoice Search] [⚙ Settings] ──┐
├─ Supplier Payment Terms ─────────────────────┤
│  Filter: [_______________]                    │
│                                               │
│  ┌── Supplier ─────────┬── Term Type ─┬── Days ─┬── Actions ─┐ │
│  │ 001 Tasman          │ [EOM ▼]      │ [20]    │ [✓ Save]   │ │
│  │                     │  (Not Set)    │         │            │ │
│  ├─────────────────────┼──────────────┼─────────┼─────────────┤ │
│  │ 010 Some Supplier   │ [NetDays ▼]  │ [30]    │ [✓ Save]   │ │
│  ├─────────────────────┼──────────────┼─────────┼─────────────┤ │
│  │ 015 New Supplier    │ [EOM ▼]      │ [20]    │ [✓ Save]   │ │
│  │                     │  (Not Set*)   │         │            │ │
│  └─────────────────────┴──────────────┴─────────┴─────────────┘ │
│                                               │
│  [Bulk Save All]                              │
└───────────────────────────────────────────────┘
```

| Component | Behavior |
|-----------|----------|
| **Supplier list** | All active suppliers from the DB, merged with saved config |
| **Term Type** | Dropdown: "EOM" or "NetDays" — defaults to "EOM" for new/unconfigured |
| **Days field** | Number input (0-999) — defaults to 20 for new/unconfigured |
| **Not Set indicator** | Unconfigured suppliers show "(Not Set)" in muted text next to the defaults. Once saved, this disappears. |
| **Per-row Save** | Saves individual supplier via `POST /api/supplier-config` |
| **Bulk Save All** | Saves all visible suppliers at once via `POST /api/supplier-config/bulk` |
| **Filter** | Text filter to narrow the supplier list (searches code + name) |

### States to Handle

- **Loading** — spinner while fetching invoices/suppliers/branches/settings
- **Empty** — "No invoices found for this period" with link to try different month
- **Error** — DB connection failure with "Check connection string" message
- **No month selected (main page)** — prompt: "Select a month and click Search"
- **Settings saved** — brief success flash/checkmark animation
- **Settings error** — "Failed to save config" with retry button

---

## Phase 6 — Save / Export

| File | Format | Content |
|------|--------|---------|
| Payables list | `payables-YYYY-MM-DD-HHMMSS.txt` | Tab-separated: Branch, Supplier, Invoice#, Date, Description, Amount, Tax, PO#, Invoice Due, Status |
| Full report (optional) | `payables-report-YYYY-MM-DD-HHMMSS.txt` | Includes summary totals at the top, due date grouping |

Each exported row includes the due date so the operator knows when each payment is due.

---

## Phase 7 — Build & Cross-Compile

| Step | Detail |
|------|--------|
| 7.1 | `build.rs` — embed Windows version metadata via `winres` (same as stocktake) |
| 7.2 | Cross-compile for `x86_64-pc-windows-gnu` via Rust cross-toolchain |
| 7.3 | Zip release: `InvoicePayables-YYYYMMDD_HHMMSS.zip` containing: exe, config.toml (template), web/ folder |
| 7.4 | Store zips in `~/RussellShared/HermesFiles/HermesOutput/` |

---

## File Structure (Final)

```
invoice-payables/
├── Cargo.toml
├── build.rs
├── config.toml              ← Example config (no real password)
├── suppliers-config.toml    ← Created at runtime, supplier payment terms
├── src/
│   ├── main.rs              ← Entry point, server setup, supplier-config init
│   ├── config.rs            ← Config loading
│   ├── db.rs                ← DB queries + pool
│   ├── server.rs            ← HTTP handlers + due-date calculation
│   └── supplier_config.rs   ← Supplier payment-terms config (load/save/merge/calc)
├── web/
│   ├── index.html           ← SPA entry (nav shell)
│   ├── search.html          ← Invoice search page
│   └── settings.html        ← Supplier config page
│   (or single index.html with view-switching via JS)
└── BUILD_PLAN.md            ← This document
```

---

## Build Order (Execution Priority)

1. **Phase 1** — cargo init, deps, config.rs, main.rs skeleton
2. **Phase 2** — db.rs: pool, queries (suppliers, branches, invoices)
3. **Phase 3** — `supplier_config.rs`: load/save/merge/calculate modules
4. **Phase 4** — server.rs: all API endpoints, wire supplier config into invoice queries
5. **Phase 5** — web/index.html: full frontend (search page + settings page)
6. **Phase 6** — save/export logic
7. **Phase 7** — cross-compile + zip

Each phase builds on the previous — verify compilation after phases 1-4, verify UI after 5, verify file output after 6.