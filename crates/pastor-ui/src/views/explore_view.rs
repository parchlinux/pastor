use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageCategory};
use pastor_store::Store;

use super::package_row::create_package_row;

pub fn create_explore_view(
    store: Store,
    on_select: impl Fn(Package) + 'static + Clone,
    on_category_select: impl Fn(PackageCategory) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .margin_bottom(32)
        .build();

    // ----------------------------------------------------
    // TOP SECTION: Spotlight Carousel / Curated Picks
    // ----------------------------------------------------
    let carousel_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .margin_start(16)
        .margin_end(16)
        .margin_top(8)
        .build();

    let carousel = adw::Carousel::builder()
        .spacing(12)
        .allow_mouse_drag(true)
        .allow_scroll_wheel(true)
        .build();

    let indicator = adw::CarouselIndicatorDots::builder()
        .carousel(&carousel)
        .halign(gtk4::Align::Center)
        .build();

    carousel_box.append(&carousel);
    carousel_box.append(&indicator);
    content_box.append(&carousel_box);

    // Initial placeholder hero card while loading
    let initial_hero = create_default_hero(store.clone(), on_select.clone());
    carousel.append(&initial_hero);
    indicator.set_visible(false);

    // Asynchronously fetch curated picks (Flathub App of the Day, Apps of the Week)
    let store_for_carousel = store.clone();
    let carousel_clone = carousel.clone();
    let indicator_clone = indicator.clone();
    let on_sel_carousel = on_select.clone();

    glib::spawn_future_local(async move {
        let picks = store_for_carousel.get_curated_picks().await.unwrap_or_default();
        if !picks.is_empty() {
            carousel_clone.remove(&initial_hero);
            for p in &picks {
                let card = create_banner_card(p, on_sel_carousel.clone());
                carousel_clone.append(&card);
            }
            indicator_clone.set_visible(picks.len() > 1);
        }
    });


    // ----------------------------------------------------
    // APPLICATION SECTIONS
    // ----------------------------------------------------

    // 1. Trending Applications
    content_box.append(&create_category_section(
        "Trending Applications",
        "Most popular community software and essentials",
        PackageCategory::Featured,
        8,
        false,
        store.clone(),
        on_select.clone(),
        on_category_select.clone(),
        on_transaction_start.clone(),
    ));

    // 2. Parch Picks
    content_box.append(&create_category_section(
        "Parch Picks",
        "Curated utilities and verified software for your desktop",
        PackageCategory::ParchPicks,
        6,
        false,
        store.clone(),
        on_select.clone(),
        on_category_select.clone(),
        on_transaction_start.clone(),
    ));

    // 3. Internet & Web
    content_box.append(&create_category_section(
        "Internet & Web",
        "Browsers, messengers, downloaders, and network tools",
        PackageCategory::Internet,
        6,
        true,
        store.clone(),
        on_select.clone(),
        on_category_select.clone(),
        on_transaction_start.clone(),
    ));

    // 4. Audio & Video
    content_box.append(&create_category_section(
        "Audio & Video",
        "Media players, video editors, and audio production tools",
        PackageCategory::Multimedia,
        6,
        true,
        store.clone(),
        on_select.clone(),
        on_category_select.clone(),
        on_transaction_start.clone(),
    ));

    // 5. Productivity & Office
    content_box.append(&create_category_section(
        "Productivity & Office",
        "Document editors, spreadsheets, and organizational apps",
        PackageCategory::Office,
        6,
        true,
        store.clone(),
        on_select.clone(),
        on_category_select.clone(),
        on_transaction_start.clone(),
    ));

    // 6. Development Tools
    content_box.append(&create_category_section(
        "Development Tools",
        "IDEs, compilers, debuggers, and developer utilities",
        PackageCategory::Development,
        6,
        true,
        store.clone(),
        on_select.clone(),
        on_category_select.clone(),
        on_transaction_start.clone(),
    ));

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}

