use gtk4::gdk::Display;
use gtk4::IconTheme;
use pastor_core::PackageCategory;

/// Resolve an icon name from a prioritized list of candidates that exists in the current theme.
pub fn resolve_icon(candidates: &[&str]) -> String {
    if let Some(display) = Display::default() {
        let theme = IconTheme::for_display(&display);
        for candidate in candidates {
            if theme.has_icon(candidate) {
                return (*candidate).to_string();
            }
        }
    }
    candidates
        .last()
        .copied()
        .unwrap_or("package-x-generic")
        .to_string()
}

pub fn update_icon() -> String {
    resolve_icon(&[
        "system-software-update-symbolic",
        "software-update-available-symbolic",
        "system-software-update",
        "view-refresh-symbolic",
    ])
}

pub fn explore_icon() -> String {
    resolve_icon(&[
        "starred-symbolic",
        "star-new-symbolic",
        "emblem-favorite-symbolic",
        "emblem-favorite",
    ])
}

pub fn installed_icon() -> String {
    resolve_icon(&[
        "package-x-generic",
        "checkbox-checked-symbolic",
        "system-software-install",
    ])
}

#[allow(dead_code)]
pub fn immutable_icon() -> String {
    resolve_icon(&[
        "computer-symbolic",
        "computer",
        "drive-harddisk-system-symbolic",
    ])
}

#[allow(dead_code)]
pub fn settings_icon() -> String {
    resolve_icon(&[
        "preferences-system-symbolic",
        "preferences-system",
        "emblem-system-symbolic",
    ])
}

#[allow(dead_code)]
pub fn snapshots_icon() -> String {
    resolve_icon(&[
        "camera-photo-symbolic",
        "camera-photo",
        "document-save-symbolic",
    ])
}

#[allow(dead_code)]
pub fn downgrade_icon() -> String {
    resolve_icon(&["edit-undo-symbolic", "edit-undo"])
}

#[allow(dead_code)]
pub fn repos_icon() -> String {
    resolve_icon(&["network-server-symbolic", "network-server"])
}

#[allow(dead_code)]
pub fn about_icon() -> String {
    resolve_icon(&["help-about-symbolic", "help-about"])
}

#[allow(dead_code)]
pub fn search_icon() -> String {
    resolve_icon(&["system-search-symbolic", "edit-find-symbolic"])
}

pub fn category_icon(cat: PackageCategory) -> String {
    match cat {
        PackageCategory::Featured => resolve_icon(&[
            "emblem-favorite-symbolic",
            "starred-symbolic",
            "emblem-favorite",
        ]),
        PackageCategory::ParchPicks => {
            resolve_icon(&["starred-symbolic", "star-new-symbolic"])
        }
        PackageCategory::Development => resolve_icon(&[
            "applications-development-symbolic",
            "applications-engineering-symbolic",
            "applications-development",
        ]),
        PackageCategory::Games => {
            resolve_icon(&["applications-games-symbolic", "applications-games"])
        }
        PackageCategory::Graphics => resolve_icon(&[
            "applications-graphics-symbolic",
            "applications-graphics",
        ]),
        PackageCategory::Internet => resolve_icon(&[
            "applications-internet-symbolic",
            "web-browser-symbolic",
            "applications-internet",
        ]),
        PackageCategory::Multimedia => resolve_icon(&[
            "applications-multimedia-symbolic",
            "applications-multimedia",
        ]),
        PackageCategory::Office => resolve_icon(&[
            "applications-office-symbolic",
            "x-office-document-symbolic",
            "applications-office",
        ]),
        PackageCategory::System => {
            resolve_icon(&["applications-system-symbolic", "applications-system"])
        }
        PackageCategory::Android => {
            resolve_icon(&["phone-symbolic", "phone", "smartphone-symbolic"])
        }
        PackageCategory::Utilities => resolve_icon(&[
            "applications-utilities-symbolic",
            "applications-utilities",
        ]),
    }
}

pub fn sidebar_toggle_icon() -> String {
    resolve_icon(&[
        "sidebar-show-symbolic",
        "view-left-pane-symbolic",
        "view-sidebar-symbolic",
        "dock-left-symbolic",
    ])
}
