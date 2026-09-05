use adw::prelude::*;
use pastor_core::Package;
use pastor_store::Store;

use super::package_row::create_package_row;

pub fn create_package_list_view(
    title: &str,
    description: &str,
    packages: Vec<Package>,
    store: Store,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .margin_top(16)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .build();

    if packages.is_empty() {
        let status_page = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title("No Packages Found")
            .description("No software matches your current query or category filter.")
            .build();
        content_box.append(&status_page);
    } else {
        let group = adw::PreferencesGroup::builder()
            .title(title)
            .description(description)
            .build();

        let list_box = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        for pkg in packages {
            let row = create_package_row(
                &pkg,
                &store,
                on_select.clone(),
                on_transaction_start.clone(),
            );
            list_box.append(&row);
        }

        group.add(&list_box);
        content_box.append(&group);
    }

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}

pub fn create_loading_view(title: &str, description: &str) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .build();

    let group = adw::PreferencesGroup::builder()
        .title(title)
        .description(description)
        .build();

    let spinner_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(48)
        .margin_bottom(48)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();

    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .width_request(40)
        .height_request(40)
        .build();
    spinner_box.append(&spinner);

    let loading_label = gtk4::Label::builder()
        .label("Loading software packages…")
        .css_classes(["caption", "dim-label"])
        .build();
    spinner_box.append(&loading_label);

    group.add(&spinner_box);
    content_box.append(&group);

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}

