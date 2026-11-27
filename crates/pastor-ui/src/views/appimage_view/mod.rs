pub mod inspect_section;
pub mod list_section;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gdk, gio, glib};
use pastor_core::{inspect_appimage, tr, AppImageMetadata, Package};
use pastor_store::Store;

use inspect_section::populate_inspect_view;
use list_section::{open_appimage_file_dialog, populate_installed_list};

#[derive(Clone)]
pub struct AppImageViewHandle {
    pub root: gtk4::Box,
    load_fn: Rc<dyn Fn(PathBuf)>,
    refresh_list_fn: Rc<dyn Fn()>,
}

impl AppImageViewHandle {
    pub fn load_file(&self, path: PathBuf) {
        (self.load_fn)(path);
    }

    pub fn refresh_list(&self) {
        (self.refresh_list_fn)();
    }
}

pub fn create_appimage_view(
    store: Store,
    on_select_package: Rc<dyn Fn(Package)>,
) -> AppImageViewHandle {
    let root = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .hexpand(true)
        .vexpand(true)
        .build();

    // 1. Manager / Installed List State
    let manager_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();
    let manager_clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(500)
        .margin_top(20)
        .margin_bottom(28)
        .margin_start(16)
        .margin_end(16)
        .build();
    let manager_outer = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .build();
    let manager_list_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .build();
    let manager_status = gtk4::Label::builder()
        .halign(gtk4::Align::Start)
        .wrap(true)
        .visible(false)
        .css_classes(["dim-label"])
        .build();
    manager_outer.append(&manager_status);
    manager_outer.append(&manager_list_box);
    manager_clamp.set_child(Some(&manager_outer));
    manager_scrolled.set_child(Some(&manager_clamp));
    stack.add_named(&manager_scrolled, Some("manager"));

    // 2. Loading State
    let loading_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(14)
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::Center)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(24)
        .margin_end(24)
        .build();
    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .width_request(36)
        .height_request(36)
        .build();
    let loading_label = gtk4::Label::builder()
        .label(tr(
            "Inspecting AppImage and searching Flatpak, AUR, and Official Repositories...",
        ))
        .wrap(true)
        .justify(gtk4::Justification::Center)
        .css_classes(["title-4"])
        .build();
    loading_box.append(&spinner);
    loading_box.append(&loading_label);
    stack.add_named(&loading_box, Some("loading"));

    // 3. Error State
    let error_page = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title(tr("Could Not Open AppImage"))
        .build();
    let error_btn_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk4::Align::Center)
        .build();
    let error_back_btn = gtk4::Button::builder()
        .label(tr("All AppImages"))
        .height_request(44)
        .css_classes(["pill"])
        .build();
    let error_retry_btn = gtk4::Button::builder()
        .label(tr("Choose Another .AppImage..."))
        .height_request(44)
        .css_classes(["suggested-action", "pill"])
        .build();
    error_btn_box.append(&error_back_btn);
    error_btn_box.append(&error_retry_btn);
    error_page.set_child(Some(&error_btn_box));
    stack.add_named(&error_page, Some("error"));

    // 4. Inspect State
    let inspect_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();
    let inspect_clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(500)
        .margin_top(20)
        .margin_bottom(28)
        .margin_start(16)
        .margin_end(16)
        .build();
    let inspect_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .build();
    inspect_clamp.set_child(Some(&inspect_box));
    inspect_scrolled.set_child(Some(&inspect_clamp));
    stack.add_named(&inspect_scrolled, Some("inspect"));

    stack.set_visible_child_name("manager");
    root.append(&stack);

    let req_id_cell: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));
    let load_slot: Rc<RefCell<Option<Rc<dyn Fn(PathBuf)>>>> = Rc::new(RefCell::new(None));
    let refresh_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let refresh_list_fn: Rc<dyn Fn()> = {
        let stack = stack.clone();
        let list_box = manager_list_box.clone();
        let status = manager_status.clone();
        let load_slot_c = load_slot.clone();
        let refresh_slot_c = refresh_slot.clone();

        Rc::new(move || {
            let on_open = load_slot_c
                .borrow()
                .clone()
                .unwrap_or_else(|| Rc::new(|_| {}));
            let on_ref = refresh_slot_c
                .borrow()
                .clone()
                .unwrap_or_else(|| Rc::new(|| {}));
            populate_installed_list(&list_box, &status, on_open, on_ref);
            stack.set_visible_child_name("manager");
        })
    };
    *refresh_slot.borrow_mut() = Some(refresh_list_fn.clone());

    let trigger_load: Rc<dyn Fn(PathBuf)> = {
        let store = store;
        let stack = stack.clone();
        let inspect_box = inspect_box;
        let error_page = error_page;
        let on_select = on_select_package;
        let req_cell = req_id_cell;
        let refresh_c = refresh_list_fn.clone();

        Rc::new(move |appimage_path: PathBuf| {
            let next_id = {
                let mut g = req_cell.borrow_mut();
                *g += 1;
                *g
            };

            stack.set_visible_child_name("loading");

            let store_c = store.clone();
            let stack_c = stack.clone();
            let inspect_c = inspect_box.clone();
            let error_c = error_page.clone();
            let select_c = on_select.clone();
            let req_c = req_cell.clone();
            let back_c = refresh_c.clone();

            glib::MainContext::default().spawn_local(async move {
                let path_bg = appimage_path.clone();
                let inspect_res =
                    tokio::task::spawn_blocking(move || inspect_appimage(&path_bg)).await;

                if *req_c.borrow() != next_id {
                    return;
                }

                let meta: AppImageMetadata = match inspect_res {
                    Ok(Ok(m)) => m,
                    Ok(Err(e)) => {
                        error_c.set_description(Some(&e.to_string()));
                        stack_c.set_visible_child_name("error");
                        return;
                    }
                    Err(e) => {
                        error_c.set_description(Some(&e.to_string()));
                        stack_c.set_visible_child_name("error");
                        return;
                    }
                };

                let fname = appimage_path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string());
                let alternatives = store_c
                    .find_appimage_alternatives(&meta, fname.as_deref())
                    .await;

                if *req_c.borrow() != next_id {
                    return;
                }

                populate_inspect_view(
                    &inspect_c,
                    appimage_path,
                    meta,
                    alternatives,
                    select_c,
                    back_c,
                );
                stack_c.set_visible_child_name("inspect");
            });
        })
    };
    *load_slot.borrow_mut() = Some(trigger_load.clone());

    {
        let ref_c = refresh_list_fn.clone();
        error_back_btn.connect_clicked(move |_| ref_c());
    }
    {
        let trig_c = trigger_load.clone();
        let btn_c = error_retry_btn.clone();
        error_retry_btn.connect_clicked(move |_| {
            open_appimage_file_dialog(&btn_c, tr("Select AppImage File"), trig_c.clone());
        });
    }

    {
        let drop_target = gtk4::DropTarget::new(
            gio::File::static_type(),
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );
        let trig_drop = trigger_load.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            if let Ok(file) = value.get::<gio::File>() {
                if let Some(p) = file.path() {
                    if p.to_string_lossy().to_lowercase().ends_with(".appimage") {
                        trig_drop(p);
                        return true;
                    }
                }
            }
            false
        });
        manager_scrolled.add_controller(drop_target);
    }

    refresh_list_fn();

    AppImageViewHandle {
        root,
        load_fn: trigger_load,
        refresh_list_fn,
    }
}
