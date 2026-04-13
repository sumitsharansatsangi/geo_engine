use std::env;
use std::path::Path;
use std::process;

const DEFAULT_SUBDISTRICT_DB: &str = "subdistrict.db";
const DEFAULT_GEO_DB: &str = "geo.db";
const DEFAULT_CITY_FST: &str = "cities-0.0.1.fst";
const DEFAULT_CITY_RKYV: &str = "cities-0.0.1.rkyv";

fn main() {
    let mut args = env::args().skip(1);
    let query = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing search query");
    });

    let subdistrict_path = args
        .next()
        .unwrap_or_else(|| DEFAULT_SUBDISTRICT_DB.to_string());
    let geo_path = args.next().unwrap_or_else(|| DEFAULT_GEO_DB.to_string());
    let city_fst_path = args.next().unwrap_or_else(|| DEFAULT_CITY_FST.to_string());
    let city_rkyv_path = args.next().unwrap_or_else(|| DEFAULT_CITY_RKYV.to_string());

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    let geo_db = Path::new(&geo_path);
    let subdistrict_db = Path::new(&subdistrict_path);
    let city_fst = Path::new(&city_fst_path);
    let city_rkyv = Path::new(&city_rkyv_path);

    if let Err(err) = geo_engine::init_path(geo_db, subdistrict_db, city_fst, city_rkyv) {
        eprintln!("Init failed: {err}");
        process::exit(1);
    }

    match geo_engine::search(&query) {
        Ok(results) => {
            if results.subdistricts.is_empty() {
                eprintln!("No subdistrict found for query: {query}");
                process::exit(1);
            }

            for matched in results.subdistricts {
                println!(
                    "{}, {}, {}",
                    matched.subdistrict.name, matched.district.name, matched.state.name
                );
            }
        }
        Err(err) => {
            eprintln!("Search failed: {err}");
            process::exit(1);
        }
    }
}

fn print_usage_and_exit(message: &str) -> ! {
    eprintln!("{message}");
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --bin lookup_subdistrict_point -- <query> [subdistrict.db path] [geo.db path] [cities.fst path] [cities.rkyv path]"
    );
    process::exit(2);
}
