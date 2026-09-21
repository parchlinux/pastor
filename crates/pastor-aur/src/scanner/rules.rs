use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuleSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl RuleSeverity {
    pub fn badge_name(&self) -> &'static str {
        match self {
            Self::Low => "Low Risk",
            Self::Medium => "Medium Warning",
            Self::High => "High Risk",
            Self::Critical => "CRITICAL THREAT",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Low => "dim-label",
            Self::Medium => "warning",
            Self::High => "error",
            Self::Critical => "destructive",
        }
    }
}

pub struct SecurityRule {
    pub id: &'static str,
    pub title: &'static str,
    pub severity: RuleSeverity,
    pub description: &'static str,
    pub pattern: &'static str,
}

pub const SECURITY_RULES: &[SecurityRule] = &[
    // 1. 2026 Supply Chain Dropper Attacks (Atomic Arch & Variants)
    SecurityRule {
        id: "AUR-R001-NPM-BUN-DROPPER",
        title: "Runtime Package Manager Dropper",
        severity: RuleSeverity::Critical,
        description: "Direct invocation of npm, bun, pnpm, or yarn inside build() or .install hook. Matches the 2026 Atomic Arch supply-chain campaign.",
        pattern: r"(npm\s+install|npx\s+|bun\s+install|bun\s+add|pnpm\s+install|pnpm\s+add|yarn\s+add|yarn\s+install)",
    },
    // 2. Remote Shell Pipe Executions (aur-guard AG001, AG002, AG003)
    SecurityRule {
        id: "AUR-R002-REMOTE-PIPE-EXEC",
        title: "Remote Code Execution via Pipe",
        severity: RuleSeverity::Critical,
        description: "Piping remote download directly into a shell interpreter (e.g. curl | bash, wget | sh, eval $(curl ...)).",
        pattern: r"((curl|wget|fetch)\b[^\n|]+\|\s*(ba?sh|zsh|dash|python|sh))",
    },
    // 3. Obfuscated Base64 / Hex / Eval Droppers (aur-guard AG050, AG051)
    SecurityRule {
        id: "AUR-R003-BASE64-EVAL",
        title: "Obfuscated Base64 / Hex Eval Execution",
        severity: RuleSeverity::Critical,
        description: "Decoding base64 or hex encrypted payloads and piping into eval or shell interpreter.",
        pattern: r"(base64\s+(-d|--decode)[^\n|]*\|\s*(ba?sh|eval|sh)|openssl\s+enc\s+-d[^\n|]*\|\s*sh|xxd\s+-r[^\n|]*\|\s*sh)",
    },
    // 4. Credential & Private Key Harvesting (aur-guard AG070, AG071)
    SecurityRule {
        id: "AUR-R004-CREDENTIAL-HARVESTING",
        title: "Credential & Secret Path Access",
        severity: RuleSeverity::High,
        description: "Script accesses SSH keys, AWS credentials, GitHub tokens, Docker configs, wallets, or browser secret stores.",
        pattern: r"(\.ssh/(id_rsa|id_ed25519|authorized_keys)|\.aws/credentials|\.config/gh/hosts|\.docker/config\.json|\.kube/config|\.mozilla/firefox/.*key4\.db|\.config/(google-chrome|chromium)/.*Login\s*Data|\.bitcoin|\.gnupg)",
    },
    // 5. Raw External IP Literals & Tunneling (aur-guard AG060, AG062)
    SecurityRule {
        id: "AUR-R005-RAW-IP-TARGET",
        title: "Hardcoded External IP URL or Reverse Tunnel",
        severity: RuleSeverity::High,
        description: "Network target uses a raw numerical IP address or reverse-tunnel service (ngrok, serveo, cloudflared) instead of a registered domain.",
        pattern: r"(https?://(?:[0-9]{1,3}\.){3}[0-9]{1,3}|ngrok|cloudflared|serveo\.net|localhost\.run)",
    },
    // 6. Privilege Escalation & SUID Modification (aur-guard AG040, AG041, AG042)
    SecurityRule {
        id: "AUR-R006-PRIVILEGE-ESCALATION",
        title: "Privilege Escalation or SUID Bit Modification",
        severity: RuleSeverity::High,
        description: "Script attempts sudo, su, setcap, or setting SUID/SGID executable bits.",
        pattern: r"(sudo\s+|su\s+-\s+|chmod\s+[ug]\+s|chmod\s+[0-7]?[42][0-7]{3}|setcap\s+)",
    },
    // 7. Raw Block Device Write & Destruction (aur-guard AG020, AG021, AG022, AG023)
    SecurityRule {
        id: "AUR-R007-RAW-DEVICE-WRITE",
        title: "System Destruction or Block Device Write",
        severity: RuleSeverity::Critical,
        description: "Destructive rm -rf /, writes to block devices (/dev/sd, /dev/nvme), filesystem formatting, or fork bomb.",
        pattern: r"(rm\s+-[a-zA-Z]*rf[a-zA-Z]*\s+(/|~|\$HOME)|dd\s+[^>\n]*of=/dev/(sd|nvme|vd|loop)|mkfs\.[a-z0-9]+\s+/dev/|:\(\)\s*\{\s*:\s*\|\s*:?\s*&\s*\}\s*;)",
    },
    // 8. Reverse Shell Backdoors (aur-guard AG010, AG011, AG012, AG013)
    SecurityRule {
        id: "AUR-R010-REVERSE-SHELL",
        title: "Interactive Reverse Shell Backdoor",
        severity: RuleSeverity::Critical,
        description: "Reverse shell construct via netcat (-e), bash /dev/tcp, python socket/pty, or perl.",
        pattern: r"(\bnc(\.traditional)?\s+[^\n|;&]*-e|bash\s+-i\s+>&?\s*/dev/(tcp|udp)/|python[0-9.]*\s+-c\s+.*socket.*dup2)",
    },
    // 9. Persistence & Startup Tampering (aur-guard AG030, AG031, AG032)
    SecurityRule {
        id: "AUR-R011-PERSISTENCE-BACKDOOR",
        title: "Persistence Mechanism / Startup Script Modification",
        severity: RuleSeverity::High,
        description: "Writes to user shell startup profiles (.bashrc, .zshrc), crontab, or system accounts.",
        pattern: r"(>>?\s*.*(\.bashrc|\.zshrc|\.profile|authorized_keys|/etc/cron))",
    },
    // 10. URL Shortener Source (aur-guard AG061)
    SecurityRule {
        id: "AUR-R012-URL-SHORTENER",
        title: "Obfuscated URL Shortener Source",
        severity: RuleSeverity::High,
        description: "Uses URL shorteners (bit.ly, tinyurl, t.co) which obscure the true origin of package sources.",
        pattern: r"https?://(bit\.ly|tinyurl\.com|t\.co|goo\.gl|is\.gd|rebrand\.ly)/",
    },
    // 11. Data Exfiltration via curl POST (aur-guard AG072)
    SecurityRule {
        id: "AUR-R013-DATA-EXFILTRATION",
        title: "Outbound Data Exfiltration via HTTP POST",
        severity: RuleSeverity::High,
        description: "Transmits local files or data using curl/wget POST or form upload during build.",
        pattern: r"(curl\s+.*(-X\s+POST|--data\b|-d\s+|-F\s+))",
    },
    // 12. Total Checksum Bypass
    SecurityRule {
        id: "AUR-R008-CHECKSUM-SKIP",
        title: "Checksums Set to SKIP",
        severity: RuleSeverity::Medium,
        description: "Integrity verification skipped for all source downloads.",
        pattern: r"((sha256sums|md5sums|sha512sums|b2sums)=\('SKIP'\))",
    },
    // 13. Plain Unencrypted HTTP Download
    SecurityRule {
        id: "AUR-R009-INSECURE-HTTP",
        title: "Unencrypted HTTP Source Download",
        severity: RuleSeverity::Low,
        description: "Sources are fetched over plaintext HTTP rather than HTTPS.",
        pattern: r"source=.*http://[^s]",
    },
];
