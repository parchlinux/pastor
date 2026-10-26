use std::collections::HashMap;
use std::sync::RwLock;

static LANGUAGE_OVERRIDE: RwLock<Option<String>> = RwLock::new(None);
static CATALOG_REGISTRY: RwLock<Option<HashMap<String, Option<&'static HashMap<String, &'static str>>>>> =
    RwLock::new(None);

#[derive(Debug, Clone)]
pub struct LocaleMatcher {
    preferred: Vec<String>,
}

impl Default for LocaleMatcher {
    fn default() -> Self {
        Self::from_env()
    }
}

impl LocaleMatcher {
    pub fn from_env() -> Self {
        let mut preferred = Vec::new();

        // 1. Explicit runtime override if set
        if let Ok(guard) = LANGUAGE_OVERRIDE.read() {
            if let Some(ref lang) = *guard {
                let cleaned = clean_locale(lang);
                if !cleaned.is_empty() {
                    preferred.push(cleaned);
                }
            }
        }

        // 2. POSIX locale message priorities: LC_ALL > LC_MESSAGES > LANG
        for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let trimmed = clean_locale(&val);
                if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" && !preferred.contains(&trimmed) {
                    preferred.push(trimmed);
                }
            }
        }

        // 3. GNU LANGUAGE variable (may be colon-separated fallback list e.g. "fa_IR:fa:en")
        if let Ok(lang_var) = std::env::var("LANGUAGE") {
            for part in lang_var.split(':') {
                let trimmed = clean_locale(part);
                if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" && !preferred.contains(&trimmed) {
                    preferred.push(trimmed);
                }
            }
        }

        // 4. For each locale like "fa_IR", also add the primary language code "fa"
        let mut with_bases = Vec::new();
        for loc in &preferred {
            with_bases.push(loc.clone());
            if let Some((base, _)) = loc.split_once('_') {
                let b = base.to_string();
                if !with_bases.contains(&b) {
                    with_bases.push(b);
                }
            }
        }

        // 5. Always add English fallbacks at the end if not already present
        for fallback in &["en_US", "en"] {
            let fb = fallback.to_string();
            if !with_bases.contains(&fb) {
                with_bases.push(fb);
            }
        }

        Self { preferred: with_bases }
    }

    pub fn new(preferred: Vec<String>) -> Self {
        Self { preferred }
    }

    pub fn primary_locale(&self) -> &str {
        self.preferred.first().map(|s| s.as_str()).unwrap_or("en")
    }

    /// Evaluates how well an `xml:lang` matches the user's preferred locales.
    /// Returns `Some(priority)` where lower numbers mean a better match (0 is best).
    /// Returns `None` if the language does not match the system locale or English fallback.
    pub fn score(&self, xml_lang: Option<&str>) -> Option<usize> {
        let lang = match xml_lang {
            None => return Some(self.preferred.len() + 1), // Untagged default (fallback after English)
            Some(l) => {
                let c = clean_locale(l);
                if c.is_empty() || c == "C" || c == "POSIX" {
                    return Some(self.preferred.len() + 1);
                }
                c
            }
        };

        // Try exact match in preferred list
        if let Some(idx) = self.preferred.iter().position(|p| p.eq_ignore_ascii_case(&lang)) {
            return Some(idx);
        }

        // Try matching language base (e.g. xml:lang="fa-IR" -> base "fa" matching preferred "fa")
        let lang_base = lang.split('_').next().unwrap_or(&lang);
        if let Some(idx) = self.preferred.iter().position(|p| p.eq_ignore_ascii_case(lang_base)) {
            return Some(idx + 1);
        }

        // Not in user's system language or English fallbacks -> reject
        None
    }

    pub fn is_allowed(&self, xml_lang: Option<&str>) -> bool {
        self.score(xml_lang).is_some()
    }
}

