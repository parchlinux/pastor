use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::{
    error::PastorError,
    package::{Package, PackageCategory, PackageId, PackageUpdate},
    transaction::TransactionEvent,
};

#[async_trait]
pub trait PackageBackend: Send + Sync {
    /// Identifier of this backend (e.g. "alpm", "aur", "flatpak", "bootc", "waydroid")
    fn name(&self) -> &'static str;

    /// Search for packages matching query string
    async fn search(&self, query: &str) -> Result<Vec<Package>, PastorError>;

    /// Retrieve packages belonging to a specific category. Backends supporting
    /// categories should override this method; default returns empty list.
    async fn get_by_category(&self, _category: PackageCategory) -> Result<Vec<Package>, PastorError> {
        Ok(vec![])
    }

    /// Retrieve curated/featured picks (e.g. Flathub App-Picks of the week/day)
    async fn curated_picks(&self) -> Result<Vec<Package>, PastorError> {
        Ok(vec![])
    }

    /// Retrieve detailed package information
    async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError>;

    /// List all currently installed packages managed by this backend
    async fn installed(&self) -> Result<Vec<Package>, PastorError>;

    /// Check for available updates
    async fn updates(&self) -> Result<Vec<PackageUpdate>, PastorError>;

    /// Install or upgrade a package, reporting real-time events via channel
    async fn install(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError>;

    /// Remove an installed package, reporting events via channel
    async fn remove(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError>;

    /// Discover available older versions for downgrading
    async fn downgrade_versions(&self, id: &PackageId) -> Result<Vec<String>, PastorError>;

    /// Downgrade a package to a specific target version
    async fn downgrade(
        &self,
        id: &PackageId,
        target_version: &str,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError>;

    /// Whether this backend can handle the given package (used to route install/remove to the right backend)
    fn handles(&self, _id: &PackageId) -> bool {
        true
    }

    /// Cancel an in-progress transaction for a package
    async fn cancel(&self, _id: &PackageId) -> Result<(), PastorError> {
        Ok(())
    }

    /// Update all pending packages managed by this backend in a single batch
    async fn update_all(
        &self,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let ups = self.updates().await?;
        let mut errs = Vec::new();
        for up in ups {
            if let Err(e) = self.install(&up.id, progress_tx.clone()).await {
                errs.push(format!("{}: {}", up.id.name, e));
            }
        }
        if !errs.is_empty() {
            Err(PastorError::TransactionFailed(errs.join(" | ")))
        } else {
            Ok(())
        }
    }

    /// Refresh remote package databases (e.g. pacman -Sy)
    async fn refresh_databases(
        &self,
        _progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        Ok(())
    }
}
