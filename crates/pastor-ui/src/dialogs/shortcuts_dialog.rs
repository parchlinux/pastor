use adw::prelude::*;

const SHORTCUTS_UI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkShortcutsWindow" id="shortcuts_window">
    <property name="modal">1</property>
    <child>
      <object class="GtkShortcutsSection">
        <property name="section-name">shortcuts</property>
        <property name="max-height">12</property>
        <child>
          <object class="GtkShortcutsGroup">
            <property name="title">Navigation &amp; Search</property>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Search packages and applications</property>
                <property name="accelerator">&lt;Primary&gt;f &lt;Primary&gt;k</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Toggle sidebar</property>
                <property name="accelerator">F9 &lt;Primary&gt;b</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Go back to previous page</property>
                <property name="accelerator">&lt;Alt&gt;Left &lt;Alt&gt;BackSpace</property>
              </object>
            </child>
          </object>
        </child>
        <child>
          <object class="GtkShortcutsGroup">
            <property name="title">General</property>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Show keyboard shortcuts</property>
                <property name="accelerator">&lt;Primary&gt;question &lt;Primary&gt;slash</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Close window</property>
                <property name="accelerator">&lt;Primary&gt;q &lt;Primary&gt;w</property>
              </object>
            </child>
          </object>
        </child>
      </object>
    </child>
  </object>
</interface>
"#;

pub fn show_shortcuts_dialog(parent: &impl IsA<gtk4::Widget>) {
    let builder = gtk4::Builder::from_string(SHORTCUTS_UI);
    if let Some(dialog) = builder.object::<gtk4::ShortcutsWindow>("shortcuts_window") {
        if let Some(root) = parent.root() {
            if let Ok(w) = root.downcast::<gtk4::Window>() {
                dialog.set_transient_for(Some(&w));
            }
        }
        dialog.present();
    }
}
