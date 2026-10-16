pub mod actions;
pub mod header;
pub mod info_rows;
pub mod screenshots;

use adw::prelude::*;
use pastor_core::Package;
use pastor_store::Store;

pub use screenshots::create_screenshot_widget;

pub fn create_package_details_view(
    pkg: Package,
    store: Store,
    on_select_package: impl Fn(Package) + 'static + Clone,
    on_downgrade: impl Fn(String) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let clamp = adw::Clamp::builder()
        .maximum_size(860)
        .tightening_threshold(620)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(28)
        .margin_top(20)
        .margin_bottom(48)
        .margin_start(24)
        .margin_end(24)
        .build();

    // 1. GNOME HIG Hero Section
    let (hero_box, actions_box) = header::create_hero_box(&pkg, &store, on_select_package.clone());

    // 2. Actions & Transaction Monitoring
    actions::setup_actions(
        &actions_box,
        &pkg,
        &store,
        on_downgrade,
        on_transaction_start,
    );
    hero_box.append(&actions_box);
    content_box.append(&hero_box);

    // 3. Screenshots Previews (adw::Carousel)
    let carousel_group = adw::PreferencesGroup::builder()
        .title("Screenshots")
        .build();

    let carousel = adw::Carousel::builder()
        .spacing(18)
        .allow_scroll_wheel(true)
        .build();

    if !pkg.screenshots.is_empty() {
        for url in &pkg.screenshots {
            let slide_card = create_screenshot_widget(url);
            carousel.append(&slide_card);
        }
    } else {
        let preview_slides = [
            ("Workspace", "Primary interface and active workspace"),
            ("Tools & Workflows", "Comprehensive tools and options"),
            ("Preferences", "User customization and display settings"),
        ];

        for (index, (slide_title, slide_desc)) in preview_slides.into_iter().enumerate() {
            let slide_card = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(["screenshot-preview-card"])
                .build();

            let preview_header = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .css_classes(["screenshot-preview-header"])
                .build();

            let header_title = gtk4::Label::builder()
                .label(format!("{} - {}", pkg.display_title(), slide_title))
                .css_classes(["caption", "dim-label"])
                .hexpand(true)
                .halign(gtk4::Align::Center)
                .build();
            preview_header.append(&header_title);
            slide_card.append(&preview_header);

            let preview_body = header::create_preview_canvas(&pkg, index);
            slide_card.append(&preview_body);

            let footer_caption = gtk4::Label::builder()
                .label(slide_desc)
                .css_classes(["caption", "dim-label"])
                .margin_bottom(12)
                .halign(gtk4::Align::Center)
                .build();
            slide_card.append(&footer_caption);

            carousel.append(&slide_card);
        }
    }

    let dots = adw::CarouselIndicatorDots::builder()
        .carousel(&carousel)
        .halign(gtk4::Align::Center)
        .margin_top(8)
        .build();

    let carousel_container = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .build();
    carousel_container.append(&carousel);
    carousel_container.append(&dots);
    carousel_group.add(&carousel_container);
    content_box.append(&carousel_group);

    // 4. About Section
    let about_group = info_rows::create_about_group(&pkg);
    content_box.append(&about_group);

    // 5. What's New Section
    let whats_new_group = info_rows::create_whats_new_group(&pkg);
    content_box.append(&whats_new_group);

    // 6. Technical Specifications
    let specs_group = info_rows::create_specs_group(&pkg);
    content_box.append(&specs_group);

    // 7. Dependencies Section
    if let Some(dep_group) = info_rows::create_dependencies_group(&pkg, &store, on_select_package) {
        content_box.append(&dep_group);
    }

    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));
    scrolled.upcast()
}
