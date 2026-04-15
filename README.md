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

Initialize, search, and reverse geocode in one command:

```bash
cargo run --bin init_search_reverse_geocode -- ./release-assets bihar 25.5941 85.1376
```

This uses the local release assets folder, searches by place name, and reverse geocodes the provided coordinates.

Build `geo-0.0.1.db` directly from the countries GeoJSON source:

```bash
cargo run --bin build_geo_db -- \
	--input-url https://github.com/datasets/geo-countries/blob/main/data/countries.geojson \
	--version 0.0.1
```

Rebuild the spatial sidecar after regenerating the country DB:

```bash
cargo run --bin build_spatial_index -- geo-0.0.1.db geo-0.0.1.spx
```

Generate `assets-manifest.json` from local release files:

```bash
cargo run --bin build_assets_manifest -- --version 0.0.1
```

By default, the helper expects versioned files named `geo-<version>.db`, `subdistrict-<version>.db`, and `cities-<version>.{fst,rkyv,points}` in the current directory. You can override individual paths with `--geo`, `--subdistrict`, `--city-fst`, `--city-rkyv`, and `--city-points`, and you can override the release download base URL with `--base-url`.

Build and package all release assets in one command:

```bash
scripts/build_release_assets.sh --version 0.0.1
```

This script runs the geo, city, and subdistrict builders, writes all `.sha256` files, and emits `assets-manifest.json`.

If you prefer Make:

```bash
make release-assets VERSION=0.0.1
```

If you prefer just:

```bash
just release-assets version=0.0.1
```

Smoke-test a downloaded or locally generated release asset folder:

```bash
cargo run --bin smoke_release_assets -- ./release-assets bihar 25.5941 85.1376
```

This binary initializes the engine from the provided asset folder and exercises the public init, lookup, search, batch, background refresh, and open-from-bytes paths.

The `release-assets/` folder is the local copy of a release bundle. The files inside map to these roles:

- `geo-<version>.db`: country polygons database used for country-level lookup.
- `geo-<version>.db.sha256`: checksum for the country database.
- `geo-<version>.spx`: spatial sidecar for faster country polygon filtering.
- `subdistrict-<version>.db`: India subdistrict polygons database with district/state metadata.
- `subdistrict-<version>.db.sha256`: checksum for the subdistrict database.
- `subdistrict-<version>.meta`: serialized subdistrict metadata used for name/code resolution.
- `subdistrict-<version>.meta.sha256`: checksum for the subdistrict metadata file.
- `cities-<version>.fst`: prefix-search index for city name lookup.
- `cities-<version>.fst.sha256`: checksum for the city FST index.
- `cities-<version>.core`: archived city records with coordinates and codes.
- `cities-<version>.core.sha256`: checksum for the city core data.
- `cities-<version>.meta`: archived string table backing the city records.
- `cities-<version>.meta.sha256`: checksum for the city metadata string table.
- `cities-<version>.points`: optional city point index used by the release bundle.
- `cities-<version>.points.sha256`: checksum for the city points file.

The `*.sha256` files are verification sidecars; the engine uses them to confirm that downloaded or copied assets match the published release.

Quick init, search, and reverse geocode example:

```bash
cargo run --bin init_search_reverse_geocode -- ./release-assets bihar 25.5941 85.1376
```

This binary initializes the engine, runs a place-name search, and reverse geocodes the provided coordinates.

When `assets-manifest.json` is present in the repo root, the binary uses it directly instead of fetching the latest GitHub release manifest.

For CI/CD releases, push a tag like `v0.0.1` or run the GitHub Actions workflow named `Release Assets`. The workflow checks that the versioned artifacts already exist, regenerates `assets-manifest.json`, and publishes the release assets automatically.

If you want the runner to rebuild from source inputs instead of using prebuilt artifacts, use the GitHub Actions workflow named `Release Assets From Sources`. For tag-based releases, set repository variables `SUBDISTRICT_SHP_URL`, `SUBDISTRICT_DBF_URL`, and optionally `DATA_CSV_URL`, `GEOJSON_URL`, and `RELEASE_BASE_URL`. After that, pushing a tag like `v0.0.2` builds geo, cities, subdistrict, checksums, and the manifest automatically.

## Release Checklist

1. Configure repository variables once in GitHub (`Settings -> Secrets and variables -> Actions -> Variables`):
	- `SUBDISTRICT_SHP_URL` (required)
	- `SUBDISTRICT_DBF_URL` (required)
	- `DATA_CSV_URL` (optional)
	- `GEOJSON_URL` (optional, default is the countries GeoJSON URL)
	- `RELEASE_BASE_URL` (optional)
2. Confirm local build is green:

```bash
cargo check --bins
```

3. Create and push a release tag:

```bash
git tag v0.0.2
git push origin v0.0.2
```

4. Watch GitHub Actions workflow `Release Assets From Sources` and verify the release artifacts and `assets-manifest.json` were published.

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
