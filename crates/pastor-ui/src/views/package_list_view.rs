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
/// Data is refreshed in place via `set_packages`: the widget tree is never
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
    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(16)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .vexpand(true)
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
        let on_sel_list = on_select.clone();
        let on_tx_list = on_transaction_start.clone();
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
                on_sel_list.clone(),
                on_tx_list.clone(),
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

    let list_view = gtk4::ListView::new(Some(selection.clone()), Some(factory));
    let sw_list = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&list_view)
        .build();

    // Grid View implementation (EXP-GRID)
    let grid_cells: Rc<RefCell<HashMap<usize, Rc<PackageRowWidgets>>>> = Rc::new(RefCell::new(HashMap::new()));
    let grid_factory = gtk4::SignalListItemFactory::new();
    {
        let cells = grid_cells.clone();
        grid_factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let key = list_item.as_ptr() as usize;
            let placeholder = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .halign(gtk4::Align::Fill)
                .valign(gtk4::Align::Start)
                .hexpand(true)
                .margin_top(3)
                .margin_bottom(3)
                .margin_start(3)
                .margin_end(3)
                .build();
            list_item.set_child(Some(&placeholder));

            let cells = cells.clone();
            placeholder.connect_destroy(move |_| {
                cells.borrow_mut().remove(&key);
            });
        });
    }
    {
        let cells = grid_cells.clone();
        let store = store.clone();
        let monitor = monitor.clone();
        let on_sel = on_select.clone();
        let on_tx = on_transaction_start.clone();
        grid_factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let key = list_item.as_ptr() as usize;

            if let Some(old) = cells.borrow_mut().remove(&key) {
                old.unbind();
            }

            let Some(pkg_obj) = list_item.item().and_downcast::<PackageObject>() else {
                return;
            };
            let pkg = pkg_obj.package();
            let widgets = Rc::new(PackageRowWidgets::build_card(
                &pkg,
                &store,
                &monitor,
                on_sel.clone(),
                on_tx.clone(),
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
        let cells = grid_cells.clone();
        grid_factory.connect_unbind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let key = list_item.as_ptr() as usize;
            if let Some(old) = cells.borrow_mut().remove(&key) {
                old.unbind();
            }
        });
    }

    let grid_view = gtk4::GridView::builder()
        .model(&selection)
        .factory(&grid_factory)
        .min_columns(2)
        .max_columns(7)
        .enable_rubberband(false)
        .build();

    let sw_grid = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&grid_view)
        .build();

    let view_stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .vhomogeneous(false)
        .hhomogeneous(false)
        .interpolate_size(true)
        .vexpand(true)
        .build();
    view_stack.add_named(&sw_list, Some("list"));
    view_stack.add_named(&sw_grid, Some("grid"));

    // ----- Filter / sort / view bar: responsive wrapping FlowBox (no horizontal overflow on mobile) -----
    let controls_bar = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .max_children_per_line(3)
        .min_children_per_line(1)
        .row_spacing(6)
        .column_spacing(8)
        .homogeneous(false)
        .valign(gtk4::Align::Center)
        .margin_bottom(6)
        .build();

    let filter_items = [
        pastor_core::tr("All Sources"),
        pastor_core::tr("Applications"),
        pastor_core::tr("Official"),
        pastor_core::tr("AUR"),
        pastor_core::tr("Flatpak"),
        pastor_core::tr("Updates"),
    ];
    let filter_strings = gtk4::StringList::new(&filter_items);
    let filter_dropdown = gtk4::DropDown::builder()
        .model(&filter_strings)
        .hexpand(true)
        .tooltip_text(pastor_core::tr("Filter by source"))
        .build();
    filter_dropdown.update_property(&[gtk4::accessible::Property::Label(pastor_core::tr("Filter by source"))]);

    let sort_items = [
        pastor_core::tr("Name (A-Z)"),
        pastor_core::tr("Name (Z-A)"),
        pastor_core::tr("Size (largest first)"),
    ];
    let sort_strings = gtk4::StringList::new(&sort_items);
    let sort_dropdown = gtk4::DropDown::builder()
        .model(&sort_strings)
        .hexpand(true)
        .build();
    let sort_tip = pastor_core::tr("Sort packages");
    sort_dropdown.set_tooltip_text(Some(sort_tip));
    sort_dropdown.update_property(&[gtk4::accessible::Property::Label(sort_tip)]);

    // View Switcher (List vs Grid)
    let view_switch_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .css_classes(["linked"])
        .valign(gtk4::Align::Center)
        .build();

    let btn_view_list = gtk4::ToggleButton::builder()
        .icon_name("view-list-symbolic")
        .tooltip_text(pastor_core::tr("List view"))
        .active(true)
        .build();
    btn_view_list.update_property(&[gtk4::accessible::Property::Label(pastor_core::tr("List view"))]);

    let btn_view_grid = gtk4::ToggleButton::builder()
        .icon_name("view-grid-symbolic")
        .tooltip_text(pastor_core::tr("Grid view"))
        .group(&btn_view_list)
        .build();
    btn_view_grid.update_property(&[gtk4::accessible::Property::Label(pastor_core::tr("Grid view"))]);

    {
        let vs = view_stack.clone();
        btn_view_list.connect_toggled(move |btn| {
            if btn.is_active() {
                vs.set_visible_child_name("list");
            }
        });
    }
    {
        let vs = view_stack.clone();
        btn_view_grid.connect_toggled(move |btn| {
            if btn.is_active() {
                vs.set_visible_child_name("grid");
            }
        });
    }

    view_switch_box.append(&btn_view_list);
    view_switch_box.append(&btn_view_grid);

    controls_bar.append(&filter_dropdown);
    controls_bar.append(&sort_dropdown);
    controls_bar.append(&view_switch_box);

    let count_lbl = gtk4::Label::builder()
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();

    let empty_status = adw::StatusPage::builder()
        .icon_name("system-search-symbolic")
        .title(pastor_core::tr(title))
        .description(pastor_core::tr(description))
        .vexpand(true)
        .visible(false)
        .build();

    content_box.append(&controls_bar);
    content_box.append(&count_lbl);
    content_box.append(&view_stack);
    content_box.append(&empty_status);
    let clamp = adw::Clamp::builder()
        .maximum_size(1080)
        .tightening_threshold(720)
        .vexpand(true)
        .build();
    clamp.set_child(Some(&content_box));

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
        let view_stack = view_stack.clone();
        let controls_bar = controls_bar.clone();

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

            let text = if pastor_core::is_persian() {
                if shown == total {
                    format!("{total} بسته")
                } else {
                    format!("{shown} از {total} بسته")
                }
            } else if shown == total {
                format!("{total} packages")
            } else {
                format!("{shown} of {total} packages")
            };
            count_lbl.set_text(&text);
            let has_any = total > 0;
            controls_bar.set_visible(has_any);
            count_lbl.set_visible(has_any);
            view_stack.set_visible(has_any && shown > 0);
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

    // ----- Wire filter dropdown -----
    {
        let filter = current_filter.clone();
        let apps = apps_only.clone();
        let apply = apply.clone();
        filter_dropdown.connect_selected_notify(move |dd| {
            match dd.selected() {
                0 => {
                    *filter.borrow_mut() = SourceFilter::All;
                    *apps.borrow_mut() = false;
                }
                1 => {
                    *filter.borrow_mut() = SourceFilter::All;
                    *apps.borrow_mut() = true;
                }
                2 => {
                    *filter.borrow_mut() = SourceFilter::Official;
                    *apps.borrow_mut() = false;
                }
                3 => {
                    *filter.borrow_mut() = SourceFilter::Aur;
                    *apps.borrow_mut() = false;
                }
                4 => {
                    *filter.borrow_mut() = SourceFilter::Flatpak;
                    *apps.borrow_mut() = false;
                }
                5 => {
                    *filter.borrow_mut() = SourceFilter::Updates;
                    *apps.borrow_mut() = false;
                }
                _ => {}
            }
            apply();
        });
    }

    if default_apps_only {
        filter_dropdown.set_selected(1);
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

    PackageListView {
        widget: clamp.upcast(),
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

    let clamp = adw::Clamp::builder()
        .maximum_size(1080)
        .tightening_threshold(720)
        .build();
    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));
    scrolled.upcast()
}
