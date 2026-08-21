//! Integration tests for apeireth-stock (post-1.0.0)
//!
//! src/ 5 module 真实现 (catalog/csv/refresh/store/symbol). 这里 (tests/) 加跨模块集成 + 边界.
//! 0 触碰 src/, 0 编造"已实现".

use apeireth_stock::{import_from_csv, Provenance, SymbolCatalog, SymbolMeta, SymbolStore};
use std::path::PathBuf;

// =============================================================================
// Provenance
// =============================================================================

#[test]
fn provenance_default_is_manual() {
    assert_eq!(Provenance::default(), Provenance::Manual);
}

#[test]
fn provenance_as_str_round_trip() {
    for p in [Provenance::FinanceDatabase, Provenance::Manual] {
        let s = p.as_str();
        let back = Provenance::from_db(s);
        assert_eq!(p, back);
    }
}

#[test]
fn provenance_from_db_unknown_falls_back_manual() {
    assert_eq!(Provenance::from_db("garbage"), Provenance::Manual);
    assert_eq!(Provenance::from_db(""), Provenance::Manual);
    assert_eq!(Provenance::from_db("unknown"), Provenance::Manual);
}

#[test]
fn provenance_eq_copy_eq_set() {
    let p = Provenance::FinanceDatabase;
    let p2 = p; // Copy
    assert_eq!(p, p2);
    // Provenance 不派生 Hash, 但 Eq + Copy. 用 Vec + sort/dedup 替代 HashSet 测试唯一性
    let mut v = vec![p, p2, Provenance::Manual, Provenance::Manual];
    v.sort_by_key(|x| *x as u8);
    v.dedup();
    assert_eq!(v.len(), 2, "FinanceDatabase + Manual");
}

#[test]
fn provenance_serde_snake_case() {
    let s = serde_json::to_string(&Provenance::FinanceDatabase).unwrap();
    assert_eq!(s, "\"finance_database\"");
    let s = serde_json::to_string(&Provenance::Manual).unwrap();
    assert_eq!(s, "\"manual\"");
}

// =============================================================================
// SymbolMeta
// =============================================================================

#[test]
fn symbol_meta_default_all_empty() {
    let m = SymbolMeta::default();
    assert!(m.symbol.is_empty());
    assert!(m.name.is_empty());
    assert!(m.sector.is_empty());
    assert!(m.industry.is_empty());
    assert!(m.exchange.is_empty());
    assert!(m.country.is_empty());
    assert!(m.currency.is_empty());
    assert!(m.market_cap.is_none());
    assert!(m.ipo_year.is_none());
    assert!(m.ipo_date.is_none());
    assert!(m.delisted_date.is_none());
    assert_eq!(m.provenance, Provenance::Manual);
}

#[test]
fn symbol_meta_is_valid_requires_symbol() {
    let mut m = SymbolMeta::default();
    assert!(!m.is_valid());
    m.symbol = "AAPL".into();
    assert!(m.is_valid());
    m.symbol = "   ".into();
    assert!(!m.is_valid(), "whitespace-only 也无效");
    m.symbol = "\t\n".into();
    assert!(!m.is_valid());
}

#[test]
fn symbol_meta_ticker_returns_symbol() {
    let m = SymbolMeta {
        symbol: "BRK.B".into(),
        ..SymbolMeta::default()
    };
    assert_eq!(m.ticker(), "BRK.B");
}

#[test]
fn symbol_meta_to_row_13_fields() {
    let m = SymbolMeta {
        symbol: "AAPL".into(),
        name: "Apple".into(),
        market_cap: Some(100.0),
        ipo_year: Some(1980),
        ipo_date: Some("1980-12-12".into()),
        delisted_date: None,
        ..SymbolMeta::default()
    };
    let row = m.to_row();
    assert_eq!(row.len(), 13);
}

#[test]
fn symbol_meta_to_row_null_for_missing() {
    let m = SymbolMeta::default();
    let row = m.to_row();
    assert_eq!(row.len(), 13);
    use rusqlite::types::Value;
    assert_eq!(row[7], Value::Null, "market_cap None → Null");
    assert_eq!(row[8], Value::Null, "ipo_year None → Null");
    assert_eq!(row[9], Value::Null, "ipo_date None → Null");
    assert_eq!(row[10], Value::Null, "delisted_date None → Null");
}

