# geo_engine

Offline geo lookup engine for country + India hierarchy:

- `subdistrict -> district -> state -> country`
- Rust-first library with Flutter Rust Bridge-friendly initialization

## Files

- `geo.db`: country polygons database
- `subdistrict.db`: India subdistrict polygons database (contains district/state metadata)
- `geo.db.sha256`: SHA-256 checksum for `geo.db`
- `subdistrict.db.sha256`: SHA-256 checksum for `subdistrict.db`

## Rust CLI

From repo root:

```bash
cargo run --bin lookup_point -- 25.5941 85.1376
```

Subdistrict-only line output:

```bash
cargo run --bin lookup_subdistrict_point -- 25.5941 85.1376
```

Rebuild subdistrict DB:

```bash
cargo run --bin build_subdistrict_db
```

## Public API

Main exported functions:

- `init_databases(country_db_path, subdistrict_db_path)`
- `init_databases_from_strings(country_path, subdistrict_path)`
- `init_with_remote(cache_dir_path)`
- `init_with_remote_path(cache_dir_path_string)`
- `lookup(lat, lon)`
- `lookup_place(lat, lon)`

Note:
- `lookup()` requires initialization first via one of the init functions above.

## Initialization Modes

### 1. Local file init

Use when your app already has DB file paths:

- `init_databases(...)` or `init_databases_from_strings(...)`

### 2. Remote bootstrap init

Use when DB files may be missing locally:

- `init_with_remote(...)` or `init_with_remote_path(...)`

Default remote base URL:

- `https://raw.githubusercontent.com/sumitsharansatsangi/geo_engine/refs/heads/main`

Override with env var:

- `GEO_ENGINE_DB_BASE_URL`

Remote server must provide:

- `geo.db`
- `subdistrict.db`
- `geo.db.sha256`
- `subdistrict.db.sha256`

## Startup Download + Checksum Behavior (Clear Rules)

When `init_with_remote*` is called (recommended at every app startup):

1. It fetches remote checksum files.
2. It compares local file SHA-256 with remote checksum.
3. It downloads only if:
   - local file is missing, or
   - checksum is different.
4. If local file exists and remote check/download fails:
   - it falls back silently to local file.
5. If local file does not exist and remote check/download fails:
   - it fails loudly with error.
6. It writes local checksum files into cache directory after successful validation/download.

## Flutter Rust Bridge Recommended Flow

1. Choose a writable cache directory in Flutter app.
2. Call `init_with_remote_path(cacheDirPath)` once during startup.
3. Call `lookup(...)` for coordinates.

This avoids embedding DB bytes in native libraries and avoids per-ABI duplication.

### FRB Startup Snippet (Dart)

```dart
import 'package:path_provider/path_provider.dart';
// import your generated FRB API
// import 'bridge_generated.dart';

Future<void> initGeoEngine() async {
  final dir = await getApplicationSupportDirectory();
  final cacheDirPath = dir.path;

  // Downloads/validates DB files when needed, otherwise reuses local files.
  // await api.initWithRemotePath(cacheDirPath: cacheDirPath);
}

Future<void> lookupPoint(double lat, double lon) async {
  // final result = await api.lookup(lat: lat, lon: lon);
  // final place = await api.lookupPlace(lat: lat, lon: lon);
}
```

### FRB Local-File Init Snippet (Dart)

```dart
import 'package:path_provider/path_provider.dart';
// import your generated FRB API
// import 'bridge_generated.dart';

Future<void> initGeoEngineFromLocalFiles() async {
  final dir = await getApplicationSupportDirectory();
  final countryPath = '${dir.path}/geo.db';
  final subdistrictPath = '${dir.path}/subdistrict.db';

  // Ensure these files already exist before calling init.
  // await api.initDatabasesFromStrings(
  //   countryDbPath: countryPath,
  //   subdistrictDbPath: subdistrictPath,
  // );
}
```

## Verify

```bash
cargo check
cargo test --test lookup_integration
```
