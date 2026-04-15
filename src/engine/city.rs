use unicode_normalization::UnicodeNormalization;

type UnicodeNormalizedIter<'a> = std::iter::FilterMap<
    std::iter::FlatMap<
        std::iter::Filter<
            unicode_normalization::Decompositions<std::str::Chars<'a>>,
            fn(&char) -> bool,
        >,
        std::char::ToLowercase,
        fn(char) -> std::char::ToLowercase,
    >,
    fn(char) -> Option<u8>,
>;

#[inline]
fn keep_non_punctuation(c: &char) -> bool {
    !c.is_ascii_punctuation()
}

#[inline]
fn to_lowercase_chars(c: char) -> std::char::ToLowercase {
    c.to_lowercase()
}

#[inline]
fn to_ascii_byte(c: char) -> Option<u8> {
    if c.is_ascii() { Some(c as u8) } else { None }
}

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct City {
    pub geoname_id: u32,
    pub country_code: String,
    pub name: String,
    pub ascii: String,
    pub admin1_code: Option<String>,
    pub admin1_name: Option<String>,
    pub admin2_code: Option<String>,
    pub admin2_name: Option<String>,
    pub lat: f32,
    pub lon: f32,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct CityCore {
    pub geoname_id: u32,
    pub country_code_id: u32,
    pub name_id: u32,
    pub ascii_id: u32,
    pub admin1_code_id: Option<u32>,
    pub admin1_name_id: Option<u32>,
    pub admin2_code_id: Option<u32>,
    pub admin2_name_id: Option<u32>,
    pub lat: f32,
    pub lon: f32,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
pub struct CityMeta {
    pub strings: Vec<String>,
}

// ── Normalization Core ────────────────────────────────────────────────────────

/// Streaming normalized iterator (zero allocation).
/// - NFKD normalization
/// - removes ASCII punctuation
/// - lowercases
/// - emits ASCII bytes only (FST-friendly)
#[inline]
pub fn normalize_iter(s: &str) -> UnicodeNormalizedIter<'_> {
    s.nfkd()
        .filter(keep_non_punctuation as fn(&char) -> bool)
        .flat_map(to_lowercase_chars as fn(char) -> std::char::ToLowercase)
        .filter_map(to_ascii_byte as fn(char) -> Option<u8>)
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
    Unicode(UnicodeNormalizedIter<'a>),
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
#[allow(dead_code)]
pub fn insert_normalized<K: AsRef<str>>(
    builder: &mut fst::MapBuilder<Vec<u8>>,
    key: K,
    value: u64,
    buf: &mut Vec<u8>,
) -> std::io::Result<()> {
    normalize_to_buf(key.as_ref(), buf);
    builder
        .insert(&buf, value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}