#[test]
fn symbol_meta_serde_roundtrip() {
    let m = SymbolMeta {
        symbol: "AAPL".into(),
        name: "Apple".into(),
        market_cap: Some(2.9e12),
        ipo_year: Some(1980),
        ipo_date: Some("1980-12-12".into()),
        delisted_date: Some("2099-01-01".into()),
        provenance: Provenance::FinanceDatabase,
        last_updated_ms: 1_700_000_000_000,
        ..SymbolMeta::default()
    };
    let s = serde_json::to_string(&m).unwrap();
    let back: SymbolMeta = serde_json::from_str(&s).unwrap();
    assert_eq!(back, m);
}

#[test]
fn symbol_meta_clone_eq() {
    let m = SymbolMeta {
        symbol: "X".into(),
        ..SymbolMeta::default()
    };
    let m2 = m.clone();
    assert_eq!(m, m2);
}

// =============================================================================
// SymbolStore - lifecycle
// =============================================================================

#[test]
fn store_in_memory_empty() {
    let s = SymbolStore::open_in_memory().unwrap();
    assert_eq!(s.count(), 0);
    assert_eq!(s.count_all(), 0);
}

#[test]
fn store_open_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let p: PathBuf = dir.path().join("store.db");
    let s = SymbolStore::open(&p).unwrap();
    s.upsert(&SymbolMeta {
        symbol: "A".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    drop(s);
    let s2 = SymbolStore::open(&p).unwrap();
    assert!(s2.get("A").is_some());
}

#[test]
fn store_migration_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let p: PathBuf = dir.path().join("idem.db");
    let _ = SymbolStore::open(&p).unwrap();
    let _ = SymbolStore::open(&p).unwrap();
    let _ = SymbolStore::open(&p).unwrap();
}

// =============================================================================
// SymbolStore - upsert / get / delete
// =============================================================================

#[test]
fn store_upsert_and_get_round_trip() {
    let s = SymbolStore::open_in_memory().unwrap();
    let m = SymbolMeta {
        symbol: "AAPL".into(),
        name: "Apple Inc.".into(),
        sector: "Technology".into(),
        market_cap: Some(2.9e12),
        ipo_year: Some(1980),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    };
    s.upsert(&m).unwrap();
    let back = s.get("AAPL").unwrap();
    assert_eq!(back, m);
}

#[test]
fn store_upsert_overwrites_existing() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "X".into(),
        name: "old".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    s.upsert(&SymbolMeta {
        symbol: "X".into(),
        name: "new".into(),
        last_updated_ms: 2,
        ..SymbolMeta::default()
    })
    .unwrap();
    let back = s.get("X").unwrap();
    assert_eq!(back.name, "new");
    assert_eq!(back.last_updated_ms, 2);
}

#[test]
fn store_get_missing_returns_none() {
    let s = SymbolStore::open_in_memory().unwrap();
    assert!(s.get("NOTHERE").is_none());
}

#[test]
fn store_get_by_ticker_delegates() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "AAPL".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    assert_eq!(s.get_by_ticker("AAPL").unwrap().symbol, "AAPL");
    assert!(s.get_by_ticker("MISSING").is_none());
}

#[test]
fn store_delete_removes() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "A".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    assert!(s.get("A").is_some());
    s.delete("A").unwrap();
    assert!(s.get("A").is_none());
}

#[test]
fn store_delete_nonexistent_no_error() {
    let s = SymbolStore::open_in_memory().unwrap();
    // DELETE 不存在 不报错 (0 行 affected)
    assert!(s.delete("NOTHERE").is_ok());
}

// =============================================================================
// SymbolStore - insert_batch
// =============================================================================

#[test]
fn store_insert_batch_empty_noop() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.insert_batch(&[]).unwrap();
    assert_eq!(s.count(), 0);
}

#[test]
fn store_insert_batch_100() {
    let s = SymbolStore::open_in_memory().unwrap();
    let batch: Vec<SymbolMeta> = (0..100)
        .map(|i| SymbolMeta {
            symbol: format!("S{:03}", i),
            last_updated_ms: i as i64,
            ..SymbolMeta::default()
        })
        .collect();
    s.insert_batch(&batch).unwrap();
    assert_eq!(s.count(), 100);
}

