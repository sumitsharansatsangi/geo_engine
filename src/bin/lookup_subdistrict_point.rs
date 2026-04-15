use std::env;
use std::path::Path;
use std::process;

const DEFAULT_ASSET_DIR: &str = ".";

fn main() {
    let mut args = env::args().skip(1);
    let query = args.next().unwrap_or_else(|| {
        print_usage_and_exit("missing search query");
    });

    let asset_dir_path = args.next().unwrap_or_else(|| DEFAULT_ASSET_DIR.to_string());

    if args.next().is_some() {
        print_usage_and_exit("received too many arguments");
    }

    let asset_dir = Path::new(&asset_dir_path);

    if let Err(err) = geo_engine::init_path(asset_dir,true) {
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
    eprintln!("  cargo run --bin lookup_subdistrict_point -- <query> [asset_dir]");
    process::exit(2);
}
