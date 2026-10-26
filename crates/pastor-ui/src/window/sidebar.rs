use adw::prelude::*;
use pastor_core::{tr, PackageCategory};

pub struct SidebarComponents {
    pub toolbar: adw::ToolbarView,
    pub search_entry: gtk4::SearchEntry,
    pub nav_list: gtk4::ListBox,
    pub cat_list: gtk4::ListBox,
    pub collapse_btn: gtk4::Button,
    pub active_categories: Vec<PackageCategory>,
}

pub fn create_nav_row(title: &str, icon_name: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::builder()
        .activatable(true)
        .build();

    let hbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(10)
        .margin_end(10)
        .build();

    let icon = gtk4::Image::builder()
        .icon_name(icon_name)
        .pixel_size(18)
        .build();
    hbox.append(&icon);

    let label = gtk4::Label::builder()
        .label(title)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    hbox.append(&label);

    row.set_child(Some(&hbox));
    row
}

pub fn build_sidebar() -> SidebarComponents {
    let sidebar_toolbar = adw::ToolbarView::new();
    let sidebar_header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .show_start_title_buttons(false)
        .build();
    let sidebar_title = adw::WindowTitle::builder()
        .title(tr("Parch Store"))
        .build();
    sidebar_header.set_title_widget(Some(&sidebar_title));

    let sidebar_collapse_btn = gtk4::Button::builder()
        .icon_name(crate::icons::sidebar_toggle_icon())
        .css_classes(["flat"])
        .tooltip_text(tr("Collapse Sidebar (F9)"))
        .valign(gtk4::Align::Center)
        .build();
    sidebar_header.pack_end(&sidebar_collapse_btn);

    sidebar_toolbar.add_top_bar(&sidebar_header);

    let sidebar_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Search Entry
    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text(tr("Search packages, apps, AUR… (Ctrl+F)"))
        .build();
    sidebar_box.append(&search_entry);

    // Discover Group
    let nav_group = adw::PreferencesGroup::builder()
        .title(tr("Discover"))
        .build();
    let nav_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .build();

    let explore_row = create_nav_row(tr("Explore"), &crate::icons::explore_icon());
    let installed_row = create_nav_row(tr("Installed"), &crate::icons::installed_icon());
    let updates_row = create_nav_row(tr("Updates"), &crate::icons::update_icon());

    nav_list.append(&explore_row);
    nav_list.append(&installed_row);
    nav_list.append(&updates_row);
    nav_group.add(&nav_list);
    sidebar_box.append(&nav_group);

    // Categories Group
    let cat_group = adw::PreferencesGroup::builder()
        .title(tr("Categories"))
        .build();
    let cat_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .build();

    let active_categories: Vec<PackageCategory> = PackageCategory::all()
        .iter()
        .copied()
        .filter(|c| *c != PackageCategory::Android && *c != PackageCategory::Featured && *c != PackageCategory::ParchPicks)
        .collect();

    for cat in &active_categories {
        let row = create_nav_row(cat.title(), &crate::icons::category_icon(*cat));
        cat_list.append(&row);
    }
    cat_group.add(&cat_list);
    sidebar_box.append(&cat_group);

    let sidebar_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&sidebar_box)
        .build();

    sidebar_toolbar.set_content(Some(&sidebar_scrolled));

    SidebarComponents {
        toolbar: sidebar_toolbar,
        search_entry,
        nav_list,
        cat_list,
        collapse_btn: sidebar_collapse_btn,
        active_categories,
    }
}
