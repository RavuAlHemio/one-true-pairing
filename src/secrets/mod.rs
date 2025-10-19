mod crypto;
mod dh;
mod proxies;


use std::collections::{BTreeMap, HashMap};

use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::secrets::crypto::{CryptoAlgorithm, DhIetf1024Sha256Aes128CbcPkcs7Crypto, PlainCrypto};
use crate::secrets::proxies::{CollectionProxy, ItemProxy, ServiceProxy, SessionProxy};


pub struct SecretSession<'a> {
    service_proxy: ServiceProxy<'a>,
    algo: Box<dyn CryptoAlgorithm>,
    session_proxy: SessionProxy<'a>,
}
impl<'a> SecretSession<'a> {
    pub async fn new(conn: &'a Connection) -> Self {
        let service_proxy = ServiceProxy::new(conn)
            .await.expect("failed to connect to secrets service");

        // try stronger algorithms first
        let algorithms: Vec<Box<dyn CryptoAlgorithm>> = vec![
            Box::new(DhIetf1024Sha256Aes128CbcPkcs7Crypto::new()),
            Box::new(PlainCrypto::new()),
        ];
        let mut session_algo_opt = None;
        for mut algo in algorithms {
            let algo_name = algo.get_name();
            eprintln!("trying algorithm {:?}", algo_name);
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
                    eprintln!("error setting up crypto algorithm {:?}: {}", algo_name, e);
                    // try the next one
                },
            }
        }
        let (session_path, algo) = session_algo_opt
            .expect("no supported algorithm found");

        let session_proxy = SessionProxy::new(
            conn,
            session_path,
        ).await.expect("failed to create session proxy");
        Self {
            service_proxy,
            algo,
            session_proxy,
        }
    }

    pub async fn get_secrets(&self) -> BTreeMap<String, OwnedObjectPath> {
        // TODO: make the choice of keyring configurable
        let conn = self.service_proxy.inner().connection();
        let collection = CollectionProxy::new(
            conn,
            ObjectPath::from_static_str("/org/freedesktop/secrets/collection/Default_5fkeyring").unwrap(),
        ).await.expect("failed to connect to default keyring");
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
                conn,
                &item_path,
            ).await else { continue };
            let Ok(name) = item_proxy.label().await else { continue };
            name_to_path.insert(name, item_path);
        }
        name_to_path
    }
}
