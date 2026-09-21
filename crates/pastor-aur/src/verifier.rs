use std::{
    path::Path,
    process::{Command, Stdio},
};
use pastor_core::error::PastorError;
use crate::scanner::{HeuristicScanner, ScanReport};

pub struct ArtifactVerifier;

impl ArtifactVerifier {
    /// Inspect the compiled .pkg.tar.* archive before handing over to ALPM/pacman root execution
    pub fn verify_package_install_hook(pkg_path: &Path) -> Result<Option<ScanReport>, PastorError> {
        tracing::info!(
            "Stage 7: Verifying compiled package artifact hooks for {}",
            pkg_path.display()
        );

        // Extract .INSTALL file from archive if present
        let mut cmd = Command::new("tar");
        cmd.arg("-I")
            .arg("zstd")
            .arg("-xOf")
            .arg(pkg_path)
            .arg(".INSTALL");

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let output = match cmd.output() {
            Ok(out) => out,
            Err(e) => {
                tracing::warn!("Failed to invoke tar for .INSTALL extraction: {e}");
                return Ok(None);
            }
        };

        if !output.status.success() || output.stdout.is_empty() {
            tracing::info!("Package artifact contains no .INSTALL script; verified clean.");
            return Ok(None);
        }

        let hook_content = String::from_utf8_lossy(&output.stdout).to_string();
        let findings = HeuristicScanner::scan_content(".INSTALL (compiled package)", &hook_content);

        let report = ScanReport { findings };
        if report.has_critical() {
            let msg = format!(
                "CRITICAL SECURITY ALERT: Tampered or malicious .INSTALL hook detected inside compiled package artifact '{}'! Installation aborted.",
                pkg_path.file_name().and_then(|n| n.to_str()).unwrap_or("package")
            );
            tracing::error!("{}", msg);
            return Err(PastorError::TransactionFailed(msg));
        }

        Ok(Some(report))
    }
}
