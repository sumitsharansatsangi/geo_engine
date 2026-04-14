use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

const DEFAULT_VERSION: &str = "0.0.1";
const DEFAULT_REPO: &str = "sumitsharansatsangi/geo_engine";
const DEFAULT_OUTPUT_PATH: &str = "assets-manifest.json";

#[derive(Debug, Serialize)]
struct AssetsManifest {
    geo: AssetGroup,
    subdistrict: AssetGroup,
    city: CityGroup,
}

#[derive(Debug, Serialize)]
struct AssetGroup {
    db: ManifestAsset,
}

#[derive(Debug, Serialize)]
struct CityGroup {
    fst: ManifestAsset,
    rkyv: ManifestAsset,
    points: ManifestAsset,
}

#[derive(Debug, Serialize)]
struct ManifestAsset {
    name: String,
    url: String,
    sha256: String,
}

#[derive(Debug)]
struct Inputs {
    version: String,
    base_url: String,
    geo_path: PathBuf,
    subdistrict_path: PathBuf,
    city_fst_path: PathBuf,
    city_rkyv_path: PathBuf,
    city_points_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let inputs = parse_args(env::args().skip(1))?;
    let base_url = normalize_base_url(&inputs.base_url);

    let manifest = AssetsManifest {
        geo: AssetGroup {
            db: manifest_asset(&inputs.geo_path, &base_url, &inputs.geo_path)?,
        },
        subdistrict: AssetGroup {
            db: manifest_asset(
                &inputs.subdistrict_path,
                &base_url,
                &inputs.subdistrict_path,
            )?,
        },
        city: CityGroup {
            fst: manifest_asset(&inputs.city_fst_path, &base_url, &inputs.city_fst_path)?,
            rkyv: manifest_asset(&inputs.city_rkyv_path, &base_url, &inputs.city_rkyv_path)?,
            points: manifest_asset(
                &inputs.city_points_path,
                &base_url,
                &inputs.city_points_path,
            )?,
        },
    };

    let json = serde_json::to_vec_pretty(&manifest)?;
    fs::write(&inputs.output_path, json)?;

    println!("✅ wrote {}", inputs.output_path.display());
    println!("ℹ️ version={}, base_url={}", inputs.version, base_url);

    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Inputs, Box<dyn std::error::Error>> {
    let mut version = DEFAULT_VERSION.to_string();
    let mut base_url = default_release_base_url(DEFAULT_REPO, &version);
    let mut geo_path: Option<PathBuf> = None;
    let mut subdistrict_path: Option<PathBuf> = None;
    let mut city_fst_path: Option<PathBuf> = None;
    let mut city_rkyv_path: Option<PathBuf> = None;
    let mut city_points_path: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => {
                let value = args.next().ok_or("missing value for --version")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err("--version cannot be empty".into());
                }
                version = trimmed.to_string();
                if base_url == default_release_base_url(DEFAULT_REPO, DEFAULT_VERSION) {
                    base_url = default_release_base_url(DEFAULT_REPO, &version);
                }
            }
            "--base-url" => {
                let value = args.next().ok_or("missing value for --base-url")?;
                base_url = value;
            }
            "--geo" => {
                geo_path = Some(PathBuf::from(args.next().ok_or("missing value for --geo")?));
            }
            "--subdistrict" => {
                subdistrict_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --subdistrict")?,
                ));
            }
            "--city-fst" => {
                city_fst_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --city-fst")?,
                ));
            }
            "--city-rkyv" => {
                city_rkyv_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --city-rkyv")?,
                ));
            }
            "--city-points" => {
                city_points_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --city-points")?,
                ));
            }
            "--output" => {
                output_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --output")?,
                ));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }

    let geo_path = geo_path.unwrap_or_else(|| PathBuf::from(default_geo_name(&version)));
    let subdistrict_path =
        subdistrict_path.unwrap_or_else(|| PathBuf::from(default_subdistrict_name(&version)));
    let city_fst_path =
        city_fst_path.unwrap_or_else(|| PathBuf::from(default_city_name(&version, "fst")));
    let city_rkyv_path =
        city_rkyv_path.unwrap_or_else(|| PathBuf::from(default_city_name(&version, "rkyv")));
    let city_points_path =
        city_points_path.unwrap_or_else(|| PathBuf::from(default_city_name(&version, "points")));
    let output_path = output_path.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_PATH));

    Ok(Inputs {
        version,
        base_url,
        geo_path,
        subdistrict_path,
        city_fst_path,
        city_rkyv_path,
        city_points_path,
        output_path,
    })
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin build_assets_manifest -- [--version X.Y.Z] [--base-url URL] [--geo PATH] [--subdistrict PATH] [--city-fst PATH] [--city-rkyv PATH] [--city-points PATH] [--output PATH]"
    );
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  cargo run --bin build_assets_manifest -- --version 0.0.2");
    eprintln!(
        "  cargo run --bin build_assets_manifest -- --version 0.0.2 --base-url https://github.com/<owner>/<repo>/releases/download/v0.0.2/"
    );
}

fn manifest_asset(
    path: &Path,
    base_url: &str,
    local_path: &Path,
) -> Result<ManifestAsset, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let sha256 = sha256_hex(&bytes);
    let name = file_name_string(path)?;
    let url = format!(
        "{}{}",
        ensure_trailing_slash(base_url),
        file_name_string(local_path)?
    );

    Ok(ManifestAsset { name, url, sha256 })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn file_name_string(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("path '{}' has no valid file name", path.display()))?;
    Ok(name.to_string())
}

fn ensure_trailing_slash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

fn normalize_base_url(base_url: &str) -> String {
    ensure_trailing_slash(base_url)
}

fn default_release_base_url(repo: &str, version: &str) -> String {
    format!("https://github.com/{repo}/releases/download/v{version}/")
}

fn default_geo_name(version: &str) -> String {
    format!("geo-{version}.db")
}

fn default_subdistrict_name(version: &str) -> String {
    format!("subdistrict-{version}.db")
}

fn default_city_name(version: &str, ext: &str) -> String {
    format!("cities-{version}.{ext}")
}
