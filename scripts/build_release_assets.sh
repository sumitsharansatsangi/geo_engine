#!/usr/bin/env bash
set -euo pipefail

VERSION="0.0.1"
GEOJSON_URL="https://github.com/datasets/geo-countries/blob/main/data/countries.geojson"
BASE_URL=""
DATA_CSV_PATH="${DATA_CSV_PATH:-data.csv}"

usage() {
  cat <<'EOF'
Usage:
  scripts/build_release_assets.sh [--version X.Y.Z] [--geojson-url URL] [--base-url URL] [--subdistrict-shp PATH] [--subdistrict-dbf PATH] [--data-csv PATH]

Examples:
  scripts/build_release_assets.sh --version 0.0.2
  scripts/build_release_assets.sh --version 0.0.2 --subdistrict-shp /path/to/SUBDISTRICT_BOUNDARY.shp --subdistrict-dbf /path/to/SUBDISTRICT_BOUNDARY.dbf
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --geojson-url)
      GEOJSON_URL="$2"
      shift 2
      ;;
    --base-url)
      BASE_URL="$2"
      shift 2
      ;;
    --subdistrict-shp)
      SUBDISTRICT_SHP_PATH="$2"
      shift 2
      ;;
    --subdistrict-dbf)
      SUBDISTRICT_DBF_PATH="$2"
      shift 2
      ;;
    --data-csv)
      DATA_CSV_PATH="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

geo_db="geo-${VERSION}.db"
subdistrict_db="subdistrict-${VERSION}.db"
city_fst="cities-${VERSION}.fst"
city_rkyv="cities-${VERSION}.rkyv"
city_points="cities-${VERSION}.points"
spatial_sidecar="geo-${VERSION}.spx"

cargo run --bin build_geo_db -- --input-url "$GEOJSON_URL" --version "$VERSION" --output "$geo_db"
cargo run --bin build_spatial_index -- "$geo_db" "$spatial_sidecar"

subdistrict_env=(
  SUBDISTRICT_OUTPUT_PATH="$subdistrict_db"
  DISTRICT_DATA_CSV_PATH="$DATA_CSV_PATH"
)

if [[ -n "${SUBDISTRICT_SHP_PATH:-}" ]]; then
  subdistrict_env+=(SUBDISTRICT_SHP_PATH="$SUBDISTRICT_SHP_PATH")
fi

if [[ -n "${SUBDISTRICT_DBF_PATH:-}" ]]; then
  subdistrict_env+=(SUBDISTRICT_DBF_PATH="$SUBDISTRICT_DBF_PATH")
fi

env "${subdistrict_env[@]}" cargo run --bin build_subdistrict_db

cargo run --bin build_city -- --version "$VERSION"

shasum -a 256 "$geo_db" > "$geo_db.sha256"
shasum -a 256 "$subdistrict_db" > "$subdistrict_db.sha256"
shasum -a 256 "$city_fst" > "$city_fst.sha256"
shasum -a 256 "$city_rkyv" > "$city_rkyv.sha256"
shasum -a 256 "$city_points" > "$city_points.sha256"

manifest_args=(
  --version "$VERSION"
  --geo "$geo_db"
  --subdistrict "$subdistrict_db"
  --city-fst "$city_fst"
  --city-rkyv "$city_rkyv"
  --city-points "$city_points"
  --output assets-manifest.json
)

if [[ -n "$BASE_URL" ]]; then
  manifest_args+=(--base-url "$BASE_URL")
fi

cargo run --bin build_assets_manifest -- "${manifest_args[@]}"

echo "Release assets generated for version $VERSION"
