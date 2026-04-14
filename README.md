# geo_engine

Offline geo lookup engine for country + India hierarchy:

- `subdistrict -> district -> state -> country`
- Rust-first library with Flutter Rust Bridge-friendly initialization

## Files

- `geo-0.0.1.db`: country polygons database
- `subdistrict.db`: India subdistrict polygons database (contains district/state metadata)
- `data.csv`: district demographics mapping with languages and major religion

Newly built `subdistrict.db` files can also embed the `data.csv` demographics, so runtime lookup can read religion/language data directly from the DB. Older databases still work and fall back to `data.csv`.

## Rust CLI

From repo root:

```bash
cargo run --bin lookup_point -- 25.5941 85.1376 geo-0.0.1.db subdistrict.db
```

Subdistrict-only line output:

```bash
cargo run --bin lookup_subdistrict_point -- 25.5941 85.1376 geo-0.0.1.db subdistrict.db data.csv
```

To bake demographics into a rebuilt `subdistrict.db`, set `DISTRICT_DATA_CSV_PATH` (or keep the default `data.csv`) when running `build_subdistrict_db`.

If you already have a `subdistrict.db` and just want to inject demographics from `data.csv` without rebuilding from shapefiles:

```bash
cargo run --bin enrich_subdistrict_db -- subdistrict.db data.csv
```

This updates the existing `subdistrict.db` in place by embedding district religion and language metadata into each subdistrict record.

Search by subdistrict name:

```bash
cargo run --bin lookup_subdistrict_point -- --search sabour subdistrict.db
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

## Runtime Defaults

The engine is configured with performance-safe defaults out of the box:

- zstd-compressed DBs are decoded by streaming into a temp file and then mmap-ed.
- Country DB loading supports a single file path by default, and also supports shard directories when you pass a directory path.
- Bounding-box prefiltering runs before polygon checks and auto-enables SIMD (NEON on aarch64, SSE on x86_64) when available.

### Optional Environment Variables

- `GEO_ENGINE_DISABLE_ZSTD_STREAM_MMAP=1`
	- Disables stream-to-temp-mmap decode and falls back to full in-memory decode for zstd DBs.
- `GEO_ENGINE_ZSTD_DICT_PATH=/absolute/path/to/dict`
	- Optional zstd dictionary used for DB decompression.
- `GEO_ENGINE_DISABLE_H3=1`
	- Disables H3 sidecar candidate acceleration.

### Country DB Path Behavior

- If `country_db_path` points to a file:
	- The engine loads a single country DB file.
- If `country_db_path` points to a directory:
	- The engine loads all shard files in that directory with extensions `.db` or `.zst`.
	- Use this mode only for very large global datasets or region-based deployments.
