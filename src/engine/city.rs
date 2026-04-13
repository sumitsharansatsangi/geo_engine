use unicode_normalization::UnicodeNormalization;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct City {
    pub geoname_id: u32,
    pub country_code: String,
    pub name: String,
    pub ascii: String,
    pub alternates: Vec<String>,
    pub admin1_code: Option<String>,
    pub admin1_name: Option<String>,
    pub admin2_code: Option<String>,
    pub admin2_name: Option<String>,
    pub lat: f32,
    pub lon: f32,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone, Copy)]
pub struct CityPoint {
    pub id: u32,
    pub lat: f32,
    pub lon: f32,
}

// ── Normalization Core ────────────────────────────────────────────────────────

/// Streaming normalized iterator (zero allocation).
/// - NFKD normalization
/// - removes ASCII punctuation
/// - lowercases
/// - emits ASCII bytes only (FST-friendly)
#[inline]
pub fn normalize_iter(s: &str) -> impl Iterator<Item = u8> + '_ {
    s.nfkd()
        .filter(|c| !c.is_ascii_punctuation())
        .flat_map(|c| c.to_lowercase())
        .filter_map(|c| {
            if c.is_ascii() {
                Some(c as u8)
            } else {
                None
            }
        })
}

/// ASCII fast-path (SIMD-friendly via LLVM auto-vectorization)
#[inline]
pub fn normalize_iter_fast(s: &str) -> NormalizeIterFast<'_> {
    if s.is_ascii() {
        NormalizeIterFast::Ascii(s.as_bytes().iter().copied())
    } else {
        NormalizeIterFast::Unicode(normalize_iter(s))
    }
}

// ── Iterator Enum (no external crate like `either`) ───────────────────────────

pub enum NormalizeIterFast<'a> {
    Ascii(std::iter::Copied<std::slice::Iter<'a, u8>>),
    Unicode(std::iter::FilterMap<
        std::iter::FlatMap<
            std::iter::Filter<
                unicode_normalization::Decompositions<'a>,
                fn(&char) -> bool
            >,
            std::char::ToLowercase,
            fn(char) -> std::char::ToLowercase
        >,
        fn(char) -> Option<u8>
    >),
}

impl<'a> Iterator for NormalizeIterFast<'a> {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            NormalizeIterFast::Ascii(iter) => {
                for b in iter {
                    if !b.is_ascii_punctuation() {
                        return Some(b.to_ascii_lowercase());
                    }
                }
                None
            }
            NormalizeIterFast::Unicode(iter) => iter.next(),
        }
    }
}

// ── Buffer-Based API (NO allocations in hot path) ─────────────────────────────

/// Normalize into a reusable buffer (recommended for indexing/search)
#[inline]
pub fn normalize_to_buf(s: &str, buf: &mut Vec<u8>) {
    buf.clear();
    buf.extend(normalize_iter_fast(s));
}

/// Optional convenience (allocates once)
#[inline]
pub fn normalize(s: &str) -> String {
    let mut buf = Vec::with_capacity(s.len());
    normalize_to_buf(s, &mut buf);

    // SAFETY: we only emit ASCII bytes
    unsafe { String::from_utf8_unchecked(buf) }
}

// ── FST-Ready Insert Helper ───────────────────────────────────────────────────

/// Insert using reusable buffer (zero allocation per call)
#[inline]
pub fn insert_normalized<K: AsRef<str>>(
    builder: &mut fst::MapBuilder<Vec<u8>>,
    key: K,
    value: u64,
    buf: &mut Vec<u8>,
) -> std::io::Result<()> {
    normalize_to_buf(key.as_ref(), buf);
    builder.insert(&buf, value)
}