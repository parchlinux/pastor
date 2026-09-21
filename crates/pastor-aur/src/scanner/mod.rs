pub mod rules;

use std::path::Path;
use pastor_core::error::PastorError;
pub use rules::{RuleSeverity, SecurityRule, SECURITY_RULES};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFinding {
    pub file: String,
    pub line_number: usize,
    pub line_content: String,
    pub rule_id: String,
    pub title: String,
    pub severity: RuleSeverity,
    pub description: String,
}

impl ScanFinding {
    pub fn new(
        file: impl Into<String>,
        line_number: usize,
        line_content: impl Into<String>,
        rule_id: impl Into<String>,
        title: impl Into<String>,
        severity: RuleSeverity,
        description: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line_number,
            line_content: line_content.into(),
            rule_id: rule_id.into(),
            title: title.into(),
            severity,
            description: description.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanReport {
    pub findings: Vec<ScanFinding>,
}

impl ScanReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn has_critical(&self) -> bool {
        self.findings.iter().any(|f| f.severity == RuleSeverity::Critical)
    }

    pub fn has_high(&self) -> bool {
        self.findings.iter().any(|f| f.severity >= RuleSeverity::High)
    }

    pub fn risk_score(&self) -> u32 {
        self.findings
            .iter()
            .map(|f| match f.severity {
                RuleSeverity::Low => 10,
                RuleSeverity::Medium => 30,
                RuleSeverity::High => 70,
                RuleSeverity::Critical => 100,
            })
            .sum()
    }

    pub fn summary_text(&self) -> String {
        if self.is_clean() {
            "✅ Static security scan passed. No suspicious patterns detected.".to_string()
        } else {
            let critical = self.findings.iter().filter(|f| f.severity == RuleSeverity::Critical).count();
            let high = self.findings.iter().filter(|f| f.severity == RuleSeverity::High).count();
            let medium = self.findings.iter().filter(|f| f.severity == RuleSeverity::Medium).count();
            let low = self.findings.iter().filter(|f| f.severity == RuleSeverity::Low).count();

            format!(
                "⚠️ Found {} potential security warning(s): {} Critical, {} High, {} Medium, {} Low",
                self.findings.len(),
                critical,
                high,
                medium,
                low
            )
        }
    }
}

pub struct HeuristicScanner;

impl HeuristicScanner {
    pub fn scan_content(filename: &str, content: &str) -> Vec<ScanFinding> {
        let mut findings = Vec::new();

        for (idx, raw_line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = raw_line.trim();

            // Ignore pure comments
            if trimmed.starts_with('#') {
                continue;
            }

            // Check each rule
            Self::check_npm_bun_dropper(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_remote_pipe(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_base64_eval(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_credential_harvesting(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_raw_ip_target(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_privilege_escalation(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_disk_corruption(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_reverse_shell(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_persistence(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_url_shortener(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_data_exfiltration(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_checksum_skip(filename, line_num, raw_line, trimmed, &mut findings);
            Self::check_insecure_http(filename, line_num, raw_line, trimmed, &mut findings);
        }

        findings
    }

    pub fn scan_file(path: &Path) -> Result<Vec<ScanFinding>, PastorError> {
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("unknown");
        let content = std::fs::read_to_string(path).map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Failed to read file {}: {e}", path.display()),
        })?;
        Ok(Self::scan_content(filename, &content))
    }

    fn check_npm_bun_dropper(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let patterns = [
            "npm install",
            "npm i ",
            "npx ",
            "bun install",
            "bun add",
            "pnpm install",
            "pnpm add",
            "yarn add",
            "yarn install",
        ];

        for p in patterns {
            if lower.contains(p) {
                findings.push(ScanFinding::new(
                    file,
                    line,
                    raw,
                    "AUR-R001-NPM-BUN-DROPPER",
                    "Runtime Package Manager Dropper",
                    RuleSeverity::Critical,
                    "Explicit invocation of npm/bun/pnpm/yarn inside package script. Matches 2026 supply-chain attack vectors.",
                ));
                break;
            }
        }
    }

    fn check_remote_pipe(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let dl_tools = ["curl", "wget", "fetch"];
        let sh_targets = ["| bash", "| sh", "| zsh", "| dash", "| python", "|eval"];

        for tool in dl_tools {
            if let Some(pos) = lower.find(tool) {
                let rest = &lower[pos..];
                for sh in sh_targets {
                    if rest.contains(sh) {
                        findings.push(ScanFinding::new(
                            file,
                            line,
                            raw,
                            "AUR-R002-REMOTE-PIPE-EXEC",
                            "Remote Code Execution via Pipe",
                            RuleSeverity::Critical,
                            "Pipes downloaded content directly to a shell interpreter.",
                        ));
                        return;
                    }
                }
            }
        }
    }

    fn check_base64_eval(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        if (lower.contains("base64 -d") || lower.contains("base64 --decode"))
            && (lower.contains("| sh") || lower.contains("| bash") || lower.contains("| eval") || lower.contains("|sh"))
        {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R003-BASE64-EVAL",
                "Obfuscated Base64 Eval Execution",
                RuleSeverity::Critical,
                "Decodes base64 string directly into shell evaluation.",
            ));
        }
    }

    fn check_credential_harvesting(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let targets = [
            ".ssh/id_rsa",
            ".ssh/id_ed25519",
            ".ssh/authorized_keys",
            ".ssh/known_hosts",
            ".aws/credentials",
            ".config/gh/hosts",
            ".docker/config.json",
            ".kube/config",
            ".mozilla/firefox",
            ".config/google-chrome",
            ".config/chromium",
            "key4.db",
            "login data",
        ];

        for t in targets {
            if lower.contains(t) {
                findings.push(ScanFinding::new(
                    file,
                    line,
                    raw,
                    "AUR-R004-CREDENTIAL-HARVESTING",
                    "Credential & Secret Path Access",
                    RuleSeverity::High,
                    "Accesses sensitive user credential store, SSH keys, or browser secrets.",
                ));
                break;
            }
        }
    }

    fn check_raw_ip_target(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let prefixes = ["http://", "https://"];
        for prefix in prefixes {
            if let Some(idx) = lower.find(prefix) {
                let rest = &lower[idx + prefix.len()..];
                let host_part = rest.split(&['/', ':', ' ', '"', '\''][..]).next().unwrap_or("");
                let segments: Vec<&str> = host_part.split('.').collect();
                if segments.len() == 4 && segments.iter().all(|s| s.parse::<u8>().is_ok()) {
                    findings.push(ScanFinding::new(
                        file,
                        line,
                        raw,
                        "AUR-R005-RAW-IP-TARGET",
                        "Hardcoded External IP URL",
                        RuleSeverity::High,
                        "Network target references raw IP address instead of registered domain.",
                    ));
                    return;
                }
            }
        }
    }

    fn check_privilege_escalation(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("sudo ")
            || lower.contains("; sudo ")
            || lower.starts_with("su ")
            || lower.contains("; su ")
            || lower.contains("chmod +s")
            || lower.contains("chmod u+s")
            || lower.contains("chmod 4755")
        {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R006-PRIVILEGE-ESCALATION",
                "Privilege Escalation or SUID Modification",
                RuleSeverity::High,
                "Invokes sudo/su or configures SUID executable flags inside package script.",
            ));
        }
    }

    fn check_disk_corruption(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        if (lower.contains("dd ") && lower.contains("of=/dev/"))
            || lower.contains("mkfs.")
        {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R007-RAW-DEVICE-WRITE",
                "Raw Block Device / Disk Write",
                RuleSeverity::Critical,
                "Direct write to raw block storage device (/dev/sd, /dev/nvme, etc.).",
            ));
        }
    }

    fn check_checksum_skip(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        if (lower.starts_with("sha256sums=")
            || lower.starts_with("md5sums=")
            || lower.starts_with("sha512sums=")
            || lower.starts_with("b2sums="))
            && (lower.contains("('skip')") || lower.contains("(\"skip\")"))
        {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R008-CHECKSUM-SKIP",
                "Checksums Set to SKIP",
                RuleSeverity::Medium,
                "Integrity hashes are set to SKIP, allowing arbitrary source tampering.",
            ));
        }
    }

    fn check_insecure_http(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("source=") && lower.contains("http://") && !lower.contains("https://") {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R009-INSECURE-HTTP",
                "Unencrypted HTTP Source Download",
                RuleSeverity::Low,
                "Sources are fetched over plaintext HTTP, vulnerable to MITM attacks.",
            ));
        }
    }

