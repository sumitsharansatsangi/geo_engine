# Asset Initialization Guide

This document explains how to initialize the geo_engine with custom configuration for downloading and validating database assets.

## Overview

The geo_engine provides two main initialization flows:

1. **Default Initialization** - Uses system cache directory, downloads files if missing, no checksum validation
2. **Custom Initialization** - Use custom asset directory and enable/disable checksum validation

## Default Usage

### Geo Engine

```rust
use geo_engine::init_geo_engine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Uses ~/.cache/geo_engine (macOS) or $XDG_CACHE_HOME/geo_engine (Linux)
    // Downloads missing files automatically, no checksum validation
    let engine = init_geo_engine()?;
    
    // Use engine for lookups...
    Ok(())
}
```

### City Assets

```rust
use geo_engine::init_city_assets;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Uses same cache directory as geo_engine
    // Downloads missing files automatically, no checksum validation
    let assets = init_city_assets()?;
    
    // Use assets.fst_path, assets.rkyv_path, assets.points_path...
    Ok(())
}
```

## Custom Configuration

### With Checksum Verification

```rust
use geo_engine::{InitConfig, init_geo_engine_with_config};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = InitConfig {
        asset_dir: PathBuf::from("./assets"),  // Custom download directory
        verify_checksum: true,                   // Enable SHA-256 verification
    };
    
    // Download files to ./assets and verify checksums
    let engine = init_geo_engine_with_config(&config)?;
    
    Ok(())
}
```

### Without Checksum Verification

```rust
use geo_engine::{InitConfig, init_geo_engine_with_config};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = InitConfig {
        asset_dir: PathBuf::from("./data"),     // Custom download directory
        verify_checksum: false,                  // Skip checksum validation
    };
    
    let engine = init_geo_engine_with_config(&config)?;
    
    Ok(())
}
```

## Initialization Behavior

### File Resolution Flow

For each required asset, the initialization follows this flow:

1. **Check if file exists** in the configured `asset_dir`
   - If file exists AND checksum validation is **disabled** → Use it immediately
   - If file exists AND checksum validation is **enabled** → Verify checksum
     - If checksum matches → Use it
     - If checksum mismatches → Delete and redownload

2. **Download from GitHub Release**
   - If file doesn't exist or checksum failed, download from `v0.0.1` release
   - Release URL: `https://github.com/sumitsharansatsangi/geo_engine/releases/download/v0.0.1/`

3. **Validate after Download** (if `verify_checksum=true`)
   - Compute SHA-256 of downloaded file
   - Compare against hardcoded expected value
   - Raise `ReleaseChecksumMismatch` error if mismatch detected

### Assets and SHA-256 Values

| Asset | Filename | SHA-256 |
|-------|----------|---------|
| Geo Database | `geo-0.0.1.db` | `44c2b0887d044135336538f0f67df3d49f2e8b64d04d4b2b3c03fb6d946f7fa0` |
| Subdistrict Database | `subdistrict-0.0.1.db` | `72ce3c7c8e3cfdea2d354172c4d5536044b05e8d2b91a5a2dda72326fb0291aa` |
| City FST Index | `cities-0.0.1.fst` | `8bb3a2f202db0864537e8ebd3bdc31c229218ca06a8ca787df5b7d7112a51995` |
| City Rkyv Data | `cities-0.0.1.rkyv` | `7da471653c444d3b1b16070a33819653f04f9f100a1065b951e89b86d6e1a6fb` |
| City Points Data | `cities-0.0.1.points` | `ac5836cf4a7a0bd93a96638830bcba546c61eec59b13ebf8317bfafdf3d0b46e` |

## Example: City Assets with Checksum

```rust
use geo_engine::{InitConfig, init_city_assets_with_config};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = InitConfig {
        asset_dir: PathBuf::from("./city_data"),
        verify_checksum: true,  // Verify checksums on init
    };
    
    // Download city assets to ./city_data with checksum validation
    let assets = init_city_assets_with_config(&config)?;
    
    println!("FST Index: {}", assets.fst_path.display());
    println!("Rkyv Data: {}", assets.rkyv_path.display());
    println!("Points Data: {}", assets.points_path.display());
    
    Ok(())
}
```

## Error Handling

```rust
use geo_engine::{InitConfig, init_geo_engine_with_config, GeoEngineError};
use std::path::PathBuf;

fn main() {
    let config = InitConfig {
        asset_dir: PathBuf::from("./assets"),
        verify_checksum: true,
    };
    
    match init_geo_engine_with_config(&config) {
        Ok(engine) => println!("Engine initialized successfully"),
        Err(GeoEngineError::ReleaseChecksumMismatch { path, expected, actual }) => {
            eprintln!("Checksum mismatch for {:?}", path);
            eprintln!("Expected: {}", expected);
            eprintln!("Actual: {}", actual);
        }
        Err(GeoEngineError::ReleaseDownloadFailed { asset, source }) => {
            eprintln!("Failed to download {}: {}", asset, source);
        }
        Err(GeoEngineError::CacheDirectoryUnavailable { path, source }) => {
            eprintln!("Cannot create directory {:?}: {}", path, source);
        }
        Err(e) => eprintln!("Initialization failed: {}", e),
    }
}
```

## Environment Variables

- `GEO_ENGINE_CACHE_DIR` - Override the default cache directory
  ```bash
  export GEO_ENGINE_CACHE_DIR=/custom/cache/path
  ```

## Key Features

✅ **Download-first approach** - Files are always downloaded if missing
✅ **Configurable checksums** - Enable/disable SHA-256 validation per initialization
✅ **Custom asset paths** - Store downloaded files anywhere
✅ **Automatic retry** - Corrupted files are automatically redownloaded
✅ **Clean error reporting** - Detailed error messages for debugging
