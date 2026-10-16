use std::rc::Rc;

use adw::prelude::*;
use gtk4::pango;
use pastor_core::Package;
use pastor_store::Store;

use crate::views::CircularProgress;

pub fn create_operations_popover(
    store: &Store,
    transaction_bar: &crate::views::TransactionBar,
    on_select_pkg: impl Fn(Package) + 'static + Clone,
) -> (gtk4::Button, Rc<dyn Fn()>) {
    let header_ops_btn = gtk4::Button::builder()
        .tooltip_text("Active Operations")
        .css_classes(["flat", "circular"])
        .visible(false)
        .valign(gtk4::Align::Center)
        .build();

    let header_ops_circle = CircularProgress::new(22);
    header_ops_btn.set_child(Some(header_ops_circle.widget()));

    let header_ops_popover = gtk4::Popover::builder()
        .autohide(true)
        .build();
    header_ops_popover.set_parent(&header_ops_btn);

    {
        let pop = header_ops_popover.clone();
        header_ops_btn.connect_clicked(move |_| {
            if pop.is_visible() {
                pop.popdown();
            } else {
                pop.popup();
            }
        });
    }

    let pop_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .width_request(340)
        .build();

    let pop_header = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_bottom(4)
        .build();

    let pop_titles = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();

    let pop_title_lbl = gtk4::Label::builder()
        .label("Active Operations")
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();
    pop_titles.append(&pop_title_lbl);

    let pop_sub_lbl = gtk4::Label::builder()
        .label("0 operations active")
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();
    pop_titles.append(&pop_sub_lbl);
    pop_header.append(&pop_titles);

    let pop_cancel_all_btn = gtk4::Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Cancel all operations")
        .css_classes(["flat", "circular"])
        .valign(gtk4::Align::Center)
        .build();
    pop_header.append(&pop_cancel_all_btn);
    pop_box.append(&pop_header);

    let pop_sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    pop_box.append(&pop_sep);

    let pop_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    let pop_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(260)
        .propagate_natural_height(true)
        .child(&pop_list)
        .build();
    pop_box.append(&pop_scrolled);

    let pop_bottom_sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    pop_box.append(&pop_bottom_sep);

    let pop_console_btn = gtk4::Button::builder()
        .icon_name("utilities-terminal-symbolic")
        .label("Native Transaction Console")
        .css_classes(["flat"])
        .halign(gtk4::Align::Center)
        .build();
    pop_box.append(&pop_console_btn);

    {
        let tb = transaction_bar.clone();
        let pop = header_ops_popover.clone();
        pop_console_btn.connect_clicked(move |_| {
            pop.popdown();
            tb.toggle_console();
        });
    }

    {
        let store = store.clone();
        pop_cancel_all_btn.connect_clicked(move |_| {
            let active = store.get_active_transactions();
            let store = store.clone();
            gtk4::glib::spawn_future_local(async move {
                for tx in active {
                    let _ = store.cancel_installation(&tx.package_id).await;
                }
            });
        });
    }

    header_ops_popover.set_child(Some(&pop_box));

    let update_ops = {
        let store = store.clone();
        let ops_btn = header_ops_btn.clone();
        let circle = header_ops_circle.clone();
        let sub_lbl = pop_sub_lbl.clone();
        let list = pop_list.clone();
        let pop = header_ops_popover.clone();
        let on_sel = on_select_pkg.clone();

        Rc::new(move || {
            let active = store.get_active_transactions();
            if active.is_empty() {
                ops_btn.set_visible(false);
                pop.popdown();
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                return;
            }

            ops_btn.set_visible(true);
            let total_frac: f32 = active.iter().map(|a| a.progress_fraction).sum();
            let avg_frac = (total_frac / active.len() as f32) as f64;
            circle.set_fraction(avg_frac);
            let pct = (avg_frac * 100.0).round() as u32;
            sub_lbl.set_text(&format!("{} operations active ({}%)", active.len(), pct));

            // Clear and rebuild rows
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }

            for item in &active {
                let row = gtk4::ListBoxRow::builder()
                    .activatable(true)
                    .build();

                let hbox = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Horizontal)
                    .spacing(10)
                    .margin_top(8)
                    .margin_bottom(8)
                    .margin_start(10)
                    .margin_end(10)
                    .build();

                let icon = gtk4::Image::builder()
                    .icon_name(crate::icons::installed_icon())
                    .pixel_size(24)
                    .build();
                hbox.append(&icon);

                let vbox = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(2)
                    .hexpand(true)
                    .build();

                let name_lbl = gtk4::Label::builder()
                    .label(&item.package_name)
                    .halign(gtk4::Align::Start)
                    .ellipsize(pango::EllipsizeMode::End)
                    .css_classes(["body", "bold"])
                    .build();
                vbox.append(&name_lbl);

                let sub_text = if !item.log_message.trim().is_empty() {
                    format!("{}% • {}", (item.progress_fraction * 100.0) as u32, item.log_message.trim())
                } else {
                    format!("{}%", (item.progress_fraction * 100.0) as u32)
                };
                let sub_lbl = gtk4::Label::builder()
                    .label(&sub_text)
                    .halign(gtk4::Align::Start)
                    .ellipsize(pango::EllipsizeMode::End)
                    .css_classes(["caption", "dim-label"])
                    .build();
                vbox.append(&sub_lbl);
                hbox.append(&vbox);

                let mini_circle = CircularProgress::new(18);
                mini_circle.set_fraction(item.progress_fraction as f64);
                hbox.append(mini_circle.widget());

                let item_cancel_btn = gtk4::Button::builder()
                    .icon_name("process-stop-symbolic")
                    .tooltip_text("Cancel installation")
                    .css_classes(["flat", "circular"])
                    .valign(gtk4::Align::Center)
                    .build();

                {
                    let s = store.clone();
                    let pid = item.package_id.clone();
                    item_cancel_btn.connect_clicked(move |_| {
                        let s_c = s.clone();
                        let pid_c = pid.clone();
                        gtk4::glib::spawn_future_local(async move {
                            let _ = s_c.cancel_installation(&pid_c).await;
                        });
                    });
                }
                hbox.append(&item_cancel_btn);

                row.set_child(Some(&hbox));

                // Clicking row content navigates to details and closes popover
                {
                    let gesture = gtk4::GestureClick::new();
                    let on_sel_inner = on_sel.clone();
                    let s_inner = store.clone();
                    let pid_inner = item.package_id.clone();
                    let pop_close = pop.clone();
                    gesture.connect_released(move |_, _, _, _| {
                        pop_close.popdown();
                        let on_sel_c = on_sel_inner.clone();
                        let s_c = s_inner.clone();
                        let p_c = pid_inner.clone();
                        gtk4::glib::spawn_future_local(async move {
                            if let Ok(Some(pkg)) = s_c.get_package(&p_c).await {
                                on_sel_c(pkg);
                            }
                        });
                    });
                    vbox.add_controller(gesture);
                }

                list.append(&row);
            }
        })
    };

    (header_ops_btn, update_ops)
}