#[test]
fn store_insert_batch_upsert() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "A".into(),
        name: "old".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    s.insert_batch(&[SymbolMeta {
        symbol: "A".into(),
        name: "new".into(),
        last_updated_ms: 2,
        ..SymbolMeta::default()
    }])
    .unwrap();
    let back = s.get("A").unwrap();
    assert_eq!(back.name, "new");
}

// =============================================================================
// SymbolStore - search
// =============================================================================

fn store_with_samples() -> SymbolStore {
    let s = SymbolStore::open_in_memory().unwrap();
    let metas = [
        ("AAPL", "Technology", Some(2.9e12)),
        ("MSFT", "Technology", Some(2.5e12)),
        ("JPM", "Financial", Some(500e9)),
        ("XOM", "Energy", Some(450e9)),
        ("NOC", "Aerospace", None), // no market cap
    ];
    for (s_, sec, mc) in metas {
        s.upsert(&SymbolMeta {
            symbol: s_.into(),
            name: format!("{s_} Inc."),
            sector: sec.into(),
            market_cap: mc,
            last_updated_ms: 1,
            ..SymbolMeta::default()
        })
        .unwrap();
    }
    s
}

#[test]
fn store_search_filter_sector() {
    let s = store_with_samples();
    let r = s.search(Some("Technology"), None, None, 10);
    assert_eq!(r.len(), 2);
    let symbols: Vec<&str> = r.iter().map(|m| m.symbol.as_str()).collect();
    assert!(symbols.contains(&"AAPL"));
    assert!(symbols.contains(&"MSFT"));
}

#[test]
fn store_search_filter_industry() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "A".into(),
        sector: "Tech".into(),
        industry: "Software".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    s.upsert(&SymbolMeta {
        symbol: "B".into(),
        sector: "Tech".into(),
        industry: "Hardware".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    let r = s.search(None, Some("Software"), None, 10);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].symbol, "A");
}

#[test]
fn store_search_filter_exchange() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "X".into(),
        exchange: "NYSE".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    s.upsert(&SymbolMeta {
        symbol: "Y".into(),
        exchange: "NASDAQ".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    let r = s.search(None, None, Some("NYSE"), 10);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].symbol, "X");
}

#[test]
fn store_search_combined_filters() {
    let s = store_with_samples();
    let r = s.search(Some("Technology"), None, None, 10);
    assert_eq!(r.len(), 2);
}

#[test]
fn store_search_orders_by_market_cap_desc() {
    let s = store_with_samples();
    let r = s.search(None, None, None, 10);
    // 第一个应有最大 market_cap
    assert_eq!(r[0].symbol, "AAPL");
    // 最后一个应是 None market_cap (NULLS LAST)
    assert_eq!(r.last().unwrap().symbol, "NOC");
}

#[test]
fn store_search_limit_truncates() {
    let s = store_with_samples();
    let r = s.search(None, None, None, 2);
    assert_eq!(r.len(), 2);
}

