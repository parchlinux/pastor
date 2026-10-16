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

        // 1. LANGUAGE can be colon-separated list (e.g. "fa_IR:fa:en_US:en")
        if let Ok(lang_var) = std::env::var("LANGUAGE") {
            for part in lang_var.split(':') {
                let trimmed = clean_locale(part);
                if !trimmed.is_empty() && !preferred.contains(&trimmed) {
                    preferred.push(trimmed);
                }
            }
        }

        // 2. LC_ALL, LC_MESSAGES, LANG
        for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let trimmed = clean_locale(&val);
                if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" && !preferred.contains(&trimmed) {
                    preferred.push(trimmed);
                }
            }
        }

        // 3. For each locale like "fa_IR", also add the primary language code "fa"
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

        // 4. Always add English fallbacks at the end if not already present
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
}