    fn check_reverse_shell(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let is_nc_e = (lower.contains("nc ") || lower.contains("ncat ")) && lower.contains(" -e ");
        let is_bash_tcp = lower.contains("/dev/tcp/") || lower.contains("/dev/udp/");
        let is_py_reverse = lower.contains("python") && lower.contains("socket") && (lower.contains("dup2") || lower.contains("spawn"));

        if is_nc_e || is_bash_tcp || is_py_reverse {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R010-REVERSE-SHELL",
                "Interactive Reverse Shell Backdoor",
                RuleSeverity::Critical,
                "Pattern characteristic of a reverse shell backdoor (nc -e, bash /dev/tcp, or python socket one-liner).",
            ));
        }
    }

    fn check_persistence(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let targets = [
            ".bashrc",
            ".zshrc",
            ".profile",
            ".bash_profile",
            "authorized_keys",
            "/etc/cron",
            "/etc/sudoers",
        ];

        if lower.contains(">>") || lower.contains("tee ") {
            for t in targets {
                if lower.contains(t) {
                    findings.push(ScanFinding::new(
                        file,
                        line,
                        raw,
                        "AUR-R011-PERSISTENCE-BACKDOOR",
                        "Persistence Mechanism / Startup Script Modification",
                        RuleSeverity::High,
                        "Appends to user shell startup profiles (.bashrc, .zshrc) or system cron/account files.",
                    ));
                    break;
                }
            }
        }
    }

    fn check_url_shortener(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        let shorteners = [
            "bit.ly/",
            "tinyurl.com/",
            "t.co/",
            "goo.gl/",
            "is.gd/",
            "rebrand.ly/",
        ];

        for s in shorteners {
            if lower.contains(s) {
                findings.push(ScanFinding::new(
                    file,
                    line,
                    raw,
                    "AUR-R012-URL-SHORTENER",
                    "Obfuscated URL Shortener Source",
                    RuleSeverity::High,
                    "Source points to an obfuscated URL shortener, hiding the real upstream download location.",
                ));
                break;
            }
        }
    }

    fn check_data_exfiltration(
        file: &str,
        line: usize,
        raw: &str,
        trimmed: &str,
        findings: &mut Vec<ScanFinding>,
    ) {
        let lower = trimmed.to_ascii_lowercase();
        if (lower.contains("curl ") || lower.contains("wget "))
            && (lower.contains("-x post") || lower.contains("--data") || lower.contains(" -d ") || lower.contains(" -f "))
            && (lower.contains("http://") || lower.contains("https://"))
        {
            findings.push(ScanFinding::new(
                file,
                line,
                raw,
                "AUR-R013-DATA-EXFILTRATION",
                "Outbound Data Exfiltration via HTTP POST",
                RuleSeverity::High,
                "Sends data via HTTP POST or form upload during package build.",
            ));
        }
    }
}
