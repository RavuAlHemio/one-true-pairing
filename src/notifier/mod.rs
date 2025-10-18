//! Implementation of the notifier (notification icon) interface.
//!
//! Derived from the specification at
//! https://github.com/KDE/kdelibs/blob/KDE/4.14/kdeui/notifications/org.kde.StatusNotifierItem.xml


pub(crate) mod proxies;


use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str, Type, Value};


const MENU_SEPARATOR_ID: i32 = 0x7FFF_FFFE;
const MENU_EXIT_ID: i32 = 0x7FFF_FFFF;


pub(crate) struct TrayIcon;

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl TrayIcon {
    #[zbus(property)]
    async fn category(&self) -> Result<ItemCategory, zbus::fdo::Error> {
        Ok(ItemCategory::ApplicationStatus)
    }

    #[zbus(property)]
    async fn id(&self) -> Result<String, zbus::fdo::Error> {
        Ok("com.ondrahosek.OneTruePairing.NotifyIcon".to_owned())
    }

    #[zbus(property)]
    async fn title(&self) -> Result<String, zbus::fdo::Error> {
        //Ok("One True Pairing".to_owned())
        Ok(String::with_capacity(0))
    }

    #[zbus(property)]
    async fn status(&self) -> Result<ItemStatus, zbus::fdo::Error> {
        Ok(ItemStatus::Active)
    }

    #[zbus(property)]
    async fn window_id(&self) -> Result<i32, zbus::fdo::Error> {
        Ok(0)
    }

    #[zbus(property)]
    async fn icon_theme_path(&self) -> Result<String, zbus::fdo::Error> {
        Ok(String::with_capacity(0))
    }

    #[zbus(property)]
    async fn menu(&self) -> Result<OwnedObjectPath, zbus::fdo::Error> {
        Ok(crate::MENU_BUS_PATH.try_into().unwrap())
    }

    #[zbus(property)]
    async fn item_is_menu(&self) -> Result<bool, zbus::fdo::Error> {
        Ok(true)
    }

    #[zbus(property)]
    async fn icon_name(&self) -> Result<String, zbus::fdo::Error> {
        Ok(String::with_capacity(0))
    }

    #[zbus(property)]
    async fn icon_pixmap(&self) -> Result<Vec<Image>, zbus::fdo::Error> {
        const ICON_WIDTH: usize = 32;
        const ICON_HEIGHT: usize = 32;

        let mut image_data = Vec::with_capacity(ICON_WIDTH * ICON_HEIGHT * 4);
        for _b in 0..ICON_WIDTH*ICON_HEIGHT {
            image_data.push(0xFF);
            image_data.push(0x00);
            /*
            image_data.push(((b >> 16) & 0xFF) as u8);
            image_data.push(((b >>  8) & 0xFF) as u8);
            image_data.push(((b >>  0) & 0xFF) as u8);
            */
            image_data.push(0xFF);
            image_data.push(0x00);
        }
        let v = Image {
            width: ICON_WIDTH.try_into().unwrap(),
            height: ICON_HEIGHT.try_into().unwrap(),
            data: image_data,
        };
        Ok(vec![v])
    }

    #[zbus(property)]
    async fn overlay_icon_name(&self) -> Result<String, zbus::fdo::Error> {
        Ok(String::with_capacity(0))
    }

    #[zbus(property)]
    async fn overlay_icon_pixmap(&self) -> Result<Vec<Image>, zbus::fdo::Error> {
        Ok(Vec::with_capacity(0))
    }

    #[zbus(property)]
    async fn attention_icon_name(&self) -> Result<String, zbus::fdo::Error> {
        Ok(String::with_capacity(0))
    }

    #[zbus(property)]
    async fn attention_icon_pixmap(&self) -> Result<Vec<Image>, zbus::fdo::Error> {
        Ok(Vec::with_capacity(0))
    }

    #[zbus(property)]
    async fn attention_movie_name(&self) -> Result<String, zbus::fdo::Error> {
        Ok(String::with_capacity(0))
    }

    #[zbus(property)]
    async fn tool_tip(&self) -> Result<ToolTip, zbus::fdo::Error> {
        Ok(ToolTip {
            icon: String::with_capacity(0),
            images: Vec::with_capacity(0),
            title: "One True Pairing".to_owned(),
            sub_title: String::with_capacity(0),
        })
    }

    async fn context_menu(&self, x: i32, y: i32) -> Result<(), zbus::fdo::Error> {
        eprintln!("WARNING: context menu triggered when the notification icon tray should show our D-Bus-published menu instead -- is your notification tray lacking a menu implementation?");
        Ok(())
    }

    async fn activate(&self, x: i32, y: i32) -> Result<(), zbus::fdo::Error> {
        // this shouldn't happen because we declared ourselves a menu
        eprintln!("activated when the notification icon tray should show our D-Bus-published menu instead -- is your notification tray lacking a menu implementation?");
        Ok(())
    }

    async fn secondary_activate(&self, x: i32, y: i32) -> Result<(), zbus::fdo::Error> {
        // ignore
        Ok(())
    }

    async fn scroll(&self, delta: i32, orientation: String) -> Result<(), zbus::fdo::Error> {
        // ignore
        Ok(())
    }

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    async fn new_icon<'e>(emitter: &SignalEmitter<'e>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    async fn new_attention_icon<'e>(emitter: &SignalEmitter<'e>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    async fn new_overlay_icon<'e>(emitter: &SignalEmitter<'e>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    async fn new_tool_tip<'e>(emitter: &SignalEmitter<'e>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    async fn new_status<'e>(emitter: &SignalEmitter<'e>, status: ItemStatus) -> Result<(), zbus::Error>;
}