pub fn clean_locale(raw: &str) -> String {
    let s = raw.trim();
    // Strip encoding: e.g. fa_IR.UTF-8 -> fa_IR
    let s = s.split('.').next().unwrap_or(s);
    // Strip modifier: e.g. en_US@euro -> en_US
    let s = s.split('@').next().unwrap_or(s);
    // Normalize hyphen to underscore: e.g. fa-IR -> fa_IR
    s.replace('-', "_")
}

/// Returns the currently active language code (e.g. "fa_IR", "fa", "en_US").
pub fn active_language() -> String {
    // 1. Explicit runtime override
    if let Ok(guard) = LANGUAGE_OVERRIDE.read() {
        if let Some(ref lang) = *guard {
            let cleaned = clean_locale(lang);
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }

    // 2. POSIX locale message priority: LC_ALL > LC_MESSAGES > LANG
    for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let val = val.trim();
            if !val.is_empty() && val != "C" && val != "POSIX" {
                let cleaned = clean_locale(val);
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
        }
    }

    // 3. GNU LANGUAGE variable (colon-separated fallback list)
    if let Ok(lang_var) = std::env::var("LANGUAGE") {
        for part in lang_var.split(':') {
            let trimmed = clean_locale(part);
            if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" {
                return trimmed;
            }
        }
    }

    "en_US".to_string()
}

/// Sets an explicit language override (e.g. `Some("fa")`, `Some("de")`, or `None` to reset).
pub fn set_language(lang: Option<&str>) {
    if let Ok(mut ov) = LANGUAGE_OVERRIDE.write() {
        *ov = lang.map(clean_locale);
    }
}

/// Checks whether a given language uses right-to-left (RTL) script.
pub fn is_rtl_language(lang: &str) -> bool {
    let clean = clean_locale(lang).to_lowercase();
    let base = clean.split('_').next().unwrap_or(&clean);
    matches!(base, "fa" | "ar" | "he" | "ur" | "ckb" | "sd" | "ps" | "ug" | "yi")
}

/// Returns true if the currently active language is a right-to-left language.
pub fn is_rtl() -> bool {
    is_rtl_language(&active_language())
}

/// Checks whether the user's active locale is Persian (Farsi).
pub fn is_persian() -> bool {
    let lang = active_language();
    let clean = clean_locale(&lang).to_lowercase();
    clean == "fa" || clean.starts_with("fa_") || clean.starts_with("fa-")
}

/// Allows overriding Persian mode explicitly (useful for backward compatibility and testing).
pub fn set_persian_mode(enabled: Option<bool>) {
    match enabled {
        Some(true) => set_language(Some("fa_IR")),
        Some(false) => set_language(Some("en_US")),
        None => set_language(None),
    }
}

/// Parsed GNU gettext `.po` catalog.
#[derive(Debug, Clone)]
pub struct PoCatalog {
    entries: HashMap<String, String>,
}

impl PoCatalog {
    pub fn parse(po_content: &str) -> Self {
        let mut entries = HashMap::new();
        let mut current_id = String::new();
        let mut current_str = String::new();
        let mut in_id = false;
        let mut in_str = false;

        for line in po_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                if !current_id.is_empty() && !current_str.is_empty() {
                    entries.insert(current_id.clone(), current_str.clone());
                    current_id.clear();
                    current_str.clear();
                }
                in_id = false;
                in_str = false;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("msgid ") {
                if !current_id.is_empty() && !current_str.is_empty() {
                    entries.insert(current_id.clone(), current_str.clone());
                    current_id.clear();
                    current_str.clear();
                }
                in_id = true;
                in_str = false;
                current_id = unescape_po_string(rest.trim());
            } else if let Some(rest) = trimmed.strip_prefix("msgstr ") {
                in_id = false;
                in_str = true;
                current_str = unescape_po_string(rest.trim());
            } else if trimmed.starts_with('"') && trimmed.ends_with('"') {
                let unescaped = unescape_po_string(trimmed);
                if in_id {
                    current_id.push_str(&unescaped);
                } else if in_str {
                    current_str.push_str(&unescaped);
                }
            }
        }

