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

    /// Retrieve packages belonging to a specific category
    async fn get_by_category(&self, category: PackageCategory) -> Result<Vec<Package>, PastorError> {
        let all = self.search("").await?;
        Ok(all.into_iter().filter(|p| p.categories.contains(&category)).collect())
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

    /// Cancel an in-progress transaction for a package
    async fn cancel(&self, _id: &PackageId) -> Result<(), PastorError> {
        Ok(())
    }
}
