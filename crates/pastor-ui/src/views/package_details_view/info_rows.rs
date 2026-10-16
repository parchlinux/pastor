use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageSource};
use pastor_store::Store;

pub fn create_spec_row(title: &str, subtitle: &str, icon_name: &str) -> adw::ActionRow {
    let esc_title = glib::markup_escape_text(title);
    let esc_sub = glib::markup_escape_text(subtitle);
    let row = adw::ActionRow::builder()
        .title(esc_title.as_str())
        .subtitle(esc_sub.as_str())
        .build();
    let img = gtk4::Image::from_icon_name(icon_name);
    row.add_prefix(&img);
    row
}

pub fn create_about_group(pkg: &Package) -> adw::PreferencesGroup {
    let about_group = adw::PreferencesGroup::builder()
        .title("About")
        .build();

    let summary_lead = gtk4::Label::builder()
        .label(&pkg.summary)
        .css_classes(["title-4"])
        .wrap(true)
        .halign(gtk4::Align::Start)
        .margin_bottom(6)
        .build();
    about_group.add(&summary_lead);

    let desc_text = pkg.description.as_deref().unwrap_or(&pkg.summary);
    let desc_label = gtk4::Label::builder()
        .label(desc_text)
        .wrap(true)
        .halign(gtk4::Align::Start)
        .css_classes(["body"])
        .margin_bottom(4)
        .build();
    about_group.add(&desc_label);

    about_group
}

pub fn create_whats_new_group(pkg: &Package) -> adw::PreferencesGroup {
    let wn_title = glib::markup_escape_text(&format!("What's New in Version {}", pkg.version));
    let whats_new_group = adw::PreferencesGroup::builder()
        .title(wn_title.as_str())
        .build();

    let changelog_text = pkg.changelog.as_deref().unwrap_or(
        "Optimized for ParchLinux with updated system dependencies, Wayland integration, and performance enhancements."
    );

    let exp_title = glib::markup_escape_text(&format!("Version {} Release Notes", pkg.version));
    let expander = adw::ExpanderRow::builder()
        .title(exp_title.as_str())
        .subtitle("Click to view changelog and update details")
        .expanded(false)
        .build();
    let rel_icon = gtk4::Image::from_icon_name("software-update-available-symbolic");
    expander.add_prefix(&rel_icon);

    let cl_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .margin_start(16)
        .margin_end(16)
        .margin_top(12)
        .margin_bottom(12)
        .spacing(6)
        .build();

    let cl_label = gtk4::Label::builder()
        .label(changelog_text)
        .wrap(true)
        .halign(gtk4::Align::Start)
        .css_classes(["body"])
        .build();
    cl_box.append(&cl_label);
    expander.add_row(&cl_box);
    whats_new_group.add(&expander);

    whats_new_group
}

