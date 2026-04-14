VERSION ?= 0.0.1
GEOJSON_URL ?= https://github.com/datasets/geo-countries/blob/main/data/countries.geojson
BASE_URL ?=
SUBDISTRICT_SHP_PATH ?=
SUBDISTRICT_DBF_PATH ?=
DATA_CSV_PATH ?= data.csv

.PHONY: release-assets help

release-assets:
	@set -e; \
	set -- --version "$(VERSION)" --geojson-url "$(GEOJSON_URL)"; \
	if [ -n "$(BASE_URL)" ]; then set -- "$$@" --base-url "$(BASE_URL)"; fi; \
	if [ -n "$(SUBDISTRICT_SHP_PATH)" ]; then set -- "$$@" --subdistrict-shp "$(SUBDISTRICT_SHP_PATH)"; fi; \
	if [ -n "$(SUBDISTRICT_DBF_PATH)" ]; then set -- "$$@" --subdistrict-dbf "$(SUBDISTRICT_DBF_PATH)"; fi; \
	if [ -n "$(DATA_CSV_PATH)" ]; then set -- "$$@" --data-csv "$(DATA_CSV_PATH)"; fi; \
	./scripts/build_release_assets.sh "$$@"

help:
	@printf '%s\n' \
		'Usage:' \
		'  make release-assets VERSION=0.0.2' \
		'' \
		'Optional overrides:' \
		'  GEOJSON_URL=...' \
		'  BASE_URL=...' \
		'  SUBDISTRICT_SHP_PATH=...' \
		'  SUBDISTRICT_DBF_PATH=...' \
		'  DATA_CSV_PATH=...'
