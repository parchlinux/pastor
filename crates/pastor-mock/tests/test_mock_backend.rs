use pastor_core::{backend::PackageBackend, snapshot::SnapshotBackend};
use pastor_mock::MockPackageBackend;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_mock_search_and_snapper() {
    let backend = MockPackageBackend::new();

    // 1. Test search
    let pkgs = backend.search("parch").await.unwrap();
    assert!(!pkgs.is_empty());
    assert!(pkgs.iter().any(|p| p.name == "parch-welcome"));

    // 2. Test Snapper snapshots
    let snapper = backend.snapshot_backend();
    let initial_snaps = snapper.list_snapshots().await.unwrap();
    assert!(!initial_snaps.is_empty());

    let pre_id = snapper.create_pre_snapshot("Test Snapshot").await.unwrap();
    assert!(pre_id > 0);

    let post_id = snapper.create_post_snapshot(pre_id, "Test Snapshot").await.unwrap();
    assert!(post_id > pre_id);

    // 3. Test transaction with simulated progress
    let target = pkgs.iter().find(|p| p.name == "parch-tweaks").unwrap();
    let (tx, mut rx) = mpsc::channel(50);

    let id = target.id.clone();
    tokio::spawn(async move {
        backend.install(&id, tx).await.unwrap();
    });

    let mut event_count = 0;
    while let Some(evt) = rx.recv().await {
        event_count += 1;
        if evt.progress_fraction >= 1.0 {
            break;
        }
    }
    assert!(event_count > 0);
}
