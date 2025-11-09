mod crypto;
mod dh;
mod proxies;


use std::collections::{BTreeMap, HashMap};

use crypto_bigint::Uint;
use futures_util::StreamExt;
use tracing::{debug, error, info, warn};
use zbus::{Connection, DBusError};
use zbus::names::BusName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use zeroize::Zeroizing;

use crate::secrets::crypto::{CryptoAlgorithm, DhIetf1024Sha256Aes128CbcPkcs7Crypto, PlainCrypto};
use crate::secrets::proxies::{CollectionProxy, ItemProxy, PromptProxy, ServiceProxy};


#[derive(Debug)]
pub struct SecretSession {
    connection: Option<Connection>,
    algo: Box<dyn CryptoAlgorithm>,
    session_path: OwnedObjectPath,
    collection_path: OwnedObjectPath,
}
impl SecretSession {
    pub async fn new(conn: Connection, collection_label: &str) -> Self {
        let service_proxy = ServiceProxy::new(&conn)
            .await.expect("failed to connect to secrets service");

        // try stronger algorithms first
        let algorithms: Vec<Box<dyn CryptoAlgorithm>> = vec![
            Box::new(DhIetf1024Sha256Aes128CbcPkcs7Crypto::new()),
            Box::new(PlainCrypto::new()),
        ];
        let mut session_algo_opt = None;
        for mut algo in algorithms {
            let algo_name = algo.get_name();
            debug!("trying algorithm {:?}", algo_name);
            let session_res = service_proxy.open_session(
                &algo.get_name(),
                &algo.get_session_input(),
            ).await;
            match session_res {
                Ok((session_output, session_name)) => {
                    if !algo.set_session_output(&session_output) {
                        panic!("invalid session output received setting up algo {:?}", algo_name);
                    }
                    session_algo_opt = Some((session_name, algo));
                    break;
                },
                Err(e) => {
                    warn!("error setting up crypto algorithm {:?}: {}", algo_name, e);
                    // try the next one
                },
            }
        }
        let (session_path, algo) = session_algo_opt
            .expect("no supported algorithm found");

        // find our collection
        debug!("querying collections");
        let collections = service_proxy.collections()
            .await.expect("failed to obtain list of collections");
        let mut wanted_collection_path_opt = None;
        for collection_path in &collections {
            let collection_proxy = CollectionProxy::new(&conn, collection_path)
                .await.expect("failed to obtain collection proxy");
            let label = collection_proxy.label()
                .await.expect("failed to request collection label");
            if label == collection_label {
                wanted_collection_path_opt = Some(collection_path.clone());
                break;
            }
        }
        let wanted_collection_path = wanted_collection_path_opt
            .expect("no collection of secrets found");
        debug!("found requested collection at {}", wanted_collection_path);

        // unlock if necessary
        let collection_proxy = CollectionProxy::new(&conn, &wanted_collection_path)
            .await.expect("failed to re-obtain collection proxy");
        let collection_is_locked = collection_proxy.locked()
            .await.expect("failed to find out if collection is locked");
        if collection_is_locked {
            debug!("collection is locked");
            let (unlocked_collections, prompt_path) = service_proxy.unlock(&[wanted_collection_path.as_ref()])
                .await.expect("failed to request unlock of collection");
            if unlocked_collections.len() == 0 {
                // okay, the user must be prompted
                let prompt_proxy = PromptProxy::new(&conn, &prompt_path)
                    .await.expect("failed to obtain prompt proxy");
                debug!("prompting user");
                prompt_proxy
                    .prompt("").await.expect("failed to trigger prompt");
                let mut completion_stream = prompt_proxy
                    .receive_completed().await.expect("failed to obtain prompt completion stream");
                let completion = completion_stream
                    .next().await.expect("failed to receive prompt completion item");
                let completion_args = completion
                    .args().expect("failed to decice prompt completion signal arguments");
                if completion_args.dismissed {
                    warn!("user dismissed unlock prompt");
                } else {
                    debug!("user completed unlock prompt");
                }
            }
        } else {
            debug!("collection is not locked");
        }

        Self {
            connection: Some(conn),
            algo,
            session_path,
            collection_path: wanted_collection_path,
        }
    }

