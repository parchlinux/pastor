pub mod backend;
pub mod cache;
pub mod deb;
pub mod doctor;
pub mod error;
pub mod locale;
pub mod package;
pub mod snapshot;
pub mod transaction;

pub use backend::PackageBackend;
pub use cache::{cleanup_stale_caches, pastor_cache_dir};
pub use deb::{
    convert_deb_to_pkg, host_arch, inspect_deb, map_debian_arch, map_debian_dep_candidates,
    map_debian_depends_offline, sanitize_arch_pkgname, sanitize_arch_pkgver,
    search_candidates_for_deb, DebMetadata,
};
pub use doctor::{
    check_database_lock, generate_diff, parse_pacman_pkg_filename, scan_pacman_cache,
    scan_pacnew_files, CacheHealthInfo, DatabaseLockInfo, DoctorReport, OrphanPackage, PacnewFile,
    PacnewKind,
};
pub use error::PastorError;
pub use locale::{
    active_language, clean_locale, is_persian, is_rtl, is_rtl_language, persian_po_catalog,
    set_language, set_persian_mode, tr, tr_static, LocaleMatcher, LocalizedDescription,
    LocalizedField, PoCatalog,
};
pub use package::{
    format_size, ModuleConfig, Package, PackageCategory, PackageIcon, PackageId, PackageSource,
    PackageState, PackageUpdate, ParchRepoType,
};
pub use snapshot::{NullSnapshotBackend, SnapshotBackend, SnapshotInfo};
pub use transaction::{TransactionEvent, TransactionStep};
