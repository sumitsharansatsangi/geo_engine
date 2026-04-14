release-assets version="0.0.1" geojson_url="https://github.com/datasets/geo-countries/blob/main/data/countries.geojson" base_url="" subdistrict_shp_path="" subdistrict_dbf_path="" data_csv_path="data.csv":
    #!/usr/bin/env bash
    set -euo pipefail

    args=(--version "{{version}}" --geojson-url "{{geojson_url}}")

    if [[ -n "{{base_url}}" ]]; then
        args+=(--base-url "{{base_url}}")
    fi

    if [[ -n "{{subdistrict_shp_path}}" ]]; then
        args+=(--subdistrict-shp "{{subdistrict_shp_path}}")
    fi

    if [[ -n "{{subdistrict_dbf_path}}" ]]; then
        args+=(--subdistrict-dbf "{{subdistrict_dbf_path}}")
    fi

    if [[ -n "{{data_csv_path}}" ]]; then
        args+=(--data-csv "{{data_csv_path}}")
    fi

    ./scripts/build_release_assets.sh "${args[@]}"

help:
    @printf '%s\n' \
        'Usage:' \
        '  just release-assets version=0.0.2' \
        '' \
        'Optional overrides:' \
        '  geojson_url=...' \
        '  base_url=...' \
        '  subdistrict_shp_path=...' \
        '  subdistrict_dbf_path=...' \
        '  data_csv_path=...'
