use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use glib::subclass::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use pastor_store::{ActiveTransactionEvent, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxPhase {
    Active = 0,
    Success = 1,
    Failed = 2,
    Cancelled = 3,
}

impl From<u32> for TxPhase {
    fn from(v: u32) -> Self {
        match v {
            1 => TxPhase::Success,
            2 => TxPhase::Failed,
            3 => TxPhase::Cancelled,
            _ => TxPhase::Active,
        }
    }
}

#[derive(Default, glib::Properties)]
#[properties(wrapper_type = super::TxObject)]
pub struct TxObjectImp {
    #[property(get, set)]
    key: RefCell<String>,
    #[property(get, set)]
    pkg_clean: RefCell<String>,
    #[property(get, set)]
    name: RefCell<String>,
    #[property(get, set)]
    log: RefCell<String>,
    #[property(get, set)]
    fraction: RefCell<f32>,
    #[property(get, set)]
    phase: RefCell<u32>,
}

#[glib::object_subclass]
impl ObjectSubclass for TxObjectImp {
    const NAME: &'static str = "PastorTxObject";
    type Type = TxObject;
}

impl ObjectImpl for TxObjectImp {
    // Delegate to the Properties derive — without this the generated
    // properties are never registered and `Object::builder().property(...)`
    // panics with "Can't find property".
    fn properties() -> &'static [glib::ParamSpec] {
        Self::derived_properties()
    }

    fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        self.derived_set_property(id, value, pspec);
    }

    fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        self.derived_property(id, pspec)
    }
}

glib::wrapper! {
    pub struct TxObject(ObjectSubclass<TxObjectImp>);
}

impl TxObject {
    fn new(key: String, pkg_clean: String, name: String, log: String) -> Self {
        glib::Object::builder()
            .property("key", key)
            .property("pkg-clean", pkg_clean)
            .property("name", name)
            .property("log", log)
            .property("fraction", 0.0_f32)
            .property("phase", TxPhase::Active as u32)
            .build()
    }

    pub fn current_phase(&self) -> TxPhase {
        TxPhase::from(self.property::<u32>("phase"))
    }

    pub fn set_phase_to(&self, phase: TxPhase) {
        self.set_property("phase", phase as u32);
    }
}

#[derive(Default)]
pub struct TxMonitorImp {
    list: RefCell<Option<gtk4::gio::ListStore>>,
    by_key: RefCell<HashMap<String, TxObject>>,
}

#[glib::object_subclass]
impl ObjectSubclass for TxMonitorImp {
    const NAME: &'static str = "PastorTxMonitor";
    type Type = TxMonitor;
}

impl ObjectImpl for TxMonitorImp {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("tx-started")
                    .param_types([glib::GString::static_type()])
                    .build(),
                glib::subclass::Signal::builder("tx-progress")
                    .param_types([glib::GString::static_type()])
                    .build(),
                glib::subclass::Signal::builder("tx-terminal")
                    .param_types([glib::GString::static_type(), u32::static_type()])
                    .build(),
            ]
        })
    }
}

glib::wrapper! {
    pub struct TxMonitor(ObjectSubclass<TxMonitorImp>);
}

