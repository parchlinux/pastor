use adw::prelude::*;
use pastor_core::{PackageId, PackageSource, ParchRepoType, TransactionEvent};
use pastor_store::Store;

pub fn create_downgrade_view(
    store: Store,
    initial_package: Option<String>,
    _on_back: impl Fn() + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .margin_top(16)
        .margin_bottom(28)
        .margin_start(24)
        .margin_end(24)
        .build();

    // View Title Header
    let title_label = gtk4::Label::builder()
        .label("Downgrade Package")
        .css_classes(["title-2"])
        .halign(gtk4::Align::Start)
        .build();
    content_box.append(&title_label);

    // Banner
    let info_banner = adw::Banner::builder()
        .title("Revert a package to a previously cached or archived version safely with automatic Snapper snapshot protection.")
        .revealed(true)
        .build();
    content_box.append(&info_banner);

    // Package Selection
    let group = adw::PreferencesGroup::builder()
        .title("Target Package")
        .description("Specify the installed package to downgrade")
        .build();

    let default_name = initial_package.as_deref().unwrap_or("").to_string();
    let entry_row = adw::EntryRow::builder()
        .title("Package name")
        .text(&default_name)
        .build();
    group.add(&entry_row);
    content_box.append(&group);

    // Versions Selection
    let versions_group = adw::PreferencesGroup::builder()
        .title("Available Historical Versions")
        .description("Found in local pacman cache (/var/cache/pacman/pkg)")
        .build();

    let list_box = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .css_classes(["boxed-list"])
        .build();

    versions_group.add(&list_box);
    content_box.append(&versions_group);

    // Action button
    let downgrade_btn = gtk4::Button::builder()
        .label("Start Safe Downgrade")
        .css_classes(["destructive-action", "pill"])
        .halign(gtk4::Align::Center)
        .sensitive(false)
        .margin_top(8)
        .build();

    content_box.append(&downgrade_btn);

    let selected_ver: std::rc::Rc<std::cell::RefCell<Option<String>>> = std::rc::Rc::new(std::cell::RefCell::new(None));

    let scan_cache = {
        let list_box = list_box.clone();
        let downgrade_btn = downgrade_btn.clone();
        let selected_ver = selected_ver.clone();

        std::rc::Rc::new(move |pkg_name: &str| {
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }
            *selected_ver.borrow_mut() = None;
            downgrade_btn.set_sensitive(false);

            let name = pkg_name.trim().to_string();
            if name.is_empty() {
                let empty_row = adw::ActionRow::builder()
                    .title("Enter a package name above to search local cache")
                    .sensitive(false)
                    .build();
                list_box.append(&empty_row);
                return;
            }

            let cache_dir = std::path::Path::new("/var/cache/pacman/pkg");
            let mut found_versions = Vec::new();

            if let Ok(entries) = std::fs::read_dir(cache_dir) {
                let prefix = format!("{}-", name);
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with(&prefix)
                        && (fname.ends_with(".pkg.tar.zst") || fname.ends_with(".pkg.tar.xz"))
                        && !fname.ends_with(".sig")
                    {
                        // Strip suffix
                        let without_ext = fname
                            .trim_end_matches(".pkg.tar.zst")
                            .trim_end_matches(".pkg.tar.xz");
                        // Strip prefix
                        let ver_arch = &without_ext[prefix.len()..];
                        let ver = ver_arch.rsplit_once('-').map(|(v, _arch)| v).unwrap_or(ver_arch);

                        let size_mb = entry.metadata().map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
                        found_versions.push((ver.to_string(), fname, size_mb));
                    }
                }
            }

            if found_versions.is_empty() {
                let empty_row = adw::ActionRow::builder()
                    .title("No cached versions found in /var/cache/pacman/pkg")
                    .subtitle("Only locally cached or archived packages can be downgraded")
                    .sensitive(false)
                    .build();
                list_box.append(&empty_row);
            } else {
                found_versions.sort_by(|a, b| b.0.cmp(&a.0));
                for (ver, fname, size) in found_versions {
                    let row = adw::ActionRow::builder()
                        .title(format!("Version {}", ver))
                        .subtitle(format!("{fname} • {:.1} MB", size))
                        .activatable(true)
                        .build();

                    let check_icon = gtk4::Image::builder()
                        .icon_name("emblem-ok-symbolic")
                        .visible(false)
                        .build();
                    row.add_suffix(&check_icon);

                    let v_clone = ver.clone();
                    let sel_ref = selected_ver.clone();
                    let d_btn = downgrade_btn.clone();
                    let lb_clone = list_box.clone();

                    row.connect_activated(move |current_row| {
                        *sel_ref.borrow_mut() = Some(v_clone.clone());
                        d_btn.set_sensitive(true);

                        // Highlight active row
                        let mut child = lb_clone.first_child();
                        while let Some(c) = child {
                            if let Ok(act) = c.clone().downcast::<adw::ActionRow>() {
                                let is_active = act.eq(current_row);
                                act.set_css_classes(if is_active { &["selected-row"] } else { &[] });
                            }
                            child = c.next_sibling();
                        }
                    });

                    list_box.append(&row);
                }
            }
        })
    };

    let scan_clone = scan_cache.clone();
    entry_row.connect_changed(move |entry| {
        scan_clone(&entry.text());
    });

    scan_cache(&default_name);

    let store_clone = store.clone();
    let on_tx = on_transaction_start.clone();
    let entry_clone = entry_row.clone();
    let sel_ver_for_btn = selected_ver.clone();
    downgrade_btn.connect_clicked(move |btn| {
        if let Some(ref target_v) = *sel_ver_for_btn.borrow() {
            let pkg_name = entry_clone.text().to_string();
            let id = PackageId::new(pkg_name, PackageSource::Parch(ParchRepoType::World));
            let rx = store_clone.downgrade(id, target_v.clone());
            on_tx(rx);
            btn.set_sensitive(false);
            btn.set_label("Downgrading...");
        }
    });

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}
