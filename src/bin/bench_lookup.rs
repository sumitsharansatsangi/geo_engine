use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone, Copy)]
struct CityPoint {
    id: u32,
    lat: f32,
    lon: f32,
}

#[derive(Debug, Clone)]
struct Config {
    geo_db: PathBuf,
    subdistrict_db: PathBuf,
    city_fst: PathBuf,
    city_core: PathBuf,
    city_meta: PathBuf,
    city_points: PathBuf,
    iterations: usize,
    warmup: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchResult {
    label: String,
    iterations: usize,
    warmup: usize,
    successes: usize,
    failures: usize,
    total_ns: u128,
    mean_ns: f64,
    p50_ns: f64,
    p95_ns: f64,
    p99_ns: f64,
}

fn main() {
    let mut args = env::args().skip(1);
    if matches!(args.next().as_deref(), Some("--child")) {
        let disable_h3 = parse_bool_flag(args.next(), "disable_h3");
        let config = parse_config_from_iter(args.collect());
        match run_single(&config, disable_h3) {
            Ok(result) => {
                println!(
                    "{}",
                    serde_json::to_string(&result).expect("serialize child benchmark result")
                );
            }
            Err(err) => {
                eprintln!("benchmark child failed: {err}");
                process::exit(1);
            }
        }
        return;
    }

    let config = parse_config_from_env_args();
    if let Err(err) = run_parent(&config) {
        eprintln!("benchmark failed: {err}");
        process::exit(1);
    }
}

fn parse_config_from_env_args() -> Config {
    let mut args = env::args().skip(1);
    let geo_db = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("geo-0.0.1.db"));
    let subdistrict_db = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("subdistrict.db"));
    let city_fst = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cities-0.0.1.fst"));
    let city_core = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cities-0.0.1.core"));
    let city_meta = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cities-0.0.1.meta"));
    let city_points = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cities-0.0.1.points"));

    let iterations = args
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(5000)
        .max(100);
    let warmup = args
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(500)
        .max(10);

    if args.next().is_some() {
        eprintln!(
            "Usage: cargo run --bin bench_lookup -- [geo-0.0.1.db] [subdistrict.db] [cities.fst] [cities.core] [cities.meta] [cities.points] [iterations] [warmup]"
        );
        process::exit(2);
    }

    Config {
        geo_db,
        subdistrict_db,
        city_fst,
        city_core,
        city_meta,
        city_points,
        iterations,
        warmup,
    }
}

fn parse_config_from_iter(args: Vec<String>) -> Config {
    if args.len() != 8 {
        eprintln!(
            "internal child usage: --child <disable_h3> <geo-0.0.1.db> <subdistrict.db> <cities.fst> <cities.core> <cities.meta> <cities.points> <iterations> <warmup>"
        );
        process::exit(2);
    }

    let iterations = args[6].parse::<usize>().unwrap_or(5000).max(100);
    let warmup = args[7].parse::<usize>().unwrap_or(500).max(10);

    Config {
        geo_db: PathBuf::from(&args[0]),
        subdistrict_db: PathBuf::from(&args[1]),
        city_fst: PathBuf::from(&args[2]),
        city_core: PathBuf::from(&args[3]),
        city_meta: PathBuf::from(&args[4]),
        city_points: PathBuf::from(&args[5]),
        iterations,
        warmup,
    }
}

fn parse_bool_flag(value: Option<String>, name: &str) -> bool {
    match value.as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("True") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("False") => false,
        _ => {
            eprintln!("invalid boolean flag {name}: {:?}", value);
            process::exit(2);
        }
    }
}

fn run_parent(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    ensure_paths(config)?;

    println!("benchmark config");
    println!("  geo db: {}", config.geo_db.display());
    println!("  subdistrict db: {}", config.subdistrict_db.display());
    println!("  city fst: {}", config.city_fst.display());
    println!("  city core: {}", config.city_core.display());
    println!("  city meta: {}", config.city_meta.display());
    println!("  city points: {}", config.city_points.display());
    println!("  iterations: {}", config.iterations);
    println!("  warmup: {}", config.warmup);

    let without_h3 = run_child(config, true)?;
    let with_h3 = run_child(config, false)?;

    print_comparison(&without_h3, &with_h3);
    Ok(())
}

fn ensure_paths(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    for path in [
        &config.geo_db,
        &config.subdistrict_db,
        &config.city_fst,
        &config.city_core,
        &config.city_meta,
        &config.city_points,
    ] {
        if !path.exists() {
            return Err(format!("missing required file: {}", path.display()).into());
        }
    }
    Ok(())
}

