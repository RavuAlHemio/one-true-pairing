//! Implementation of the notifier (notification icon) interface.
//!
//! Derived from the specification at
//! https://github.com/KDE/kdelibs/blob/KDE/4.14/kdeui/notifications/org.kde.StatusNotifierItem.xml


pub(crate) mod proxies;


use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str, Type, Value};


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
        Ok("/MenuBar".try_into().unwrap())
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
        println!("CONTEXT MENU x={x} y={y}");
        Ok(())
    }

    async fn activate(&self, x: i32, y: i32) -> Result<(), zbus::fdo::Error> {
        println!("ACTIVATE x={x} y={y}");
        Ok(())
    }

    async fn secondary_activate(&self, x: i32, y: i32) -> Result<(), zbus::fdo::Error> {
        println!("SECONDARY ACTIVATE x={x} y={y}");
        Ok(())
    }

    async fn scroll(&self, delta: i32, orientation: String) -> Result<(), zbus::fdo::Error> {
        println!("SCROLL delta={delta} orientation={orientation:?}");
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
    fn obtain_layout_structure(&self, property_names: Vec<String>) -> MenuLayout {
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
                id: 0x7FFF_FFFE,
                properties: separator_props.clone(),
                children: Vec::with_capacity(0),
            }.try_into().unwrap(),
            MenuLayout {
                id: 0x7FFF_FFFF,
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
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl ContextMenu {
    #[zbus(property)]
    async fn version(&self) -> Result<u32, zbus::Error> {
        Ok(0)
    }

    #[zbus(property)]
    async fn status(&self) -> Result<MenuStatus, zbus::Error> {
        Ok(MenuStatus::Normal)
    }

    fn get_layout(&self, parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> Result<(u32, MenuLayout), zbus::fdo::Error> {
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

    fn get_group_properties(&self, ids: Vec<i32>, property_names: Vec<String>) -> Result<Vec<(i32, HashMap<String, OwnedValue>)>, zbus::fdo::Error> {
        let layout = self.obtain_layout_structure(&property_names);

        todo!("recursively flatten structure and extract the interesting IDs");
    }

    /// Get a signal property on a single item.
    ///
    /// This is not useful if you're going to implement this interface, it should only be used if
    /// you're debugging via a commandline tool.
    fn get_property(&self, id: i32, name: String) -> Result<OwnedValue, zbus::fdo::Error>;

    /// This is called by the applet to notify the application an event happened on a menu item.
    fn event(&self, id: i32, event_id: String, data: OwnedValue, timestamp: u32) -> Result<(), zbus::fdo::Error>;

    /// This is called by the applet to notify the application that it is about to show the menu under the specified
    /// item.
    ///
    /// The return value indicates if the menu should be updated first.
    fn about_to_show(&self, id: i32) -> Result<bool, zbus::fdo::Error>;

    /// Triggered when there are lots of property updates across many items so they all get grouped
    /// into a single dbus message.
    ///
    /// The format is the ID of the item with a hashtable of names and values for those properties.
    #[zbus(signal)]
    fn items_properties_updated(&self, updated_props: Vec<(i32, HashMap<String, OwnedValue>)>, removed_props: Vec<(i32, Vec<String>)>) -> Result<(), zbus::Error>;

    /// Triggered by the application to notify display of a layout update, up to revision
    #[zbus(signal)]
    fn layout_updated(&self, revision: u32, parent: i32) -> Result<(), zbus::Error>;

    /// The server is requesting that all clients displaying this menu open it to the user.
    ///
    /// This would be for things like hotkeys that when the user presses them the menu should open
    /// and display itself to the user.
    #[zbus(signal)]
    fn item_activation_requested(&self, id: i32, timestamp: u32) -> Result<(), zbus::Error>;
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
