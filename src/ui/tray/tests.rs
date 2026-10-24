//! Exercise the exported SNI and D-Bus menu on a private real bus.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;

use super::*;

struct Bus {
    child: Child,
    address: String,
}
impl Bus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().into(),
        }
    }
    async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Watcher {
    registrations: Arc<Mutex<Vec<String>>>,
    host: bool,
    refuse: bool,
}
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: &str) -> fdo::Result<()> {
        if self.refuse {
            return Err(fdo::Error::Failed("Registration refused".into()));
        }
        self.registrations.lock().unwrap().push(service.into());
        Ok(())
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        self.host
    }
}

async fn watcher(bus: &Bus, host: bool, refuse: bool) -> (Connection, Arc<Mutex<Vec<String>>>) {
    let connection = bus.connect().await;
    let registrations = Arc::new(Mutex::new(Vec::new()));
    connection
        .object_server()
        .at(
            "/StatusNotifierWatcher",
            Watcher {
                registrations: registrations.clone(),
                host,
                refuse,
            },
        )
        .await
        .unwrap();
    connection.request_name(WATCHER).await.unwrap();
    (connection, registrations)
}

async fn proxy<'a>(
    connection: &'a Connection,
    destination: &'a str,
    path: &'a str,
    interface: &'a str,
) -> zbus::Proxy<'a> {
    zbus::proxy::Builder::new(connection)
        .destination(destination)
        .path(path)
        .interface(interface)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[test]
fn wire_signatures_match_sni_and_dbusmenu() {
    assert_eq!(Layout::SIGNATURE.to_string(), "(ia{sv}av)");
    assert_eq!(ToolTip::SIGNATURE.to_string(), "(sa(iiay)ss)");
    assert_eq!(Vec::<MenuEvent>::SIGNATURE.to_string(), "a(isvu)");
}