pub(crate) struct ContextMenu;

impl ContextMenu {
    fn obtain_layout_structure(&self, property_names: &[String]) -> MenuLayout {
        fn want(property_names: &[String], key: &str) -> bool {
            property_names.is_empty() || property_names.iter().any(|pn| pn == key)
        }

        let mut separator_props = HashMap::new();
        if want(&property_names, "type") {
            separator_props.insert(
                "type".to_owned(),
                Str::from("separator").into(),
            );
        }

        let menu_entries: Vec<OwnedValue> = vec![
            MenuLayout {
                id: MENU_SEPARATOR_ID,
                properties: separator_props.clone(),
                children: Vec::with_capacity(0),
            }.try_into().unwrap(),
            MenuLayout {
                id: MENU_EXIT_ID,
                properties: {
                    let mut props = HashMap::new();
                    if want(&property_names, "type") {
                        props.insert(
                            "type".to_owned(),
                            Str::from("standard").into(),
                        );
                    }
                    if want(&property_names, "label") {
                        props.insert(
                            "label".to_owned(),
                            Str::from("E_xit").into(),
                        );
                    }
                    props
                },
                children: Vec::with_capacity(0),
            }.try_into().unwrap(),
        ];

        MenuLayout {
            id: 0,
            properties: HashMap::new(),
            children: menu_entries,
        }
    }

    fn flatten_entries(top_layout: &MenuLayout, collection: &mut Vec<MenuLayout>) {
        collection.push(top_layout.clone());
        for raw_child in &top_layout.children {
            let child = MenuLayout::try_from(raw_child.clone())
                .expect("MenuLayout child is not MenuLayout");
            Self::flatten_entries(&child, collection);
        }
    }

    fn obtain_group_properties(&self, ids: &[i32], property_names: &[String]) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let layout = self.obtain_layout_structure(&property_names);
        let mut entries = Vec::new();
        Self::flatten_entries(&layout, &mut entries);

        let mut ret = Vec::new();
        for entry in entries {
            if ids.iter().any(|i| *i == entry.id) {
                // interesting entry

                let mut props = HashMap::new();
                for (k, v) in &entry.properties {
                    if property_names.is_empty() || property_names.iter().any(|pn| pn == k) {
                        // interesting property
                        props.insert(k.clone(), v.clone());
                    }
                }

                ret.push((entry.id, props));
            }
        }