#[test]
fn store_search_by_industry_delegates() {
    let s = SymbolStore::open_in_memory().unwrap();
    s.upsert(&SymbolMeta {
        symbol: "A".into(),
        industry: "Software".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    s.upsert(&SymbolMeta {
        symbol: "B".into(),
        industry: "Hardware".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    let r = s.search_by_industry("Software", 10);
    assert_eq!(r.len(), 1);
}

#[test]
fn store_list_by_exchange_delegates() {
    let s = SymbolStore::open_in_memory().unwrap();
    for i in 0..3 {
        s.upsert(&SymbolMeta {
            symbol: format!("S{i}"),
            exchange: "NYSE".into(),
            last_updated_ms: 1,
            ..SymbolMeta::default()
        })
        .unwrap();
    }
    s.upsert(&SymbolMeta {
        symbol: "X".into(),
        exchange: "NASDAQ".into(),
        last_updated_ms: 1,
        ..SymbolMeta::default()
    })
    .unwrap();
    assert_eq!(s.list_by_exchange("NYSE", 10).len(), 3);
    assert_eq!(s.list_by_exchange("NASDAQ", 10).len(), 1);
    assert_eq!(s.list_by_exchange("OTHER", 10).len(), 0);
}

// =============================================================================
// SymbolCatalog trait
// =============================================================================

#[test]
fn catalog_trait_get_search_count() {
    let s = store_with_samples();
    let cat: &dyn SymbolCatalog = &s;
    assert!(cat.get("AAPL").is_some());
    assert!(cat.get("NOTHERE").is_none());
    let r = cat.search(Some("Technology"), None, None, 10);
    assert_eq!(r.len(), 2);
    assert_eq!(cat.count(), 5);
}

#[test]
fn catalog_trait_new_spec_methods() {
    let s = store_with_samples();
    let cat: &dyn SymbolCatalog = &s;
    assert_eq!(cat.count_all(), 5);
    // get_by_ticker / search_by_industry / list_by_exchange via trait
    assert!(cat.get_by_ticker("AAPL").is_some());
    let r = cat.search_by_industry("Aerospace", 10);
    // NOC sample has no industry set so it's empty string, not "Aerospace"
    // 仅验证方法可调
    let _ = r;
    let r = cat.list_by_exchange("NYSE", 10);
    let _ = r;
}

// =============================================================================
// CSV import
// =============================================================================

fn write_csv(content: &str) -> tempfile::NamedTempFile {
    let f = tempfile::NamedTempFile::new().unwrap();
    fs_err::write(f.path(), content).unwrap();
    f
}

#[test]
fn csv_import_basic_row() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               AAPL,Apple,Tech,Hardware,NASDAQ,US,USD,2900000000000,1980\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 1);
    assert_eq!(stats.skipped, 0);
    let back = s.get("AAPL").unwrap();
    assert_eq!(back.name, "Apple");
    assert_eq!(back.market_cap, Some(2.9e12));
    assert_eq!(back.ipo_year, Some(1980));
}

#[test]
fn csv_import_minimal_row() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               MSFT,,,,,,,,\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::Manual).unwrap();
    assert_eq!(stats.imported, 1);
    let back = s.get("MSFT").unwrap();
    assert!(back.name.is_empty());
    assert!(back.market_cap.is_none());
    assert_eq!(back.provenance, Provenance::Manual);
}

#[test]
fn csv_import_skips_empty_symbol() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               AAPL,A,,,,,,,\n\
               ,EmptyRow,,,,,,,\n\
               GOOG,G,,,,,,,\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 2);
    assert_eq!(stats.skipped, 1);
}

#[test]
fn csv_import_invalid_numeric_to_none() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               BAD,X,,,,,,not-a-num,not-a-year\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::Manual).unwrap();
    assert_eq!(stats.imported, 1);
    let back = s.get("BAD").unwrap();
    assert!(back.market_cap.is_none());
    assert!(back.ipo_year.is_none());
}

#[test]
fn csv_import_case_insensitive_headers() {
    let csv = "SYMBOL,NAME,SECTOR,INDUSTRY,EXCHANGE,COUNTRY,CURRENCY,MARKET_CAP,IPO_YEAR\n\
               TSLA,Tesla,Auto,Cars,NASDAQ,US,USD,800000000000,2010\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 1);
    assert_eq!(s.get("TSLA").unwrap().name, "Tesla");
}

#[test]
fn csv_import_missing_columns_default_empty() {
    let csv = "symbol,name\n\
               XYZ,XYZ Corp\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::Manual).unwrap();
    assert_eq!(stats.imported, 1);
    let back = s.get("XYZ").unwrap();
    assert_eq!(back.name, "XYZ Corp");
    assert!(back.sector.is_empty());
}

#[test]
fn csv_import_utf8_chinese() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               0700.HK,腾讯控股,Tech,Internet,HKEX,CN,CNY,3000000000000,2004\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 1);
    let back = s.get("0700.HK").unwrap();
    assert_eq!(back.name, "腾讯控股");
}

#[test]
fn csv_import_quoted_field_with_comma() {
    let csv = r#"symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year
BRK.B,"Berkshire Hathaway Inc., Class B",Financial,Insurance,NYSE,US,USD,900000000000,1996
"#;
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 1);
    let back = s.get("BRK.B").unwrap();
    assert_eq!(back.name, "Berkshire Hathaway Inc., Class B");
}