fn run_child(config: &Config, disable_h3: bool) -> Result<BenchResult, Box<dyn std::error::Error>> {
    let exe = env::current_exe()?;
    let output = Command::new(exe)
        .arg("--child")
        .arg(if disable_h3 { "1" } else { "0" })
        .arg(config.geo_db.as_os_str())
        .arg(config.subdistrict_db.as_os_str())
        .arg(config.city_fst.as_os_str())
        .arg(config.city_core.as_os_str())
        .arg(config.city_meta.as_os_str())
        .arg(config.city_points.as_os_str())
        .arg(config.iterations.to_string())
        .arg(config.warmup.to_string())
        .env(
            "GEO_ENGINE_DISABLE_SPATIAL_INDEX",
            if disable_h3 { "1" } else { "0" },
        )
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("child benchmark failed: {stderr}").into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .ok_or("child benchmark did not return JSON")?;

    let result: BenchResult = serde_json::from_str(line)?;
    Ok(result)
}

fn run_single(
    config: &Config,
    disable_h3: bool,
) -> Result<BenchResult, Box<dyn std::error::Error>> {
    let points = load_points(&config.city_points)?;
    if points.is_empty() {
        return Err("no points loaded from cities.points".into());
    }

    let asset_dir = config.geo_db.parent().unwrap_or_else(|| Path::new("."));
    geo_engine::init_path(asset_dir,true)?;

    for i in 0..config.warmup {
        let (lat, lon) = points[i % points.len()];
        let _ = geo_engine::reverse_geocoding(lat, lon);
    }

    let mut timings_ns = Vec::with_capacity(config.iterations);
    let mut successes = 0usize;
    let mut failures = 0usize;
    let total_start = Instant::now();

    for i in 0..config.iterations {
        let (lat, lon) = points[i % points.len()];
        let start = Instant::now();
        match geo_engine::reverse_geocoding(lat, lon) {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
        timings_ns.push(start.elapsed().as_secs_f64() * 1_000_000_000.0);
    }

    let total_ns = total_start.elapsed().as_nanos();
    timings_ns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let label = if disable_h3 {
        "without_spatial_index"
    } else {
        "with_spatial_index"
    }
    .to_string();

    Ok(BenchResult {
        label,
        iterations: config.iterations,
        warmup: config.warmup,
        successes,
        failures,
        total_ns,
        mean_ns: timings_ns.iter().copied().sum::<f64>() / timings_ns.len() as f64,
        p50_ns: percentile(&timings_ns, 0.50),
        p95_ns: percentile(&timings_ns, 0.95),
        p99_ns: percentile(&timings_ns, 0.99),
    })
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * p).round() as usize;
    values[idx.min(values.len() - 1)]
}

fn load_points(path: &Path) -> Result<Vec<(f32, f32)>, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let archived: &rkyv::Archived<Vec<CityPoint>> =
        rkyv::access::<rkyv::Archived<Vec<CityPoint>>, rkyv::rancor::Error>(&bytes).unwrap_or_else(
            |_| unsafe {
                // SAFETY: cities.points is produced by this project's builder.
                rkyv::access_unchecked(&bytes)
            },
        );

    let mut points = Vec::with_capacity(archived.len());
    for point in archived.iter() {
        points.push((point.lat.into(), point.lon.into()));
    }
    Ok(points)
}

fn print_comparison(without_h3: &BenchResult, with_h3: &BenchResult) {
    println!();
    println!("results");
    print_result(without_h3);
    print_result(with_h3);

    let speedup_mean = ratio(without_h3.mean_ns, with_h3.mean_ns);
    let speedup_p95 = ratio(without_h3.p95_ns, with_h3.p95_ns);
    let speedup_p99 = ratio(without_h3.p99_ns, with_h3.p99_ns);

    println!();
    println!("comparison");
    println!("  mean speedup: {:.3}x", speedup_mean);
    println!("  p95 speedup:  {:.3}x", speedup_p95);
    println!("  p99 speedup:  {:.3}x", speedup_p99);
    println!("  note: speedup > 1.0 means spatial sidecar is faster");
}

fn print_result(result: &BenchResult) {
    println!("  {}", result.label);
    println!("    successes: {}", result.successes);
    println!("    failures:  {}", result.failures);
    println!("    mean:      {:.0} ns", result.mean_ns);
    println!("    p50:       {:.0} ns", result.p50_ns);
    println!("    p95:       {:.0} ns", result.p95_ns);
    println!("    p99:       {:.0} ns", result.p99_ns);
}

fn ratio(base: f64, comparison: f64) -> f64 {
    if comparison <= 0.0 {
        return 0.0;
    }
    base / comparison
}
