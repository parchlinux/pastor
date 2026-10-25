use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageIcon, PackageSource};
use pastor_store::Store;

use super::package_row::PackageRowWidgets;
use crate::models::{PackageObject, TxMonitor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFilter {
    All,
    Official,
    Aur,
    Flatpak,
    Updates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    NameAsc,
    NameDesc,
    SizeDesc,
}

/// Heuristic: does this entry look like an application rather than a system
/// library? AppStream-matched packages (display_name set), grouped/categorized
/// packages, and all non-ALPM sources count.
fn is_app_like(p: &Package) -> bool {
    if p.display_name.is_some() || !p.categories.is_empty() {
        return true;
    }
    match p.id.source {
        PackageSource::Aur | PackageSource::Flatpak { .. } | PackageSource::Waydroid | PackageSource::Bootc => true,
        PackageSource::Parch(_) | PackageSource::Arch(_) => matches!(p.icon, Some(PackageIcon::LocalPath(_))),
    }
}

fn matches_filter(p: &Package, f: SourceFilter) -> bool {
    match f {
        SourceFilter::All => true,
        SourceFilter::Official => matches!(p.id.source, PackageSource::Parch(_) | PackageSource::Arch(_)),
        SourceFilter::Aur => matches!(p.id.source, PackageSource::Aur),
        SourceFilter::Flatpak => matches!(p.id.source, PackageSource::Flatpak { .. }),
        SourceFilter::Updates => p.state == pastor_core::PackageState::UpdateAvailable,
    }
}

fn sort_packages(pkgs: &mut Vec<Package>, mode: SortMode) {
    match mode {
        SortMode::NameAsc => pkgs.sort_by(|a, b| a.display_title().to_lowercase().cmp(&b.display_title().to_lowercase())),
        SortMode::NameDesc => pkgs.sort_by(|a, b| b.display_title().to_lowercase().cmp(&a.display_title().to_lowercase())),
        SortMode::SizeDesc => pkgs.sort_by(|a, b| b.size_installed.unwrap_or(0).cmp(&a.size_installed.unwrap_or(0))),
    }
}

/// A virtualized package list (`gtk4::ListView` + `gio::ListStore<PackageObject>`).
/// Data is refreshed in place via `set_packages` — the widget tree is never
/// rebuilt, so a 1000+ package list stays responsive across refreshes.
pub struct PackageListView {
    pub widget: gtk4::Widget,
    pub set_packages: Rc<dyn Fn(Vec<Package>)>,
}

pub fn create_package_list_view(
    title: &str,
    description: &str,
    packages: Vec<Package>,
    store: Store,
    monitor: TxMonitor,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
    default_apps_only: bool,
) -> PackageListView {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(16)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .build();

    let model = gtk4::gio::ListStore::new::<PackageObject>();
    let selection = gtk4::NoSelection::new(Some(model.clone()));

    // Per-list-item slots for the currently bound row widgets, keyed by the
    // ListItem pointer (stable while the item exists). All access happens on
    // the GTK main thread; entries are cleared on unbind and item destroy so
    // recycled rows never leak monitor handlers.
    let cells: Rc<RefCell<HashMap<usize, Rc<PackageRowWidgets>>>> = Rc::new(RefCell::new(HashMap::new()));

    let factory = gtk4::SignalListItemFactory::new();
    {
        let cells = cells.clone();
        factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let key = list_item.as_ptr() as usize;
            let placeholder = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(["card"])
                .margin_top(4)
                .margin_bottom(4)
                .build();
            list_item.set_child(Some(&placeholder));

            let cells = cells.clone();
            placeholder.connect_destroy(move |_| {
                cells.borrow_mut().remove(&key);
            });
        });
    }
    {
        let cells = cells.clone();
        let store = store.clone();
        let monitor = monitor.clone();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let key = list_item.as_ptr() as usize;

            if let Some(old) = cells.borrow_mut().remove(&key) {
                old.unbind();
            }

            let Some(pkg_obj) = list_item.item().and_downcast::<PackageObject>() else {
                return;
            };
            let pkg = pkg_obj.package();
            let widgets = Rc::new(PackageRowWidgets::build(
                &pkg,
                &store,
                &monitor,
                on_select.clone(),
                on_transaction_start.clone(),
            ));
            widgets.bind();

            if let Some(placeholder) = list_item.child().and_downcast::<gtk4::Box>() {
                while let Some(child) = placeholder.first_child() {
                    placeholder.remove(&child);
                }
                placeholder.append(widgets.root());
            }
            cells.borrow_mut().insert(key, widgets);
        });
    }
    {
        let cells = cells.clone();
        factory.connect_unbind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let key = list_item.as_ptr() as usize;
            if let Some(old) = cells.borrow_mut().remove(&key) {
                old.unbind();
            }
        });
    }

    let list_view = gtk4::ListView::new(Some(selection), Some(factory));

    // ----- Filter / sort bar -----
    let filter_bar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(10)
        .margin_bottom(4)
        .build();

    let linked_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .css_classes(["linked"])
        .hexpand(true)
        .build();

    let btn_all = gtk4::ToggleButton::builder().label("All").active(true).build();
    let btn_apps = gtk4::ToggleButton::builder().label("Applications").group(&btn_all).build();
    let btn_official = gtk4::ToggleButton::builder().label("Official").group(&btn_all).build();
    let btn_aur = gtk4::ToggleButton::builder().label("AUR").group(&btn_all).build();
    let btn_flatpak = gtk4::ToggleButton::builder().label("Flatpak").group(&btn_all).build();
    let btn_updates = gtk4::ToggleButton::builder().label("Updates").group(&btn_all).build();

    for b in [&btn_all, &btn_apps, &btn_official, &btn_aur, &btn_flatpak, &btn_updates] {
        linked_box.append(b);
    }

    let sort_strings = gtk4::StringList::new(&["Name (A–Z)", "Name (Z–A)", "Size (largest first)"]);
    let sort_dropdown = gtk4::DropDown::builder().model(&sort_strings).build();
    sort_dropdown.set_tooltip_text(Some("Sort packages"));
    sort_dropdown.update_property(&[gtk4::accessible::Property::Label("Sort packages")]);

    filter_bar.append(&linked_box);
    filter_bar.append(&sort_dropdown);

    let count_lbl = gtk4::Label::builder()
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();

    let empty_status = adw::StatusPage::builder()
        .icon_name("system-search-symbolic")
        .title(title)
        .description(description)
        .visible(false)
        .build();

    content_box.append(&filter_bar);
    content_box.append(&count_lbl);
    content_box.append(&list_view);
    content_box.append(&empty_status);
    scrolled.set_child(Some(&content_box));

    // ----- State + in-place refresh -----
    let all_packages: Rc<RefCell<Vec<Package>>> = Rc::new(RefCell::new(Vec::new()));
    let current_filter: Rc<RefCell<SourceFilter>> = Rc::new(RefCell::new(SourceFilter::All));
    let apps_only: Rc<RefCell<bool>> = Rc::new(RefCell::new(default_apps_only));
    let current_sort: Rc<RefCell<SortMode>> = Rc::new(RefCell::new(SortMode::NameAsc));

    let apply = {
        let model = model.clone();
        let all = all_packages.clone();
        let filter = current_filter.clone();
        let apps = apps_only.clone();
        let sort = current_sort.clone();
        let count_lbl = count_lbl.clone();
        let empty_status = empty_status.clone();
        let list_view = list_view.clone();
        let filter_bar = filter_bar.clone();

        move || {
            let f = *filter.borrow();
            let apps_only = *apps.borrow();
            let s = *sort.borrow();

            let total = all.borrow().len();
            let mut visible: Vec<Package> = all
                .borrow()
                .iter()
                .filter(|p| matches_filter(p, f) && (!apps_only || is_app_like(p)))
                .cloned()
                .collect();
            sort_packages(&mut visible, s);
            let shown = visible.len();

            model.remove_all();
            for p in &visible {
                model.append(&PackageObject::new(p.clone()));
            }

            let text = if shown == total {
                format!("{total} packages")
            } else {
                format!("{shown} of {total} packages")
            };
            count_lbl.set_text(&text);
            let has_any = total > 0;
            filter_bar.set_visible(has_any);
            count_lbl.set_visible(has_any);
            list_view.set_visible(has_any && shown > 0);
            empty_status.set_visible(!has_any || shown == 0);
        }
    };

    let set_packages: Rc<dyn Fn(Vec<Package>)> = {
        let all = all_packages.clone();
        let apply = apply.clone();
        Rc::new(move |pkgs| {
            *all.borrow_mut() = pkgs;
            apply();
        })
    };

    set_packages(packages);

    // ----- Wire filter chips -----
    macro_rules! wire_filter {
        ($btn:expr, $mode:expr) => {{
            let filter = current_filter.clone();
            let apps = apps_only.clone();
            let apply = apply.clone();
            $btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    *filter.borrow_mut() = $mode;
                    *apps.borrow_mut() = false;
                    apply();
                }
            });
        }};
    }
    wire_filter!(btn_all, SourceFilter::All);
    wire_filter!(btn_official, SourceFilter::Official);
    wire_filter!(btn_aur, SourceFilter::Aur);
    wire_filter!(btn_flatpak, SourceFilter::Flatpak);
    wire_filter!(btn_updates, SourceFilter::Updates);

    {
        let apps = apps_only.clone();
        let filter = current_filter.clone();
        let apply = apply.clone();
        btn_apps.connect_toggled(move |btn| {
            if btn.is_active() {
                *apps.borrow_mut() = true;
                *filter.borrow_mut() = SourceFilter::All;
                apply();
            }
        });
    }

    {
        let sort = current_sort.clone();
        let apply = apply.clone();
        sort_dropdown.connect_selected_notify(move |dd| {
            *sort.borrow_mut() = match dd.selected() {
                1 => SortMode::NameDesc,
                2 => SortMode::SizeDesc,
                _ => SortMode::NameAsc,
            };
            apply();
        });
    }

    if default_apps_only {
        btn_apps.set_active(true);
    }

    PackageListView {
        widget: scrolled.upcast(),
        set_packages,
    }
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

    let esc_title = glib::markup_escape_text(title);
    let esc_desc = glib::markup_escape_text(description);
    let group = adw::PreferencesGroup::builder()
        .title(esc_title.as_str())
        .description(esc_desc.as_str())
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

    let loading_lbl = gtk4::Label::builder()
        .label("Searching repositories, AUR, and Flathub catalog…")
        .css_classes(["dim-label", "caption"])
        .build();

    spinner_box.append(&spinner);
    spinner_box.append(&loading_lbl);
    group.add(&spinner_box);
    content_box.append(&group);

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}