#[test]
fn csv_import_duplicate_symbol_upsert() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               AAPL,Apple v1,Tech,CE,NASDAQ,US,USD,100,1980\n\
               AAPL,Apple v2,Tech,CE,NASDAQ,US,USD,200,1980\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 2);
    let back = s.get("AAPL").unwrap();
    assert_eq!(back.name, "Apple v2");
    assert_eq!(back.market_cap, Some(200.0));
}

#[test]
fn csv_import_nonexistent_file_errors() {
    let s = SymbolStore::open_in_memory().unwrap();
    let r = import_from_csv(&s, "/nonexistent/file.csv", Provenance::Manual);
    assert!(r.is_err());
}

#[test]
fn csv_import_batch_boundary_1000() {
    // 用 1001 行触发 batch=1000 flush
    let mut csv =
        String::from("symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n");
    for i in 0..1001 {
        csv.push_str(&format!("S{i},Name{i},Tech,HW,NYSE,US,USD,1000,2000\n"));
    }
    let f = write_csv(&csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 1001, "1001 行全入");
    assert_eq!(s.count(), 1001);
}

// =============================================================================
// Cross-module integration
// =============================================================================

#[test]
fn integration_csv_to_search() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               AAPL,Apple,Tech,CE,NASDAQ,US,USD,2900000000000,1980\n\
               MSFT,Microsoft,Tech,SW,NASDAQ,US,USD,2500000000000,1986\n\
               JPM,JPMorgan,Finance,Banks,NYSE,US,USD,500000000000,2000\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    let stats = import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert_eq!(stats.imported, 3);

    // 通过 catalog trait 查
    let cat: &dyn SymbolCatalog = &s;
    let tech = cat.search(Some("Tech"), None, None, 10);
    assert_eq!(tech.len(), 2);
    let finance = cat.search(Some("Finance"), None, None, 10);
    assert_eq!(finance.len(), 1);
    let jpmorgan = finance[0].clone();
    assert_eq!(jpmorgan.symbol, "JPM");
}

#[test]
fn integration_csv_to_delete() {
    let csv = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
               AAPL,Apple,Tech,CE,NASDAQ,US,USD,100,1980\n";
    let f = write_csv(csv);
    let s = SymbolStore::open_in_memory().unwrap();
    import_from_csv(&s, f.path(), Provenance::FinanceDatabase).unwrap();
    assert!(s.get("AAPL").is_some());
    s.delete("AAPL").unwrap();
    assert!(s.get("AAPL").is_none());
}

#[test]
fn integration_csv_upsert_via_import() {
    let csv1 = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
                AAPL,Apple v1,Tech,CE,NASDAQ,US,USD,100,1980\n";
    let csv2 = "symbol,name,sector,industry,exchange,country,currency,market_cap,ipo_year\n\
                AAPL,Apple v2,Tech,CE,NASDAQ,US,USD,200,1980\n";
    let f1 = write_csv(csv1);
    let f2 = write_csv(csv2);
    let s = SymbolStore::open_in_memory().unwrap();
    import_from_csv(&s, f1.path(), Provenance::FinanceDatabase).unwrap();
    import_from_csv(&s, f2.path(), Provenance::FinanceDatabase).unwrap();
    let back = s.get("AAPL").unwrap();
    assert_eq!(back.name, "Apple v2");
    assert_eq!(back.market_cap, Some(200.0));
}

#[test]
fn integration_v6_migration_columns() {
    // V5 旧库 (无 ipo_date/delisted_date) → V6 自动 ALTER ADD
    let dir = tempfile::tempdir().unwrap();
    let p: PathBuf = dir.path().join("v5.db");
    let conn = rusqlite::Connection::open(&p).unwrap();
    conn.execute_batch(
        "CREATE TABLE symbols (
            symbol TEXT PRIMARY KEY,
            name TEXT, sector TEXT, industry TEXT, exchange TEXT,
            country TEXT, currency TEXT, market_cap REAL, ipo_year INTEGER,
            provenance TEXT, last_updated_ms INTEGER
        )",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO symbols VALUES ('OLD', 'Old Co', '', '', '', '', '', NULL, NULL, 'manual', 0)",
        [],
    )
    .unwrap();
    drop(conn);

    let s = SymbolStore::open(&p).unwrap();
    let back = s.get("OLD").unwrap();
    assert!(back.ipo_date.is_none());
    assert!(back.delisted_date.is_none());
}
