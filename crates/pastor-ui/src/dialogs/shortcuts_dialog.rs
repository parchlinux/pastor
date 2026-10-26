use adw::prelude::*;
use gtk4::glib;

pub fn show_shortcuts_dialog(parent: &impl IsA<gtk4::Widget>) {
    let shortcuts_ui = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkShortcutsWindow" id="shortcuts_window">
    <property name="modal">1</property>
    <child>
      <object class="GtkShortcutsSection">
        <property name="section-name">shortcuts</property>
        <property name="max-height">12</property>
        <child>
          <object class="GtkShortcutsGroup">
            <property name="title">{}</property>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">{}</property>
                <property name="accelerator">&lt;Primary&gt;f &lt;Primary&gt;k</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">{}</property>
                <property name="accelerator">F9 &lt;Primary&gt;b</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">{}</property>
                <property name="accelerator">&lt;Alt&gt;Left &lt;Alt&gt;BackSpace</property>
              </object>
            </child>
          </object>
        </child>
        <child>
          <object class="GtkShortcutsGroup">
            <property name="title">{}</property>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">{}</property>
                <property name="accelerator">&lt;Primary&gt;question &lt;Primary&gt;slash</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">{}</property>
                <property name="accelerator">&lt;Primary&gt;q &lt;Primary&gt;w</property>
              </object>
            </child>
          </object>
        </child>
      </object>
    </child>
  </object>
</interface>
"#,
        glib::markup_escape_text(&pastor_core::tr("Navigation & Search")),
        glib::markup_escape_text(&pastor_core::tr("Search packages and applications")),
        glib::markup_escape_text(&pastor_core::tr("Toggle sidebar")),
        glib::markup_escape_text(&pastor_core::tr("Go back to previous page")),
        glib::markup_escape_text(&pastor_core::tr("General")),
        glib::markup_escape_text(&pastor_core::tr("Show keyboard shortcuts")),
        glib::markup_escape_text(&pastor_core::tr("Close window")),
    );

    let builder = gtk4::Builder::from_string(&shortcuts_ui);
    if let Some(dialog) = builder.object::<gtk4::ShortcutsWindow>("shortcuts_window") {
        if let Some(root) = parent.root() {
            if let Ok(w) = root.downcast::<gtk4::Window>() {
                dialog.set_transient_for(Some(&w));
            }
        }
        dialog.present();
    }
}
