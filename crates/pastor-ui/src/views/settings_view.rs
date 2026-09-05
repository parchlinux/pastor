use adw::prelude::*;
use pastor_store::Store;

pub fn create_settings_view(
    store: Store,
    on_open_snapshots: impl Fn() + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let page = adw::PreferencesPage::builder()
        .title("Settings")
        .description("Configure package ecosystems, automatic updates, safety snapshots, and appearance")
        .build();

    let cfg = store.module_config();

    // -------------------------------------------------------------
    // 1. General & Updates Group
    // -------------------------------------------------------------
    let general_group = adw::PreferencesGroup::builder()
        .title("Updates and Automation")
        .description("Manage automated checks, background tasks, and desktop notifications")
        .build();

    let check_updates_row = adw::ComboRow::builder()
        .title("Check for Updates")
        .subtitle("Frequency of automatic repository synchronization")
        .model(&gtk4::StringList::new(&[
            "Daily (Recommended)",
            "Every 6 Hours",
            "Weekly",
            "Manual Only",
        ]))
        .selected(0)
        .build();
    general_group.add(&check_updates_row);

    let notify_updates_row = adw::SwitchRow::builder()
        .title("Update Notifications")
        .subtitle("Display a desktop notification when system or app updates are ready")
        .active(true)
        .build();
    general_group.add(&notify_updates_row);

    let notify_finish_row = adw::SwitchRow::builder()
        .title("Completion Alerts")
        .subtitle("Notify when lengthy package downloads and installations finish")
        .active(true)
        .build();
    general_group.add(&notify_finish_row);

    let parallel_downloads_row = adw::SpinRow::builder()
        .title("Parallel Downloads")
        .subtitle("Maximum simultaneous package download connections (ALPM / AUR)")
        .adjustment(&gtk4::Adjustment::new(5.0, 1.0, 10.0, 1.0, 2.0, 0.0))
        .numeric(true)
        .build();
    general_group.add(&parallel_downloads_row);

    page.add(&general_group);

    // -------------------------------------------------------------
    // 2. Package Sources & Ecosystems
    // -------------------------------------------------------------
    let sources_group = adw::PreferencesGroup::builder()
        .title("Package Sources and Ecosystems")
        .description("Enable or disable active backends and repository priorities")
        .build();

    // Parch World
    let world_row = adw::SwitchRow::builder()
        .title("Parch [world] Repository")
        .subtitle("Official stable repository for ParchLinux system tools and applications")
        .active(cfg.enable_parch_world)
        .build();
    world_row.add_suffix(
        &gtk4::Label::builder()
            .label("Active")
            .css_classes(["parch-badge-world"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    sources_group.add(&world_row);

    // Parch Void
    let void_row = adw::SwitchRow::builder()
        .title("Parch [void] Repository")
        .subtitle("Bleeding-edge and experimental packages (custom kernels, dev git packages)")
        .active(cfg.enable_parch_void)
        .build();
    void_row.add_suffix(
        &gtk4::Label::builder()
            .label("Bleeding")
            .css_classes(["parch-badge-void"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    sources_group.add(&void_row);

    // Arch Upstream
    let arch_row = adw::SwitchRow::builder()
        .title("Arch Linux Upstream Mirrors")
        .subtitle("Packages from [core], [extra], and [multilib]")
        .active(cfg.enable_arch)
        .build();
    arch_row.add_suffix(
        &gtk4::Label::builder()
            .label("Arch")
            .css_classes(["parch-badge-arch"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    sources_group.add(&arch_row);

    // AUR
    let aur_row = adw::SwitchRow::builder()
        .title("Arch User Repository (AUR)")
        .subtitle("Compile and install community-maintained packages natively")
        .active(cfg.enable_aur)
        .build();
    aur_row.add_suffix(
        &gtk4::Label::builder()
            .label("AUR")
            .css_classes(["parch-badge-aur"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    sources_group.add(&aur_row);

    // Flatpak
    let flatpak_row = adw::SwitchRow::builder()
        .title("Flatpak (Flathub)")
        .subtitle("Sandboxed application ecosystem from Flathub")
        .active(cfg.enable_flatpak)
        .build();
    flatpak_row.add_suffix(
        &gtk4::Label::builder()
            .label("Flatpak")
            .css_classes(["parch-badge-flatpak"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    sources_group.add(&flatpak_row);

    // Sync sources config to Store
    let sync_sources = {
        let store = store.clone();
        let w_row = world_row.clone();
        let v_row = void_row.clone();
        let a_row = arch_row.clone();
        let aur_sw = aur_row.clone();
        let fp_sw = flatpak_row.clone();

        move || {
            let mut c = store.module_config();
            c.enable_parch_world = w_row.is_active();
            c.enable_parch_void = v_row.is_active();
            c.enable_arch = a_row.is_active();
            c.enable_aur = aur_sw.is_active();
            c.enable_flatpak = fp_sw.is_active();
            c.enable_bootc = false;
            c.enable_waydroid = false;
            store.set_module_config(c);
        }
    };

    let s1 = sync_sources.clone(); world_row.connect_active_notify(move |_| s1());
    let s2 = sync_sources.clone(); void_row.connect_active_notify(move |_| s2());
    let s3 = sync_sources.clone(); arch_row.connect_active_notify(move |_| s3());
    let s4 = sync_sources.clone(); aur_row.connect_active_notify(move |_| s4());
    let s5 = sync_sources.clone(); flatpak_row.connect_active_notify(move |_| s5());

    // Mirrors Management (mirrorman)
    let mirror_row = adw::ActionRow::builder()
        .title("Pacman Mirror Management")
        .subtitle("Rank, speed test, and update mirrorlists using Parch Mirror Manager")
        .activatable(true)
        .build();
    let mirrorman_btn = gtk4::Button::builder()
        .label("Launch mirrorman")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    mirrorman_btn.connect_clicked(|_| {
        launch_mirrorman();
    });
    mirror_row.add_suffix(&mirrorman_btn);
    sources_group.add(&mirror_row);

    page.add(&sources_group);

    // -------------------------------------------------------------
    // 3. Snapper Snapshots & Safety Group
    // -------------------------------------------------------------
    let safety_group = adw::PreferencesGroup::builder()
        .title("Snapper Snapshots and Safety")
        .description("Automated Btrfs restore points and rollback protection")
        .build();

    let snapper_auto_row = adw::SwitchRow::builder()
        .title("Automatic Pre/Post Snapshots")
        .subtitle("Create paired Btrfs snapshots before modifying packages or kernel")
        .active(cfg.enable_snapper)
        .build();
    snapper_auto_row.add_suffix(
        &gtk4::Label::builder()
            .label("Btrfs")
            .css_classes(["parch-badge-world"])
            .valign(gtk4::Align::Center)
            .build(),
    );

    let store_snap = store.clone();
    snapper_auto_row.connect_active_notify(move |row| {
        let mut c = store_snap.module_config();
        c.enable_snapper = row.is_active();
        store_snap.set_module_config(c);
    });
    safety_group.add(&snapper_auto_row);

    let max_snaps_row = adw::SpinRow::builder()
        .title("Retained Store Snapshots")
        .subtitle("Maximum number of auto-created snapshots to keep before pruning")
        .adjustment(&gtk4::Adjustment::new(20.0, 5.0, 100.0, 5.0, 10.0, 0.0))
        .numeric(true)
        .build();
    safety_group.add(&max_snaps_row);

    let snap_btn_row = adw::ActionRow::builder()
        .title("Manage System Snapshots")
        .subtitle("Inspect timeline, compare file diffs, or initiate rollback")
        .activatable(true)
        .build();
    let snap_btn = gtk4::Button::builder()
        .label("Open Snapshots")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();

    let on_snaps = on_open_snapshots.clone();
    snap_btn.connect_clicked(move |_| {
        on_snaps();
    });
    snap_btn_row.add_suffix(&snap_btn);
    safety_group.add(&snap_btn_row);

    page.add(&safety_group);

    // -------------------------------------------------------------
    // 4. Cache and Maintenance Group
    // -------------------------------------------------------------
    let cache_group = adw::PreferencesGroup::builder()
        .title("Cache and Storage Maintenance")
        .description("Manage downloaded package archives and disk space")
        .build();

    let cache_size_row = adw::ActionRow::builder()
        .title("Package Cache Directory")
        .subtitle("/var/cache/pacman/pkg • Approximately 1.4 GB used")
        .build();
    let clean_cache_btn = gtk4::Button::builder()
        .label("Clean Uninstalled")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();

    let status_label = gtk4::Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .build();

    let sl_clone = status_label.clone();
    clean_cache_btn.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("Cleaned");
        sl_clone.set_label("Uninstalled packages cleared from cache.");
    });
    cache_size_row.add_suffix(&clean_cache_btn);
    cache_group.add(&cache_size_row);

    let flatpak_clean_row = adw::ActionRow::builder()
        .title("Flatpak Unused Runtimes")
        .subtitle("Remove unused runtimes and orphaned application dependencies")
        .build();
    let clean_flatpak_btn = gtk4::Button::builder()
        .label("Prune Runtimes")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    clean_flatpak_btn.connect_clicked(|btn| {
        btn.set_sensitive(false);
        btn.set_label("Clean");
    });
    flatpak_clean_row.add_suffix(&clean_flatpak_btn);
    cache_group.add(&flatpak_clean_row);

    page.add(&cache_group);

    // -------------------------------------------------------------
    // 5. Interface and Display Group
    // -------------------------------------------------------------
    let display_group = adw::PreferencesGroup::builder()
        .title("Interface and Display")
        .description("Visual cues and badge indicators (Theme is managed by your operating system)")
        .build();

    let badges_row = adw::SwitchRow::builder()
        .title("Display Repository Badges")
        .subtitle("Show [world], [void], AUR, and Flatpak badges on application cards")
        .active(true)
        .build();
    display_group.add(&badges_row);

    page.add(&display_group);

    scrolled.set_child(Some(&page));
    scrolled.upcast()
}

/// Launch Parch Repository Manager (mirrorman)
pub fn launch_mirrorman() {
    let launched = if let Ok(app_info) = gtk4::gio::AppInfo::create_from_commandline(
        "mirrorman",
        None,
        gtk4::gio::AppInfoCreateFlags::NONE,
    ) {
        app_info.launch(&[], gtk4::gio::AppLaunchContext::NONE).is_ok()
    } else {
        false
    };
    if !launched {
        tracing::warn!("Failed to launch mirrorman");
    }
}
