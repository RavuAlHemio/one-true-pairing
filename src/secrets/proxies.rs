use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zbus::proxy;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Type, Value};


#[proxy(
    interface = "org.freedesktop.Secret.Service",
    default_service = "org.freedesktop.secrets",
    default_path = "/org/freedesktop/secrets",
)]
pub trait Service {
    #[zbus(property)]
    fn collections(&self) -> Result<Vec<OwnedObjectPath>, zbus::Error>;

    fn open_session(&self, algorithm: &str, input: &Value<'_>) -> Result<(OwnedValue, OwnedObjectPath), zbus::fdo::Error>;
    fn create_collection(&self, properties: &HashMap<String, OwnedValue>, alias: &str) -> Result<(OwnedObjectPath, OwnedObjectPath), zbus::fdo::Error>;
    fn search_items(&self, attributes: &HashMap<String, String>) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), zbus::fdo::Error>;
    fn unlock(&self, objects: &[ObjectPath<'_>]) -> Result<(Vec<OwnedObjectPath>, OwnedObjectPath), zbus::fdo::Error>;
    fn lock(&self, objects: &[ObjectPath<'_>]) -> Result<(Vec<OwnedObjectPath>, OwnedObjectPath), zbus::fdo::Error>;
    fn get_secrets(&self, items: &[ObjectPath<'_>], session: ObjectPath<'_>) -> Result<HashMap<OwnedObjectPath, Secret>, zbus::fdo::Error>;
    fn read_alias(&self, name: &str) -> Result<OwnedObjectPath, zbus::fdo::Error>;
    fn set_alias(&self, name: &str, collection: ObjectPath<'_>) -> Result<(), zbus::fdo::Error>;

    #[zbus(signal)]
    fn collection_created(&self, collection: ObjectPath<'_>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn collection_deleted(&self, collection: ObjectPath<'_>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn collection_changed(&self, collection: ObjectPath<'_>) -> Result<(), zbus::Error>;
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, OwnedValue, PartialEq, Serialize, Type, Value)]
pub struct Secret {
    pub session: OwnedObjectPath,
    pub parameters: Vec<u8>,
    pub value: Vec<u8>,
    pub content_type: String,
}

#[proxy(
    interface = "org.freedesktop.Secret.Collection",
    default_service = "org.freedesktop.secrets",
)]
pub trait Collection {
    #[zbus(property)]
    fn items(&self) -> Result<Vec<OwnedObjectPath>, zbus::Error>;

    #[zbus(property)]
    fn label(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn set_label(&self, label: &str) -> Result<(), zbus::Error>;

    #[zbus(property)]
    fn locked(&self) -> Result<bool, zbus::Error>;

    #[zbus(property)]
    fn created(&self) -> Result<u64, zbus::Error>;

    #[zbus(property)]
    fn modified(&self) -> Result<u64, zbus::Error>;

    fn delete(&self) -> Result<OwnedObjectPath, zbus::fdo::Error>;
    fn search_items(&self, attributes: &HashMap<String, String>) -> Result<Vec<OwnedObjectPath>, zbus::fdo::Error>;
    fn create_item(&self, properties: &HashMap<String, OwnedValue>, secret: Secret, replace: bool) -> Result<(OwnedObjectPath, OwnedObjectPath), zbus::fdo::Error>;

    #[zbus(signal)]
    fn item_created(&self, item: ObjectPath<'_>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn item_deleted(&self, item: ObjectPath<'_>) -> Result<(), zbus::Error>;

    #[zbus(signal)]
    fn item_changed(&self, item: ObjectPath<'_>) -> Result<(), zbus::Error>;
}

#[proxy(
    interface = "org.freedesktop.Secret.Item",
    default_service = "org.freedesktop.secrets",
)]
pub trait Item {
    #[zbus(property)]
    fn locked(&self) -> Result<bool, zbus::Error>;

    #[zbus(property)]
    fn attributes(&self) -> Result<HashMap<String, String>, zbus::Error>;

    #[zbus(property)]
    fn set_attributes(&self, attributes: HashMap<String, String>) -> Result<(), zbus::Error>;

    #[zbus(property)]
    fn label(&self) -> Result<String, zbus::Error>;

    #[zbus(property)]
    fn set_label(&self, label: &str) -> Result<(), zbus::Error>;

    #[zbus(property)]
    fn created(&self) -> Result<u64, zbus::Error>;

    #[zbus(property)]
    fn modified(&self) -> Result<u64, zbus::Error>;

    fn delete(&self) -> Result<OwnedObjectPath, zbus::fdo::Error>;
    fn get_secret(&self, session: ObjectPath<'_>) -> Result<Secret, zbus::fdo::Error>;
    fn set_secret(&self, secret: Secret) -> Result<(), zbus::fdo::Error>;
}

#[proxy(
    interface = "org.freedesktop.Secret.Session",
    default_service = "org.freedesktop.secrets",
)]
pub trait Session {
    fn close(&self) -> Result<(), zbus::fdo::Error>;
}

#[proxy(
    interface = "org.freedesktop.Secret.Prompt",
    default_service = "org.freedesktop.secrets",
)]
pub trait Prompt {
    fn prompt(&self, window_id: &str) -> Result<(), zbus::fdo::Error>;
    fn dismiss(&self) -> Result<(), zbus::fdo::Error>;

    #[zbus(signal)]
    fn completed(&self, dismissed: bool, result: Value<'_>) -> Result<(), zbus::Error>;
}