pub fn create_specs_group(pkg: &Package) -> adw::PreferencesGroup {
    let specs_group = adw::PreferencesGroup::builder()
        .title("Details")
        .build();

    let lic_str = pkg.license.as_deref().unwrap_or("Open Source");
    let dl_str = match pkg.size_download {
        Some(sz) => pastor_core::format_size(sz),
        None => "Varies".to_string(),
    };
    let inst_str = match pkg.size_installed {
        Some(sz) => pastor_core::format_size(sz),
        None => "Varies".to_string(),
    };

    let dev_row = create_spec_row(
        "Developer",
        pkg.maintainer.as_deref().unwrap_or("ParchLinux Contributors"),
        "avatar-default-symbolic",
    );
    specs_group.add(&dev_row);

    let pkg_row = create_spec_row(
        "Package Name",
        &pkg.name,
        "application-x-executable-symbolic",
    );
    specs_group.add(&pkg_row);

    let ver_row = create_spec_row(
        "Version",
        &pkg.version,
        "tag-symbolic",
    );
    specs_group.add(&ver_row);

    let lic_row = create_spec_row(
        "License",
        lic_str,
        "help-about-symbolic",
    );
    specs_group.add(&lic_row);

    let source_title = match &pkg.id.source {
        PackageSource::Parch(repo) => format!("Parch Linux [{}]", repo),
        PackageSource::Arch(repo) => format!("Arch Linux Upstream [{}]", repo),
        PackageSource::Aur => "Arch User Repository (AUR)".to_string(),
        PackageSource::Flatpak { remote } => format!("Flatpak ({})", remote),
        PackageSource::Bootc => "bootc Transactional Base OS".to_string(),
        PackageSource::Waydroid => "Waydroid Android Environment".to_string(),
    };
    let source_row = create_spec_row(
        "Repository and Source",
        &source_title,
        "package-x-generic-symbolic",
    );
    specs_group.add(&source_row);

    let arch_row = create_spec_row(
        "Architecture",
        "x86_64",
        "system-run-symbolic",
    );
    specs_group.add(&arch_row);

    let dl_row = create_spec_row(
        "Download Size",
        &dl_str,
        "folder-download-symbolic",
    );
    specs_group.add(&dl_row);

    let inst_row = create_spec_row(
        "Installed Footprint",
        &inst_str,
        "drive-harddisk-symbolic",
    );
    specs_group.add(&inst_row);

    let format_info = match &pkg.id.source {
        PackageSource::Flatpak { .. } => "Flatpak OCI Bundle (Sandboxed runtime)",
        PackageSource::Bootc => "bootc OCI Container Image Layer",
        PackageSource::Waydroid => "Android Package Container",
        _ => "Native ALPM Package (.pkg.tar.zst)",
    };
    let format_row = create_spec_row(
        "Package Format",
        format_info,
        "package-x-generic-symbolic",
    );
    specs_group.add(&format_row);

    let safety_row = create_spec_row(
        "System Safety",
        "Automatic Snapper Btrfs pre/post snapshots for instant rollback",
        "camera-photo-symbolic",
    );
    specs_group.add(&safety_row);

    if let Some(ref url) = pkg.homepage {
        let home_row = create_spec_row(
            "Project Website",
            url,
            "applications-internet-symbolic",
        );

        let link_btn = gtk4::LinkButton::builder()
            .uri(url)
            .icon_name("insert-link-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Open in browser")
            .valign(gtk4::Align::Center)
            .build();
        home_row.add_suffix(&link_btn);
        specs_group.add(&home_row);
    }

    specs_group
}

pub fn create_dependencies_group(
    pkg: &Package,
    store: &Store,
    on_select_package: impl Fn(Package) + 'static + Clone,
) -> Option<adw::PreferencesGroup> {
    if pkg.dependencies.is_empty() {
        return None;
    }

    let dep_group = adw::PreferencesGroup::builder()
        .title("Dependencies")
        .build();

    let expander = adw::ExpanderRow::builder()
        .title(format!("Dependencies ({})", pkg.dependencies.len()))
        .subtitle("Packages and runtimes required for functionality")
        .expanded(false)
        .build();
    let dep_icon = gtk4::Image::from_icon_name("system-run-symbolic");
    expander.add_prefix(&dep_icon);

    let dep_flow = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .max_children_per_line(4)
        .min_children_per_line(1)
        .row_spacing(8)
        .column_spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    for dep in &pkg.dependencies {
        let dep_clean = dep.split(|c| c == '<' || c == '>' || c == '=').next().unwrap_or(dep).trim().to_string();
        let chip = gtk4::Button::builder()
            .label(&dep_clean)
            .css_classes(["dependency-tag", "flat"])
            .tooltip_text(&format!("View package details for {dep_clean}"))
            .valign(gtk4::Align::Center)
            .build();

        let s = store.clone();
        let name = dep_clean.clone();
        let on_sel = on_select_package.clone();
        chip.connect_clicked(move |_| {
            let s_c = s.clone();
            let name_c = name.clone();
            let on_sel_c = on_sel.clone();
            glib::spawn_future_local(async move {
                if let Ok(results) = s_c.search_raw(&name_c).await {
                    if let Some(target) = results.into_iter().find(|p| p.name == name_c || p.id.name == name_c) {
                        on_sel_c(target);
                    }
                }
            });
        });
        dep_flow.insert(&chip, -1);
    }

    expander.add_row(&dep_flow);
    dep_group.add(&expander);
    Some(dep_group)
}