        if !current_id.is_empty() && !current_str.is_empty() {
            entries.insert(current_id, current_str);
        }

        Self { entries }
    }

    /// Parse compiled GNU gettext `.mo` binary format.
    pub fn parse_mo(data: &[u8]) -> Option<Self> {
        if data.len() < 24 {
            return None;
        }
        let magic = u32::from_le_bytes(data[0..4].try_into().ok()?);
        if magic != 0x950412de && magic != 0xde120495 {
            return None;
        }
        let is_le = magic == 0x950412de;
        let read_u32 = |offset: usize| -> Option<usize> {
            let bytes: [u8; 4] = data.get(offset..offset + 4)?.try_into().ok()?;
            if is_le {
                Some(u32::from_le_bytes(bytes) as usize)
            } else {
                Some(u32::from_be_bytes(bytes) as usize)
            }
        };

        let num_strings = read_u32(8)?;
        let orig_table_offset = read_u32(12)?;
        let trans_table_offset = read_u32(16)?;

        let mut entries = HashMap::with_capacity(num_strings);
        for i in 0..num_strings {
            let orig_len = read_u32(orig_table_offset + i * 8)?;
            let orig_off = read_u32(orig_table_offset + i * 8 + 4)?;
            let trans_len = read_u32(trans_table_offset + i * 8)?;
            let trans_off = read_u32(trans_table_offset + i * 8 + 4)?;

            let orig_bytes = data.get(orig_off..orig_off + orig_len)?;
            let trans_bytes = data.get(trans_off..trans_off + trans_len)?;

            let orig = std::str::from_utf8(orig_bytes).ok()?;
            let trans = std::str::from_utf8(trans_bytes).ok()?;

            if !orig.is_empty() && !trans.is_empty() {
                entries.insert(orig.to_string(), trans.to_string());
            }
        }
        Some(Self { entries })
    }

    pub fn get(&self, msgid: &str) -> Option<&str> {
        self.entries.get(msgid).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn unescape_po_string(raw: &str) -> String {
    let inner = raw.strip_prefix('"').unwrap_or(raw);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

static PERSIAN_CATALOG: std::sync::OnceLock<PoCatalog> = std::sync::OnceLock::new();

/// Returns the Persian translation catalog parsed from system `.mo` file or embedded `po/fa.po`.
pub fn persian_po_catalog() -> &'static PoCatalog {
    PERSIAN_CATALOG.get_or_init(|| {
        for path in &[
            "/usr/share/locale/fa/LC_MESSAGES/pastor.mo",
            "/usr/share/locale/fa_IR/LC_MESSAGES/pastor.mo",
            "/usr/local/share/locale/fa/LC_MESSAGES/pastor.mo",
            "po/fa.mo",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                if let Some(cat) = PoCatalog::parse_mo(&bytes) {
                    if !cat.is_empty() {
                        return cat;
                    }
                }
            }
        }
        const EMBEDDED_FA_PO: &str = include_str!("../../../po/fa.po");
        PoCatalog::parse(EMBEDDED_FA_PO)
    })
}

fn load_catalog_for_lang(lang: &str) -> Option<&'static HashMap<String, &'static str>> {
    let clean = clean_locale(lang);
    let base = clean.split('_').next().unwrap_or(&clean);

    let search_paths = [
        format!("/usr/share/locale/{clean}/LC_MESSAGES/pastor.mo"),
        format!("/usr/share/locale/{base}/LC_MESSAGES/pastor.mo"),
        format!("/usr/local/share/locale/{clean}/LC_MESSAGES/pastor.mo"),
        format!("/usr/local/share/locale/{base}/LC_MESSAGES/pastor.mo"),
        format!("po/{clean}.mo"),
        format!("po/{base}.mo"),
    ];

    let mut parsed: Option<PoCatalog> = None;
    for path in &search_paths {
        if let Ok(bytes) = std::fs::read(path) {
            if let Some(cat) = PoCatalog::parse_mo(&bytes) {
                if !cat.is_empty() {
                    parsed = Some(cat);
                    break;
                }
            }
        }
    }

    if parsed.is_none() && base == "fa" {
        const EMBEDDED_FA_PO: &str = include_str!("../../../po/fa.po");
        parsed = Some(PoCatalog::parse(EMBEDDED_FA_PO));
    }

    let cat = parsed?;
    let mut map: HashMap<String, &'static str> = HashMap::with_capacity(cat.entries.len());
    for (k, v) in cat.entries {
        let v_static: &'static str = Box::leak(v.into_boxed_str());
        map.insert(k, v_static);
    }
    let map_static: &'static HashMap<String, &'static str> = Box::leak(Box::new(map));
    Some(map_static)
}