#[tokio::test]
async fn missing_watcher_host_and_refused_registration_fail_explicitly() {
    let bus = Bus::new();
    let (commands, _) = async_channel::bounded(8);
    assert!(
        register(bus.connect().await, commands.clone(), false)
            .await
            .is_err()
    );
    let (connection, _) = watcher(&bus, false, false).await;
    assert!(
        register(bus.connect().await, commands.clone(), false)
            .await
            .is_err()
    );
    connection.close().await.unwrap();
    let (_watcher, _) = watcher(&bus, true, true).await;
    assert!(
        register(bus.connect().await, commands, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn native_and_sandbox_registration_export_compatible_interfaces() {
    for sandboxed in [false, true] {
        let bus = Bus::new();
        let (_watcher, registrations) = watcher(&bus, true, false).await;
        let (commands, receiver) = async_channel::bounded(8);
        let handle = register(bus.connect().await, commands, sandboxed)
            .await
            .unwrap();
        let service = registrations.lock().unwrap()[0].clone();
        if sandboxed {
            assert!(service.starts_with(':'));
        } else {
            assert!(service.starts_with("org.kde.StatusNotifierItem-"));
        }
        let observer = bus.connect().await;
        let item = proxy(&observer, &service, ITEM_PATH, "org.kde.StatusNotifierItem").await;
        assert_eq!(item.get_property::<String>("Id").await.unwrap(), APP_ID);
        assert_eq!(
            item.get_property::<String>("Title").await.unwrap(),
            APP_NAME
        );
        assert_eq!(
            item.get_property::<String>("Status").await.unwrap(),
            "Active"
        );
        assert_eq!(
            item.get_property::<String>("Category").await.unwrap(),
            "ApplicationStatus"
        );
        assert_eq!(
            item.get_property::<String>("IconName").await.unwrap(),
            format!("{APP_ID}-symbolic")
        );
        assert!(!item.get_property::<bool>("ItemIsMenu").await.unwrap());
        assert_eq!(
            item.get_property::<zbus::OwnedObjectPath>("Menu")
                .await
                .unwrap()
                .as_str(),
            MENU_PATH
        );
        assert_eq!(
            item.get_property::<Pixmaps>("IconPixmap").await.unwrap(),
            [] as [(i32, i32, std::vec::Vec<u8>); 0]
        );
        for property in ["OverlayIconPixmap", "AttentionIconPixmap"] {
            assert_eq!(
                item.get_property::<Pixmaps>(property).await.unwrap(),
                [] as [(i32, i32, std::vec::Vec<u8>); 0]
            );
        }
        for property in [
            "IconThemePath",
            "OverlayIconName",
            "AttentionIconName",
            "AttentionMovieName",
        ] {
            assert_eq!(item.get_property::<String>(property).await.unwrap(), "");
        }
        assert_eq!(item.get_property::<i32>("WindowId").await.unwrap(), 0);
        let mut changed = item.receive_signal("NewToolTip").await.unwrap();
        update_counts(&handle, 4, 2).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), changed.next())
            .await
            .unwrap()
            .unwrap();
        let tooltip: ToolTip = item.get_property("ToolTip").await.unwrap();
        assert_eq!(tooltip.2, APP_NAME);
        assert!(tooltip.3.contains('4') && tooltip.3.contains('2'));
        item.call::<_, _, ()>("Activate", &(0_i32, 0_i32))
            .await
            .unwrap();
        assert_eq!(receiver.recv().await.unwrap(), TrayCommand::Show);
        item.call::<_, _, ()>("SecondaryActivate", &(0_i32, 0_i32))
            .await
            .unwrap();
        assert!(receiver.try_recv().is_err());
        assert!(
            item.call::<_, _, ()>("ContextMenu", &(0_i32, 0_i32))
                .await
                .is_err()
        );
        item.call::<_, _, ()>("Scroll", &(1_i32, "vertical"))
            .await
            .unwrap();
        item.call::<_, _, ()>("Scroll", &(1_i32, "horizontal"))
            .await
            .unwrap();
        assert!(
            item.call::<_, _, ()>("Scroll", &(1_i32, "bad"))
                .await
                .is_err()
        );
        handle.connection.close().await.unwrap();
        assert!(item.get_property::<String>("Status").await.is_err());
    }
}

#[tokio::test]
async fn menu_layout_filtering_actions_and_invalid_ids_round_trip() {
    let bus = Bus::new();
    let (_watcher, registrations) = watcher(&bus, true, false).await;
    let (commands, receiver) = async_channel::bounded(8);
    let handle = register(bus.connect().await, commands, true).await.unwrap();
    let service = registrations.lock().unwrap()[0].clone();
    let observer = bus.connect().await;
    let menu = proxy(&observer, &service, MENU_PATH, "com.canonical.dbusmenu").await;
    assert_eq!(menu.get_property::<u32>("Version").await.unwrap(), 3);
    assert_eq!(
        menu.get_property::<String>("TextDirection").await.unwrap(),
        "ltr"
    );
    assert_eq!(
        menu.get_property::<String>("Status").await.unwrap(),
        "normal"
    );
    assert_eq!(
        menu.get_property::<Vec<String>>("IconThemePath")
            .await
            .unwrap(),
        [] as [std::string::String; 0]
    );
    let (revision, layout): (u32, Layout) = menu
        .call("GetLayout", &(0_i32, -1_i32, Vec::<String>::new()))
        .await
        .unwrap();
    assert_eq!(revision, 1);
    assert_eq!(layout.children.len(), 3);
    let child: (i32, Properties, Vec<OwnedVariant>) = layout
        .children
        .into_iter()
        .next()
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(child.0, 1);
    assert_eq!(
        String::try_from(
            child
                .1
                .into_iter()
                .find(|(name, _)| name == "label")
                .unwrap()
                .1
        )
        .unwrap(),
        handle.state.show
    );
    let (_, shallow): (u32, Layout) = menu
        .call("GetLayout", &(0_i32, 0_i32, Vec::<String>::new()))
        .await
        .unwrap();
    assert_eq!(shallow.children, [] as [zbus::OwnedVariant; 0]);
    let (_, leaf): (u32, Layout) = menu
        .call("GetLayout", &(3_i32, -1_i32, vec!["label"]))
        .await
        .unwrap();
    assert_eq!(leaf.properties.len(), 1);
    assert_eq!(leaf.children, [] as [zbus::OwnedVariant; 0]);
    assert!(
        menu.call::<_, _, (u32, Layout)>("GetLayout", &(99_i32, -1_i32, Vec::<String>::new()))
            .await
            .is_err()
    );
    let all: Vec<(i32, Properties)> = menu
        .call(
            "GetGroupProperties",
            &(Vec::<i32>::new(), Vec::<String>::new()),
        )
        .await
        .unwrap();
    assert_eq!(all.len(), 4);
    let selected: Vec<(i32, Properties)> = menu
        .call("GetGroupProperties", &(vec![1_i32, 99], vec!["label"]))
        .await
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].1.len(), 1);
    let separator: OwnedVariant = menu.call("GetProperty", &(2_i32, "type")).await.unwrap();
    assert_eq!(String::try_from(separator).unwrap(), "separator");
    for (id, property) in [(99_i32, "label"), (1, "unknown")] {
        assert!(
            menu.call::<_, _, OwnedVariant>("GetProperty", &(id, property))
                .await
                .is_err()
        );
    }
    for id in [0_i32, 1, 2, 3] {
        assert!(
            !menu
                .call::<_, _, bool>("AboutToShow", &(id,))
                .await
                .unwrap()
        );
    }
    assert!(
        menu.call::<_, _, bool>("AboutToShow", &(99_i32,))
            .await
            .is_err()
    );
    let (updates, errors): (Vec<i32>, Vec<i32>) = menu
        .call("AboutToShowGroup", &(vec![0_i32, 99],))
        .await
        .unwrap();
    assert_eq!(updates, [] as [i32; 0]);
    assert_eq!(errors, vec![99]);
    menu.call::<_, _, ()>(
        "Event",
        &(1_i32, "hovered", OwnedVariant::from(0_i32), 0_u32),
    )
    .await
    .unwrap();
    assert!(receiver.try_recv().is_err());
    let errors: Vec<i32> = menu
        .call(
            "EventGroup",
            &(vec![
                (1_i32, "clicked", OwnedVariant::from(0_i32), 0_u32),
                (99, "clicked", OwnedVariant::from(0_i32), 0_u32),
                (3, "clicked", OwnedVariant::from(0_i32), 0_u32),
            ],),
        )
        .await
        .unwrap();
    assert_eq!(errors, vec![99]);
    assert_eq!(receiver.recv().await.unwrap(), TrayCommand::Show);
    assert_eq!(receiver.recv().await.unwrap(), TrayCommand::Quit);
    drop(receiver);
    menu.call::<_, _, ()>(
        "Event",
        &(1_i32, "clicked", OwnedVariant::from(0_i32), 0_u32),
    )
    .await
    .unwrap();
    handle.connection.close().await.unwrap();
}

#[tokio::test]
async fn watcher_restart_registers_the_existing_item_again() {
    let bus = Bus::new();
    let (first, registrations) = watcher(&bus, true, false).await;
    let (commands, _receiver) = async_channel::bounded(8);
    let handle = register(bus.connect().await, commands, false)
        .await
        .unwrap();
    let original = registrations.lock().unwrap()[0].clone();
    first.close().await.unwrap();
    let (_replacement, registrations) = watcher(&bus, true, false).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !registrations.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*registrations.lock().unwrap(), [original]);
    handle.connection.close().await.unwrap();
}
