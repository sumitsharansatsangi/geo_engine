# geo_engine

Offline geo lookup engine for country + India hierarchy:

- `subdistrict -> district -> state -> country`
- Rust-first library with Flutter Rust Bridge-friendly initialization

## Files

- `geo.db`: country polygons database
- `subdistrict.db`: India subdistrict polygons database (contains district/state metadata)

## Rust CLI

From repo root:

```bash
cargo run --bin lookup_point -- 25.5941 85.1376 geo.db subdistrict.db
```

Subdistrict-only line output:

```bash
cargo run --bin lookup_subdistrict_point -- 25.5941 85.1376 geo.db subdistrict.db
```

## Public API

Main exported functions:

- `lookup_with_subdistrict_path(lat, lon, country_db_path, subdistrict_db_path)`



## Usage

Provide paths to the database files and call `lookup_with_subdistrict_path(lat, lon, country_db_path, subdistrict_db_path)` for coordinates.

## Verify

```bash
cargo check
cargo test --test lookup_integration
```