impl TxMonitor {
    /// Create the monitor and attach the single broadcast subscriber.
    /// This is the only place in the UI that consumes `subscribe_transactions()`;
    /// `Lagged` is handled centrally so individual rows can never freeze.
    pub fn new(store: &Store) -> TxMonitor {
        let monitor: TxMonitor = glib::Object::new();
        *monitor.imp().list.borrow_mut() = Some(gtk4::gio::ListStore::new::<TxObject>());

        let mut sub = store.subscribe_transactions();
        let monitor_weak = monitor.downgrade();
        glib::spawn_future_local(async move {
            loop {
                match sub.recv().await {
                    Ok(evt) => {
                        if let Some(m) = monitor_weak.upgrade() {
                            m.handle_event(evt);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Transaction monitor skipped {n} events (receiver lagged)");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        monitor
    }

    pub fn list(&self) -> gtk4::gio::ListStore {
        self.imp()
            .list
            .borrow()
            .as_ref()
            .expect("TxMonitor list not initialized")
            .clone()
    }

    pub fn tx_for_key(&self, key: &str) -> Option<TxObject> {
        self.imp().by_key.borrow().get(key).cloned()
    }

    /// Loose match used by rows: exact key first, then cleaned package name.
    pub fn tx_for_package(&self, pkg_key: &str, pkg_clean: &str) -> Option<TxObject> {
        let map = self.imp().by_key.borrow();
        if let Some(tx) = map.get(pkg_key) {
            return Some(tx.clone());
        }
        map.values()
            .find(|tx| tx.property::<String>("pkg-clean") == pkg_clean)
            .cloned()
    }

    pub fn active_count(&self) -> usize {
        self.imp()
            .by_key
            .borrow()
            .values()
            .filter(|tx| tx.current_phase() == TxPhase::Active)
            .count()
    }

    pub fn connect_started<F: Fn(&str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("tx-started", false, move |vals| {
            let key = vals[1].get::<glib::GString>().unwrap();
            f(&key);
            None
        })
    }

    pub fn connect_progress<F: Fn(&str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("tx-progress", false, move |vals| {
            let key = vals[1].get::<glib::GString>().unwrap();
            f(&key);
            None
        })
    }

    pub fn connect_terminal<F: Fn(&str, TxPhase) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("tx-terminal", false, move |vals| {
            let key = vals[1].get::<glib::GString>().unwrap();
            let phase = TxPhase::from(vals[2].get::<u32>().unwrap());
            f(&key, phase);
            None
        })
    }

    fn handle_event(&self, evt: ActiveTransactionEvent) {
        match evt {
            ActiveTransactionEvent::Started(info) => {
                let key = info.package_id.event_key();
                let clean = info.package_id.clean_name();
                let existing = self.imp().by_key.borrow().get(&key).cloned();
                let tx = match existing {
                    Some(tx) => {
                        tx.set_property("fraction", 0.0_f32);
                        tx.set_property("log", info.log_message.clone());
                        tx.set_phase_to(TxPhase::Active);
                        tx
                    }
                    None => {
                        let tx = TxObject::new(key.clone(), clean, info.package_name.clone(), info.log_message.clone());
                        self.imp().by_key.borrow_mut().insert(key.clone(), tx.clone());
                        self.list().append(&tx);
                        tx
                    }
                };
                let _ = tx;
                self.emit_by_name::<()>("tx-started", &[&key.as_str()]);
            }
            ActiveTransactionEvent::Progress(info) => {
                let key = info.package_id.event_key();
                if let Some(tx) = self.imp().by_key.borrow().get(&key).cloned() {
                    tx.set_property("fraction", info.progress_fraction);
                    tx.set_property("log", info.log_message.clone());
                }
                self.emit_by_name::<()>("tx-progress", &[&key.as_str()]);
            }
            ActiveTransactionEvent::Completed(id) => {
                self.terminal(&id, TxPhase::Success);
            }
            ActiveTransactionEvent::Failed(id, msg) => {
                let tx = self.terminal(&id, TxPhase::Failed);
                if let Some(tx) = tx {
                    tx.set_property("log", msg);
                }
            }
            ActiveTransactionEvent::Cancelled(id) => {
                self.terminal(&id, TxPhase::Cancelled);
            }
        }
    }

    fn terminal(&self, id: &pastor_core::PackageId, phase: TxPhase) -> Option<TxObject> {
        let key = id.event_key();
        let tx = self.imp().by_key.borrow().get(&key).cloned();
        if let Some(tx) = &tx {
            tx.set_property("fraction", 1.0_f32);
            tx.set_phase_to(phase);
        }
        self.emit_by_name::<()>("tx-terminal", &[&key.as_str(), &(phase as u32)]);
        tx
    }
}