    pub async fn get_secrets(&self) -> BTreeMap<String, OwnedObjectPath> {
        let collection = CollectionProxy::new(
            self.connection.as_ref().unwrap(),
            &self.collection_path,
        ).await.expect("failed to connect to secret collection");
        let mut attributes = HashMap::new();
        attributes.insert(
            "xdg:schema".to_owned(),
            "com.ondrahosek.OneTruePairing".to_owned(),
        );
        let item_paths = collection.search_items(&attributes)
            .await.expect("failed to search for OTP items");

        let mut name_to_path = BTreeMap::new();
        for item_path in item_paths {
            // ask for its name
            let Ok(item_proxy) = ItemProxy::new(
                self.connection.as_ref().unwrap(),
                &item_path,
            ).await else { continue };
            let Ok(name) = item_proxy.label().await else { continue };
            name_to_path.insert(name, item_path);
        }
        name_to_path
    }

    pub async fn get_secret(&self, item_path: ObjectPath<'_>) -> Option<Zeroizing<Vec<u8>>> {
        let item_proxy = match ItemProxy::new(self.connection.as_ref().unwrap(), item_path).await {
            Ok(ip) => ip,
            Err(e) => {
                error!("failed to obtain item proxy: {}", e);
                return None;
            }
        };
        let session_path_copy = self.session_path.clone();
        let returned_secret = match item_proxy.get_secret(session_path_copy.into()).await {
            Ok(rs) => rs,
            Err(e) => {
                error!("failed to obtain secret from item: {}", e);
                return None;
            }
        };
        match self.algo.decode_secret(&returned_secret.parameters, &returned_secret.value) {
            Some(s) => Some(s),
            None => {
                error!("algo failed to decode secret");
                return None;
            },
        }
    }

    pub async fn drop_connection(&mut self) {
        let connection_opt = std::mem::replace(
            &mut self.connection,
            None,
        );
        if let Some(connection) = connection_opt {
            connection.graceful_shutdown().await;
        }
    }
}

trait UintExt {
    fn to_be_byte_vec(&self) -> Vec<u8>;
}
impl<const LIMBS: usize> UintExt for Uint<LIMBS> {
    fn to_be_byte_vec(&self) -> Vec<u8> {
        self
            .as_limbs()
            .iter()
            .rev() // order is least-significant limb first
            .flat_map(|limb| limb.0.to_be_bytes())
            .collect()
    }
}

/// Waits until a secret manager appears on the bus.
pub async fn wait_for_secret_manager(
    dbus_conn: &zbus::connection::Connection,
) {
    // first register the change listener, then ask about an existing owner;
    // this prevents a race condition

    // talk to the bus manager
    let dbus_proxy = zbus::fdo::DBusProxy::new(&dbus_conn)
        .await.expect("failed to create D-Bus API proxy");

    // register the change listener
    let mut new_kid_on_the_block_stream = dbus_proxy.receive_name_owner_changed_with_args(&[
        (0, "org.freedesktop.secrets"),
    ])
        .await.expect("failed to obtain stream waiting for secrets owner change");

    // ask if there (already) is someone there
    let name_owner_res = dbus_proxy
        .get_name_owner(BusName::try_from("org.freedesktop.secrets").unwrap()).await;
    match name_owner_res {
        Ok(_) => {
            // nothing to wait for :-)
            return;
        },
        Err(e) if e.name() == "org.freedesktop.DBus.Error.NameHasNoOwner" => {
            // we shall wait
        },
        Err(e) => panic!("failed to query org.freedesktop.secrets name owner: {}", e),
    }

    warn!("no one is currently offering org.freedesktop.secrets; I have to be patient");

    loop {
        let new_kid = new_kid_on_the_block_stream
            .next().await.expect("new-owner stream ended");
        let new_kid_args = new_kid
            .args().expect("failed to obtain new-owner event args");
        if new_kid_args.name() != "org.freedesktop.secrets" {
            continue;
        }
        if new_kid_args.new_owner().is_none() {
            // still nobody there
            continue;
        }

        // this is it
        break;
    }

    info!("a secret provider has appeared :-)");
}
