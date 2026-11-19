pub mod actions;
pub mod header;
pub mod info_rows;
pub mod screenshots;
pub mod share;

use adw::prelude::*;
use pastor_core::Package;
use pastor_store::Store;

use crate::models::TxMonitor;

pub use screenshots::create_screenshot_widget;

pub fn create_package_details_view(
    pkg: Package,
    store: Store,
    monitor: TxMonitor,
    on_select_package: impl Fn(Package) + 'static + Clone,
    on_downgrade: impl Fn(String) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let clamp = adw::Clamp::builder()
        .maximum_size(860)
        .tightening_threshold(620)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(24)
        .margin_top(16)
        .margin_bottom(28)
        .margin_start(16)
        .margin_end(16)
        .build();

    // 1. GNOME HIG Hero Section
    let (hero_widgets, actions_box) = header::create_hero_box(&pkg, &store, on_select_package.clone());

    // 2. Actions & Transaction Monitoring (inside the hero, as before)
    actions::setup_actions(
        &actions_box,
        &pkg,
        &store,
        &monitor,
        on_downgrade,
        on_transaction_start,
    );
    content_box.append(&hero_widgets.hero_box);

    // Context Quick Stats Bar (Security, Size, Version, License, Source)
    let context_bar = info_rows::create_context_bar(&pkg);
    content_box.append(&context_bar);

    // 2. Screenshots Previews (adw::Carousel) - only when genuine screenshots are available
    if !pkg.screenshots.is_empty() {
        let carousel_group = adw::PreferencesGroup::builder()
            .title(pastor_core::tr("Screenshots"))
            .build();

        let carousel = adw::Carousel::builder()
            .spacing(18)
            .allow_mouse_drag(true)
            .allow_scroll_wheel(true)
            .build();

        for url in &pkg.screenshots {
            let slide_card = create_screenshot_widget(url);
            carousel.append(&slide_card);
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
    }

    // 3. About Section
    let about_group = info_rows::create_about_group(&pkg);
    content_box.append(&about_group);

    // 4. What's New Section (only when real changelog notes exist)
    if let Some(whats_new_group) = info_rows::create_whats_new_group(&pkg) {
        content_box.append(&whats_new_group);
    }

    // 5. Technical Specifications
    let specs_group = info_rows::create_specs_group(&pkg);
    content_box.append(&specs_group);

    // 6. Dependencies Section
    if let Some(dep_group) = info_rows::create_dependencies_group(&pkg, &store, on_select_package) {
        content_box.append(&dep_group);
    }

    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));

    // Responsive BreakpointBin: adapts hero layout and spacing on mobile screens
    let bin = adw::BreakpointBin::builder()
        .width_request(200)
        .height_request(200)
        .hexpand(true)
        .vexpand(true)
        .build();
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        580.0,
        adw::LengthUnit::Px,
    ));

    let hw_c = hero_widgets.clone();
    let cbox_c = content_box.clone();
    bp.connect_apply(move |_| {
        hw_c.set_narrow(true);
        cbox_c.set_margin_start(12);
        cbox_c.set_margin_end(12);
    });

    let hw_u = hero_widgets.clone();
    let cbox_u = content_box.clone();
    bp.connect_unapply(move |_| {
        hw_u.set_narrow(false);
        cbox_u.set_margin_start(16);
        cbox_u.set_margin_end(16);
    });

    bin.add_breakpoint(bp);
    bin.set_child(Some(&scrolled));

    bin.upcast()

}
