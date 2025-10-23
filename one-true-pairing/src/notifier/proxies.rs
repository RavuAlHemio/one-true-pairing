//! Proxies for the notifier (notification icon) interface.
//!
//! Derived from the specifications at
//! https://github.com/KDE/kdelibs/blob/KDE/4.14/kdeui/notifications/org.kde.StatusNotifierWatcher.xml
//! and
//! https://github.com/KDE/kdelibs/blob/KDE/4.14/kdeui/notifications/org.kde.StatusNotifierItem.xml


use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::notifier::{Image, ItemCategory, ItemStatus, MenuLayout, MenuStatus, ToolTip};


#[zbus::proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher",
)]
pub trait StatusNotifierWatcher {
    fn register_status_notifier_item(&self, service: String) -> Result<(), zbus::Error>;
    fn register_status_notifier_host(&self, service: String) -> Result<(), zbus::Error>;

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Result<Vec<String>, zbus::Error>;

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> Result<bool, zbus::Error>;

    #[zbus(property)]
    fn protocol_version(&self) -> Result<i32, zbus::Error>;

    #[zbus(signal)]
    fn status_notifier_item_registered(&self, item: String) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn status_notifier_item_unregistered(&self, item: String) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn status_notifier_host_registered(&self) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn status_notifier_host_unregistered(&self) -> Result<(), zbus::Error>;
}


#[zbus::proxy(interface = "org.kde.StatusNotifierItem")]
pub trait StatusNotifierItem {
    #[zbus(property)]
    fn category(&self) -> Result<ItemCategory, zbus::Error>;

    #[zbus(property)]
    fn id(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn title(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn status(&self) -> Result<ItemStatus, zbus::Error>;

    #[zbus(property)]
    fn window_id(&self) -> Result<i32, zbus::Error>;

    #[zbus(property)]
    fn icon_theme_path(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn menu(&self) -> Result<OwnedObjectPath, zbus::Error>;

    #[zbus(property)]
    fn item_is_menu(&self) -> Result<bool, zbus::Error>;

    #[zbus(property)]
    fn icon_name(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn icon_pixmap(&self) -> Result<Vec<Image>, zbus::Error>;

    #[zbus(property)]
    fn overlay_icon_name(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Result<Vec<Image>, zbus::Error>;

    #[zbus(property)]
    fn attention_icon_name(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Result<Vec<Image>, zbus::Error>;

    #[zbus(property)]
    fn attention_movie_name(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn tool_tip(&self) -> Result<ToolTip, zbus::Error>;

    fn context_menu(&self, x: i32, y: i32) -> Result<(), zbus::Error>;
    fn activate(&self, x: i32, y: i32) -> Result<(), zbus::Error>;
    fn secondary_activate(&self, x: i32, y: i32) -> Result<(), zbus::Error>;
    fn scroll(&self, delta: i32, orientation: String) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn new_title(&self) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn new_icon(&self) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn new_attention_icon(&self) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn new_overlay_icon(&self) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn new_tool_tip(&self) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn new_status(&self, status: ItemStatus) -> Result<(), zbus::Error>;
}


/// A DBus interface to expose menus on DBus.
///
/// Menu items are represented with a unique numeric id and a dictionary of properties.
#[zbus::proxy(interface = "com.canonical.dbusmenu")]
pub trait Menu {
    /// Provides the version of the DBusmenu API that this API is implementing.
    #[zbus(property)]
    fn version(&self) -> Result<u32, zbus::Error>;

    /// Tells if the menus are in a normal state or they believe that they could use some attention.
    ///
    /// Cases for showing them would be if help were referring to them or they accessors were being
    /// highlighted. This property can have two values: `Normal` in almost all cases and `Notice`
    /// when they should have a higher priority to be shown.
    #[zbus(property)]
    fn status(&self) -> Result<MenuStatus, zbus::Error>;

    /// Provides the layout and properties that are attached to the entries that are in the layout.
    ///
    /// It only gives the items that are children of the item that is specified in `parentId`. It
    /// will return all of the properties or specific ones depending of the value in
    /// `propertyNames`.
    ///
    /// The format is recursive, where the elements of `'children'` are `MenuLayout`s themselves.
    /// The maximum depth of these structures depends on `recursion_depth`.
    fn get_layout(&self, parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> Result<(u32, MenuLayout), zbus::fdo::Error>;

    fn get_group_properties(&self, ids: Vec<i32>, property_names: Vec<String>) -> Result<Vec<(i32, HashMap<String, OwnedValue>)>, zbus::fdo::Error>;

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