        ret
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl ContextMenu {
    #[zbus(property)]
    async fn version(&self) -> Result<u32, zbus::fdo::Error> {
        Ok(0)
    }

    #[zbus(property)]
    async fn status(&self) -> Result<MenuStatus, zbus::fdo::Error> {
        Ok(MenuStatus::Normal)
    }

    async fn get_layout(&self, parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> Result<(u32, MenuLayout), zbus::fdo::Error> {
        if parent_id != 0 {
            // return an empty menu
            return Ok((
                0,
                MenuLayout::default(),
            ));
        }

        let layout = self.obtain_layout_structure(&property_names);
        Ok((0, layout))
    }

    async fn get_group_properties(&self, ids: Vec<i32>, property_names: Vec<String>) -> Result<Vec<(i32, HashMap<String, OwnedValue>)>, zbus::fdo::Error> {
        let props = self.obtain_group_properties(&ids, &property_names);
        Ok(props)
    }

    /// Get a single property on a single item.
    ///
    /// This is not useful if you're going to implement this interface, it should only be used if
    /// you're debugging via a commandline tool.
    async fn get_property(&self, id: i32, name: String) -> Result<OwnedValue, zbus::fdo::Error> {
        let objs_props = self.obtain_group_properties(&[id], &[name.clone()]);
        for (id, props) in objs_props {
            for v in props.values() {
                return Ok(v.clone());
            }

            // we found the object but not the property
            return Err(zbus::fdo::Error::UnknownProperty(format!("property {:?} not found on menu item {}", name, id)));
        }

        // we did not find the object
        return Err(zbus::fdo::Error::UnknownObject(format!("menu item {} not found", id)));
    }

    /// This is called by the applet to notify the application an event happened on a menu item.
    async fn event(&self, id: i32, event_id: MenuEvent, data: OwnedValue, timestamp: u32) -> Result<(), zbus::fdo::Error> {
        if event_id != MenuEvent::Clicked {
            return Ok(());
        }

        match id {
            MENU_SEPARATOR_ID => {
                eprintln!("how the heck did you click a separator?!");
            },
            MENU_EXIT_ID => {
                // the fun is over; trigger the stopper
                crate::STOPPER.wait();
                eprintln!("stopper triggered");
            },
            _ => {
                // TODO: find entry by index
                // TODO: generate OTP code
                // TODO: provide code via clipboard
            },
        }

        Ok(())
    }

    /// This is called by the applet to notify the application that it is about to show the menu under the specified
    /// item.
    ///
    /// The return value indicates if the menu should be updated first.
    async fn about_to_show(&self, id: i32) -> Result<bool, zbus::fdo::Error> {
        Ok(false)
    }

    /// Triggered when there are lots of property updates across many items so they all get grouped
    /// into a single dbus message.
    ///
    /// The format is the ID of the item with a hashtable of names and values for those properties.
    #[zbus(signal)]
    async fn items_properties_updated(emitter: &SignalEmitter<'_>, updated_props: Vec<(i32, HashMap<String, OwnedValue>)>, removed_props: Vec<(i32, Vec<String>)>) -> Result<(), zbus::Error>;

    /// Triggered by the application to notify display of a layout update, up to revision
    #[zbus(signal)]
    async fn layout_updated(emitter: &SignalEmitter<'_>, revision: u32, parent: i32) -> Result<(), zbus::Error>;

    /// The server is requesting that all clients displaying this menu open it to the user.
    ///
    /// This would be for things like hotkeys that when the user presses them the menu should open
    /// and display itself to the user.
    #[zbus(signal)]
    async fn item_activation_requested(emitter: &SignalEmitter<'_>, id: i32, timestamp: u32) -> Result<(), zbus::Error>;
}


#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, OwnedValue, PartialEq, PartialOrd, Serialize, Type, Value)]
#[zvariant(signature = "s")]
pub enum ItemStatus {
    Passive = 1,
    Active = 2,
    NeedsAttention = 3,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, OwnedValue, PartialEq, PartialOrd, Serialize, Type, Value)]
#[zvariant(signature = "s")]
pub enum ItemCategory {
    ApplicationStatus = 1,
    Communications = 2,
    SystemServices = 3,
    Hardware = 4,
    Reserved = 129,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, OwnedValue, PartialEq, PartialOrd, Serialize, Type, Value)]
#[zvariant(signature = "s")]
#[serde(rename_all = "kebab-case")]
pub enum MenuStatus {
    Normal,
    Notice,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, OwnedValue, PartialEq, PartialOrd, Type, Value)]
pub struct Image {
    pub width: i32,
    pub height: i32,

    /// Big-endian ARGB32 (0xAARRGGBB) pixels.
    ///
    /// `data.len() == width * height * sizeof::<u32>()`
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, OwnedValue, PartialEq, PartialOrd, Type, Value)]
pub struct ToolTip {
    pub icon: String,
    pub images: Vec<Image>,
    pub title: String,
    pub sub_title: String,
}

#[derive(Clone, Debug, Default, Deserialize, OwnedValue, PartialEq, Serialize, Type, Value)]
pub struct MenuLayout {
    pub id: i32,
    pub properties: HashMap<String, OwnedValue>,
    pub children: Vec<OwnedValue>, // mostly recursively MenuLayout
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, OwnedValue, PartialEq, PartialOrd, Serialize, Type, Value)]
#[zvariant(signature = "s")]
#[serde(rename_all = "kebab-case")]
pub enum MenuEvent {
    Clicked,
    Hovered,
}