fn create_category_section(
    title: &str,
    description: &str,
    category: PackageCategory,
    max_items: usize,
    show_see_all: bool,
    store: Store,
    on_select: impl Fn(Package) + 'static + Clone,
    on_category_select: impl Fn(PackageCategory) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let group = adw::PreferencesGroup::builder()
        .title(title)
        .description(description)
        .margin_start(16)
        .margin_end(16)
        .build();

    if show_see_all {
        let see_all_btn = gtk4::Button::builder()
            .label("See All")
            .css_classes(["flat"])
            .valign(gtk4::Align::Center)
            .build();
        let on_cat = on_category_select.clone();
        see_all_btn.connect_clicked(move |_| {
            on_cat(category);
        });
        group.set_header_suffix(Some(&see_all_btn));
    }

    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .margin_top(16)
        .margin_bottom(16)
        .halign(gtk4::Align::Center)
        .build();
    group.add(&spinner);

    let list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .visible(false)
        .build();

    let store_c = store.clone();
    let list_c = list.clone();
    let spinner_c = spinner.clone();
    let group_c = group.clone();

    glib::spawn_future_local(async move {
        if let Ok(mut pkgs) = store_c.get_by_category(category).await {
            if pkgs.is_empty() {
                group_c.set_visible(false);
            } else {
                pkgs.truncate(max_items);
                for p in pkgs {
                    let row = create_package_row(
                        &p,
                        &store_c,
                        on_select.clone(),
                        on_transaction_start.clone(),
                    );
                    list_c.append(&row);
                }
                list_c.set_visible(true);
            }
        } else {
            group_c.set_visible(false);
        }
        spinner_c.stop();
        spinner_c.set_visible(false);
    });

    group.add(&list);
    group.upcast()
}

fn create_pkg_icon(pkg: &Package, size: i32) -> gtk4::Image {
    match &pkg.icon {
        Some(pastor_core::PackageIcon::LocalPath(path)) => {
            let img = gtk4::Image::from_file(path);
            img.set_pixel_size(size);
            img.set_valign(gtk4::Align::Center);
            img
        }
        Some(pastor_core::PackageIcon::Themed(name)) => gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(size)
            .valign(gtk4::Align::Center)
            .build(),
        _ => gtk4::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(size)
            .valign(gtk4::Align::Center)
            .build(),
    }
}