fn get_or_load_catalog(lang: &str) -> Option<&'static HashMap<String, &'static str>> {
    let clean = clean_locale(lang);
    let base = clean.split('_').next().unwrap_or(&clean).to_string();

    {
        if let Ok(reg) = CATALOG_REGISTRY.read() {
            if let Some(ref cache) = *reg {
                if let Some(entry) = cache.get(&clean).or_else(|| cache.get(&base)) {
                    return *entry;
                }
            }
        }
    }

    let loaded = load_catalog_for_lang(&clean);

    if let Ok(mut reg) = CATALOG_REGISTRY.write() {
        let cache = reg.get_or_insert_with(HashMap::new);
        cache.insert(clean, loaded);
        cache.insert(base, loaded);
    }

    loaded
}

/// Translates a given UI string to the active locale using GNU gettext catalogs.
pub fn tr(text: &str) -> &str {
    let lang = active_language();
    if lang.is_empty() || lang == "C" || lang == "POSIX" {
        return text;
    }
    let base = lang.split('_').next().unwrap_or(&lang);
    if base.eq_ignore_ascii_case("en") {
        return text;
    }

    if let Some(cat) = get_or_load_catalog(&lang) {
        if let Some(&trans) = cat.get(text) {
            if !trans.is_empty() {
                return trans;
            }
        }
    }
    text
}

