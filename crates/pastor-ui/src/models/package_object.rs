use std::cell::RefCell;
use std::sync::OnceLock;

use glib::prelude::*;
use glib::subclass::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageState};

#[derive(Default)]
pub struct PackageObjectImp {
    package: RefCell<Option<Package>>,
}

#[glib::object_subclass]
impl ObjectSubclass for PackageObjectImp {
    const NAME: &'static str = "PastorPackageObject";
    type Type = PackageObject;
}

impl ObjectImpl for PackageObjectImp {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("changed").build()])
    }
}

glib::wrapper! {
    pub struct PackageObject(ObjectSubclass<PackageObjectImp>);
}

impl PackageObject {
    pub fn new(pkg: Package) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().package.borrow_mut() = Some(pkg);
        obj
    }

    pub fn package(&self) -> Package {
        self.imp()
            .package
            .borrow()
            .clone()
            .expect("PackageObject without package data")
    }

    pub fn id_key(&self) -> String {
        self.imp()
            .package
            .borrow()
            .as_ref()
            .map(|p| p.id.event_key())
            .unwrap_or_default()
    }

    pub fn state(&self) -> Option<PackageState> {
        self.imp().package.borrow().as_ref().map(|p| p.state)
    }

    /// Replace the stored package (e.g. after a terminal transaction event refreshed
    /// it from the store) and notify bound rows to re-render.
    pub fn update(&self, pkg: Package) {
        *self.imp().package.borrow_mut() = Some(pkg);
        self.emit_by_name::<()>("changed", &[]);
    }

    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("changed", false, move |vals| {
            let obj = vals[0].get::<Self>().unwrap();
            f(&obj);
            None
        })
    }
}