fn create_banner_card(
    pkg: &Package,
    on_select: impl Fn(Package) + 'static + Clone,
) -> gtk4::Widget {
    let card = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .css_classes(["cascade-card"])
        .build();

    if let Some(cursor) = gtk4::gdk::Cursor::from_name("pointer", None) {
        card.set_cursor(Some(&cursor));
    }

    // Top Header: Eyebrow, Title, Pitch
    let header_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .build();

    let eyebrow_lbl = gtk4::Label::builder()
        .label("SPOTLIGHT PICK")
        .css_classes(["cascade-eyebrow"])
        .halign(gtk4::Align::Start)
        .build();
    header_box.append(&eyebrow_lbl);

    let title_lbl = gtk4::Label::builder()
        .label(pkg.display_title())
        .css_classes(["cascade-title"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    header_box.append(&title_lbl);

    let pitch_lbl = gtk4::Label::builder()
        .label(&pkg.summary)
        .css_classes(["cascade-pitch"])
        .halign(gtk4::Align::Start)
        .wrap(true)
        .lines(2)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    header_box.append(&pitch_lbl);
    card.append(&header_box);

    // Optional Screenshot Preview
    if let Some(first_shot) = pkg.screenshots.first() {
        let shot_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .height_request(140)
            .css_classes(["cascade-banner-art"])
            .halign(gtk4::Align::Fill)
            .overflow(gtk4::Overflow::Hidden)
            .build();

        let shot_widget = crate::views::package_details_view::create_screenshot_widget(first_shot);
        shot_box.append(&shot_widget);
        card.append(&shot_box);
    }

    // Bottom Footer Row
    let footer_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(14)
        .css_classes(["cascade-footer"])
        .valign(gtk4::Align::Center)
        .build();

    let icon_w = create_pkg_icon(pkg, 48);
    footer_box.append(&icon_w);

    let app_meta = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();

    let app_name = gtk4::Label::builder()
        .label(pkg.display_title())
        .css_classes(["cascade-app-title"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    app_meta.append(&app_name);

    let dev_name = pkg.maintainer.as_deref().unwrap_or("ParchLinux");
    let app_sub = gtk4::Label::builder()
        .label(dev_name)
        .css_classes(["cascade-app-subtitle"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    app_meta.append(&app_sub);
    footer_box.append(&app_meta);

    let action_btn = gtk4::Button::builder()
        .label(if pkg.is_installed() { "Open" } else { "Install" })
        .css_classes(["suggested-action", "cascade-get-btn"])
        .valign(gtk4::Align::Center)
        .build();

    let on_sel_btn = on_select.clone();
    let pkg_btn = pkg.clone();
    action_btn.connect_clicked(move |_| {
        on_sel_btn(pkg_btn.clone());
    });
    footer_box.append(&action_btn);
    card.append(&footer_box);

    let gesture = gtk4::GestureClick::new();
    let on_sel_card = on_select;
    let pkg_card = pkg.clone();
    gesture.connect_pressed(move |_, _, _, _| {
        on_sel_card(pkg_card.clone());
    });
    card.add_controller(gesture);

    card.upcast()
}

fn create_default_hero(
    store: Store,
    on_select: impl Fn(Package) + 'static + Clone,
) -> gtk4::Widget {
    let card = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .css_classes(["cascade-card"])
        .build();

    let header_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .build();

    let eyebrow_lbl = gtk4::Label::builder()
        .label("WELCOME TO PARCH")
        .css_classes(["cascade-eyebrow"])
        .halign(gtk4::Align::Start)
        .build();
    header_box.append(&eyebrow_lbl);

    let title_lbl = gtk4::Label::builder()
        .label("Discover Apps & Tools")
        .css_classes(["cascade-title"])
        .halign(gtk4::Align::Start)
        .build();
    header_box.append(&title_lbl);

    let pitch_lbl = gtk4::Label::builder()
        .label("Curated native and Flatpak software center optimized for speed and reliability.")
        .css_classes(["cascade-pitch"])
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    header_box.append(&pitch_lbl);
    card.append(&header_box);

    let footer_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(14)
        .css_classes(["cascade-footer"])
        .valign(gtk4::Align::Center)
        .build();

    let hero_icon = gtk4::Image::builder()
        .icon_name("system-software-install")
        .pixel_size(48)
        .valign(gtk4::Align::Center)
        .build();
    footer_box.append(&hero_icon);

    let app_meta = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();

    let app_name = gtk4::Label::builder()
        .label("ParchLinux Curated Picks")
        .css_classes(["cascade-app-title"])
        .halign(gtk4::Align::Start)
        .build();
    app_meta.append(&app_name);

    let app_sub = gtk4::Label::builder()
        .label("Explore recommended applications for ParchLinux")
        .css_classes(["cascade-app-subtitle"])
        .halign(gtk4::Align::Start)
        .build();
    app_meta.append(&app_sub);
    footer_box.append(&app_meta);

    let hero_btn = gtk4::Button::builder()
        .label("Explore")
        .css_classes(["suggested-action", "cascade-get-btn"])
        .valign(gtk4::Align::Center)
        .build();

    hero_btn.connect_clicked(move |_| {
        let s = store.clone();
        let sel = on_select.clone();
        glib::spawn_future_local(async move {
            if let Ok(pkgs) = s.get_by_category(PackageCategory::ParchPicks).await {
                if let Some(first) = pkgs.into_iter().next() {
                    sel(first);
                }
            }
        });
    });
    footer_box.append(&hero_btn);
    card.append(&footer_box);

    card.upcast()
}