/// Variant of `tr` for static strings that preserves the `'static` lifetime.
pub fn tr_static(text: &'static str) -> &'static str {
    let lang = active_language();
    if lang.is_empty() || lang == "C" || lang == "POSIX" {
        return text;
    }
    let base = lang.split('_').next().unwrap_or(&lang);
    if base.eq_ignore_ascii_case("en") {
        return text;
    }

    if let Some(cat) = get_or_load_catalog(&lang) {
        if let Some(&trans) = cat.get(text) {
            if !trans.is_empty() {
                return trans;
            }
        }
    }
    text
}

#[derive(Debug, Clone, Default)]
pub struct LocalizedField {
    pub best_score: usize,
    pub value: String,
}

impl LocalizedField {
    pub fn new() -> Self {
        Self {
            best_score: usize::MAX,
            value: String::new(),
        }
    }

    pub fn update(&mut self, score: usize, val: &str) {
        let val_trimmed = val.trim();
        if !val_trimmed.is_empty() && score < self.best_score {
            self.best_score = score;
            self.value = val_trimmed.to_string();
        }
    }

    pub fn take(&mut self) -> Option<String> {
        if self.value.is_empty() {
            None
        } else {
            self.best_score = usize::MAX;
            Some(std::mem::take(&mut self.value))
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if self.value.is_empty() {
            None
        } else {
            Some(&self.value)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalizedDescription {
    pub best_score: usize,
    pub best_paragraphs: Vec<String>,
    pub current_block_score: Option<usize>,
    pub current_block_paragraphs: Vec<String>,
}

impl LocalizedDescription {
    pub fn new() -> Self {
        Self {
            best_score: usize::MAX,
            best_paragraphs: Vec::new(),
            current_block_score: None,
            current_block_paragraphs: Vec::new(),
        }
    }

    pub fn start_description_tag(&mut self, lang_score: Option<usize>) {
        self.current_block_score = lang_score;
        self.current_block_paragraphs.clear();
    }

    pub fn add_paragraph(&mut self, p_lang_score: Option<usize>, text: &str) {
        let t = text.trim();
        if t.is_empty() {
            return;
        }

        // If p has an explicit lang score, check it; otherwise fallback to block score
        let effective_score = if p_lang_score.is_some() {
            p_lang_score
        } else {
            self.current_block_score
        };

        if let Some(score) = effective_score {
            if self.current_block_score.is_none() {
                // Inline paragraph-level localization
                if score < self.best_score {
                    self.best_score = score;
                    self.best_paragraphs = vec![t.to_string()];
                } else if score == self.best_score {
                    self.best_paragraphs.push(t.to_string());
                }
            } else {
                self.current_block_paragraphs.push(t.to_string());
            }
        }
    }

    pub fn end_description_tag(&mut self) {
        if let Some(score) = self.current_block_score {
            if !self.current_block_paragraphs.is_empty() && score < self.best_score {
                self.best_score = score;
                self.best_paragraphs = std::mem::take(&mut self.current_block_paragraphs);
            }
        }
        self.current_block_paragraphs.clear();
        self.current_block_score = None;
    }

    pub fn finish(&mut self) -> Option<String> {
        self.end_description_tag();
        let res = if self.best_paragraphs.is_empty() {
            None
        } else {
            Some(self.best_paragraphs.join("\n\n"))
        };
        self.best_score = usize::MAX;
        self.best_paragraphs.clear();
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_locale_matching_english() {
        let matcher = LocaleMatcher::new(vec!["en_US".into(), "en".into()]);
        assert_eq!(matcher.score(Some("en_US")), Some(0));
        assert_eq!(matcher.score(Some("en")), Some(1));
        assert!(matcher.score(None).is_some());
        assert_eq!(matcher.score(Some("fa")), None);
        assert_eq!(matcher.score(Some("de")), None);
    }

    #[test]
    fn test_locale_matching_persian_preference() {
        let matcher = LocaleMatcher::new(vec!["fa_IR".into(), "fa".into(), "en_US".into(), "en".into()]);
        let fa_score = matcher.score(Some("fa")).unwrap();
        let en_score = matcher.score(Some("en")).unwrap();
        assert!(fa_score < en_score);
        assert_eq!(matcher.score(Some("de")), None);
    }

    #[test]
    fn test_localized_description_selection() {
        let matcher = LocaleMatcher::new(vec!["fa_IR".into(), "fa".into(), "en_US".into(), "en".into()]);
        let mut desc = LocalizedDescription::new();

        // 1. English description block
        desc.start_description_tag(matcher.score(Some("en")));
        desc.add_paragraph(None, "English paragraph 1");
        desc.add_paragraph(None, "English paragraph 2");
        desc.end_description_tag();

        // 2. German description block (should be ignored)
        desc.start_description_tag(matcher.score(Some("de")));
        desc.add_paragraph(None, "Deutscher Absatz 1");
        desc.end_description_tag();

        // 3. Persian description block (should win over English)
        desc.start_description_tag(matcher.score(Some("fa")));
        desc.add_paragraph(None, "پاراگراف فارسی ۱");
        desc.add_paragraph(None, "پاراگراف فارسی ۲");
        desc.end_description_tag();

        let final_text = desc.finish().unwrap();
        assert_eq!(final_text, "پاراگراف فارسی ۱\n\nپاراگراف فارسی ۲");
    }

    #[test]
    fn test_persian_ui_translations() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let cat = persian_po_catalog();
        assert!(cat.len() > 100);
        assert_eq!(cat.get("Explore"), Some("کاوش"));
        assert_eq!(cat.get("Installed"), Some("نصب‌شده"));
        assert_eq!(cat.get("Updates"), Some("به‌روزرسانی‌ها"));
        assert_eq!(cat.get("Parch Store"), Some("فروشگاه پارچ"));

        set_persian_mode(Some(false));
        assert_eq!(tr("Explore"), "Explore");
        assert_eq!(tr("Installed"), "Installed");
        assert_eq!(tr("Updates"), "Updates");

        set_persian_mode(Some(true));
        assert_eq!(tr("Explore"), "کاوش");
        assert_eq!(tr("Installed"), "نصب‌شده");
        assert_eq!(tr("Updates"), "به‌روزرسانی‌ها");
        assert_eq!(tr("Install"), "نصب");
        assert_eq!(tr("Uninstall"), "حذف");
        assert_eq!(tr("Share Application"), "اشتراک‌گذاری برنامه");
        assert_eq!(tr("Flathub (Flatpak)"), "فلت‌هاب (فلت‌پک)");

        // Reset
        set_persian_mode(None);
    }

    #[test]
    fn test_multi_language_and_rtl_resolution() {
        let _guard = TEST_MUTEX.lock().unwrap();
        // Test RTL detection
        assert!(is_rtl_language("fa"));
        assert!(is_rtl_language("fa_IR"));
        assert!(is_rtl_language("ar_EG"));
        assert!(is_rtl_language("he"));
        assert!(is_rtl_language("ur"));
        assert!(!is_rtl_language("en_US"));
        assert!(!is_rtl_language("de_DE"));
        assert!(!is_rtl_language("fr"));

        // Test explicit set_language
        set_language(Some("fa_IR"));
        assert_eq!(active_language(), "fa_IR");
        assert!(is_persian());
        assert!(is_rtl());
        assert_eq!(tr("Explore"), "کاوش");
        assert_eq!(tr_static("Explore"), "کاوش");

        set_language(Some("en_US"));
        assert_eq!(active_language(), "en_US");
        assert!(!is_persian());
        assert!(!is_rtl());
        assert_eq!(tr("Explore"), "Explore");

        // Clean reset
        set_language(None);
    }

    #[test]
    fn test_po_and_mo_parsing() {
        let po_data = r#"
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"

msgid "Hello"
msgstr "درود"

msgid "Goodbye"
msgstr "بدرود"
"#;
        let cat = PoCatalog::parse(po_data);
        assert_eq!(cat.get("Hello"), Some("درود"));
        assert_eq!(cat.get("Goodbye"), Some("بدرود"));

        // Synthetic MO binary buffer for "Test" -> "آزمون"
        let orig_str = b"Test\0";
        let trans_str = "آزمون\0".as_bytes();
        let orig_offset: u32 = 44;
        let trans_offset: u32 = orig_offset + orig_str.len() as u32;

        let mut mo_bytes = Vec::new();
        mo_bytes.extend_from_slice(&0x950412de_u32.to_le_bytes()); // magic
        mo_bytes.extend_from_slice(&0_u32.to_le_bytes());          // revision
        mo_bytes.extend_from_slice(&1_u32.to_le_bytes());          // num_strings
        mo_bytes.extend_from_slice(&28_u32.to_le_bytes());         // orig_table_offset
        mo_bytes.extend_from_slice(&36_u32.to_le_bytes());         // trans_table_offset
        mo_bytes.extend_from_slice(&0_u32.to_le_bytes());          // hash table size
        mo_bytes.extend_from_slice(&0_u32.to_le_bytes());          // hash table offset
        // Orig table entry: len, offset
        mo_bytes.extend_from_slice(&(orig_str.len() as u32 - 1).to_le_bytes());
        mo_bytes.extend_from_slice(&orig_offset.to_le_bytes());
        // Trans table entry: len, offset
        mo_bytes.extend_from_slice(&(trans_str.len() as u32 - 1).to_le_bytes());
        mo_bytes.extend_from_slice(&trans_offset.to_le_bytes());
        // Strings data
        mo_bytes.extend_from_slice(orig_str);
        mo_bytes.extend_from_slice(trans_str);

        let parsed_mo = PoCatalog::parse_mo(&mo_bytes).expect("Failed to parse valid synthetic MO binary");
        assert_eq!(parsed_mo.get("Test"), Some("آزمون"));
    }
}
