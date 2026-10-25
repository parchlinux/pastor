use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use pastor_core::Package;
use pastor_store::Store;

use crate::models::TxMonitor;
use crate::views::{create_loading_view, create_package_list_view};

pub struct SearchController {
    pub show_search_fn: Rc<dyn Fn(String)>,
    pub open_package_fn: Rc<dyn Fn(String)>,
}

pub fn setup_search_controller(
    search_entry: &gtk4::SearchEntry,
    store: &Store,
    monitor: &TxMonitor,
    navigate_to_view: Rc<dyn Fn(&str, Option<gtk4::Widget>)>,
    on_select_pkg: impl Fn(Package) + 'static + Clone,
    on_tx_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
    go_back: Rc<dyn Fn()>,
    current_page_name: Rc<RefCell<String>>,
    sync_title: Rc<dyn Fn()>,
    active_section_title: Rc<RefCell<String>>,
    active_section_sub: Rc<RefCell<String>>,
) -> SearchController {
    let search_generation = Rc::new(Cell::new(0u64));
    let debounce_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    let execute_search = {
        let nav = navigate_to_view.clone();
        let store = store.clone();
        let monitor = monitor.clone();
        let on_sel = on_select_pkg.clone();
        let on_tx = on_tx_start.clone();
        let cur_t = active_section_title.clone();
        let cur_s = active_section_sub.clone();
        let sync_fn = sync_title.clone();
        let page_ref = current_page_name.clone();
        let s_entry = search_entry.clone();
        let search_gen = search_generation.clone();
        let go_back_exec = go_back.clone();

        Rc::new(move |query: String| {
            let trimmed = query.trim().to_string();
            if trimmed.is_empty() {
                search_gen.set(search_gen.get() + 1);
                if page_ref.borrow().as_str() == "search" {
                    go_back_exec();
                }
                return;
            }

            let my_gen = search_gen.get() + 1;
            search_gen.set(my_gen);

            *cur_t.borrow_mut() = format!("Search “{}”", trimmed);
            *cur_s.borrow_mut() = "".to_string();
            sync_fn();

            let nav_s = nav.clone();
            let store_s = store.clone();
            let monitor_s = monitor.clone();
            let on_sel_s = on_sel.clone();
            let on_tx_s = on_tx.clone();
            let q_title = format!("Search results for \"{}\"", trimmed);
            let page_ref_task = page_ref.clone();
            let search_gen_task = search_gen.clone();
            let search_entry_task = s_entry.clone();
            let query_task = trimmed.clone();

            nav_s(
                "search",
                Some(create_loading_view(
                    &q_title,
                    "Searching repositories and Flathub catalog…",
                )),
            );

            glib::spawn_future_local(async move {
                if let Ok(results) = store_s.search_raw(&query_task).await {
                    // Guard 1: discard if a newer search was initiated
                    if search_gen_task.get() != my_gen {
                        return;
                    }
                    // Guard 2: discard if the user navigated away from the search view
                    if page_ref_task.borrow().as_str() != "search" {
                        return;
                    }
                    // Guard 3: discard if the search entry text has changed
                    if search_entry_task.text().trim() != query_task {
                        return;
                    }

                    let list_view = create_package_list_view(
                        &q_title,
                        "Filtered packages matching your search query across all active ecosystems",
                        results,
                        store_s,
                        monitor_s,
                        on_sel_s,
                        on_tx_s,
                        false,
                    )
                    .widget;
                    nav_s("search", Some(list_view));
                }
            });
        })
    };

    let show_search_fn: Rc<dyn Fn(String)> = {
        let search_entry_for_fn = search_entry.clone();
        let exec_search_for_fn = execute_search.clone();
        Rc::new(move |q: String| {
            search_entry_for_fn.set_text(&q);
            exec_search_for_fn(q);
        })
    };

    let open_package_fn: Rc<dyn Fn(String)> = {
        let nav = navigate_to_view.clone();
        let store = store.clone();
        let on_sel = on_select_pkg.clone();
        let exec_search = execute_search.clone();
        let search_entry_sub = search_entry.clone();

        Rc::new(move |pkg_name: String| {
            let clean = pkg_name.strip_suffix(".desktop").unwrap_or(&pkg_name).to_string();
            let nav_c = nav.clone();
            let store_c = store.clone();
            let on_sel_c = on_sel.clone();
            let exec_search_c = exec_search.clone();
            let search_entry_inner = search_entry_sub.clone();
            let target_name = clean.clone();

            nav_c(
                "details",
                Some(create_loading_view(
                    &format!("Loading {}…", target_name),
                    "Querying repositories, AUR, and Flathub catalog…",
                )),
            );

            glib::spawn_future_local(async move {
                if let Ok(results) = store_c.search_raw(&target_name).await {
                    // Look for exact match first
                    let exact = results.iter().find(|p| {
                        let p_clean = p.name.strip_suffix(".desktop").unwrap_or(&p.name);
                        p.name.eq_ignore_ascii_case(&target_name)
                            || p.id.name.eq_ignore_ascii_case(&target_name)
                            || p_clean.eq_ignore_ascii_case(&target_name)
                            || p.display_title().eq_ignore_ascii_case(&target_name)
                    });

                    if let Some(pkg) = exact {
                        on_sel_c(pkg.clone());
                    } else if !results.is_empty() {
                        search_entry_inner.set_text(&target_name);
                        exec_search_c(target_name);
                    } else {
                        let not_found_page = adw::StatusPage::builder()
                            .icon_name("system-search-symbolic")
                            .title(format!("Package '{}' Not Found", target_name))
                            .description("The requested package was not found in active repositories, AUR, or Flatpak.")
                            .build();
                        nav_c("details", Some(not_found_page.upcast()));
                    }
                }
            });
        })
    };

    // Search entry text changed (debounced 250ms)
    {
        let exec = execute_search.clone();
        let timer_holder = debounce_timer.clone();
        let page_ref = current_page_name.clone();
        let go_back_changed = go_back.clone();
        let search_gen_changed = search_generation.clone();

        search_entry.connect_search_changed(move |entry| {
            if let Some(source) = timer_holder.borrow_mut().take() {
                source.remove();
            }

            let query = entry.text().to_string();
            if query.trim().is_empty() {
                search_gen_changed.set(search_gen_changed.get() + 1);
                if page_ref.borrow().as_str() == "search" {
                    go_back_changed();
                }
                return;
            }

            let exec_clone = exec.clone();
            let timer_holder_inner = timer_holder.clone();
            let query_clone = query.clone();

            let source_id = glib::timeout_add_local_once(
                std::time::Duration::from_millis(250),
                move || {
                    timer_holder_inner.borrow_mut().take();
                    exec_clone(query_clone);
                },
            );
            *timer_holder.borrow_mut() = Some(source_id);
        });
    }

    // Search entry Enter pressed (immediate execution, cancels debounce)
    {
        let exec = execute_search.clone();
        let timer_holder = debounce_timer.clone();
        search_entry.connect_activate(move |entry| {
            if let Some(source) = timer_holder.borrow_mut().take() {
                source.remove();
            }
            let query = entry.text().to_string();
            exec(query);
        });
    }

    // Stop search (Escape or clear button clicked)
    {
        let page_ref = current_page_name.clone();
        let go_back_c = go_back.clone();
        let timer_holder = debounce_timer.clone();
        let search_gen = search_generation.clone();
        search_entry.connect_stop_search(move |_| {
            if let Some(source) = timer_holder.borrow_mut().take() {
                source.remove();
            }
            search_gen.set(search_gen.get() + 1);
            if page_ref.borrow().as_str() == "search" {
                go_back_c();
            }
        });
    }

    SearchController {
        show_search_fn,
        open_package_fn,
    }
}
