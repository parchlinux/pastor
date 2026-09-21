pub mod appstream;
pub mod backend;
pub mod worker;

pub use appstream::AlpmCatalog;
pub use backend::AlpmBackend;
pub use worker::run_worker;

pub fn vercmp(v1: &str, v2: &str) -> std::cmp::Ordering {
    alpm::vercmp(v1, v2)
}
