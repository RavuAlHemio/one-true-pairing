use std::fmt::{self, Debug};

use aes::{Aes128, cipher::{BlockDecryptMut, block_padding::Pkcs7}};
use cbc::{Decryptor, cipher::KeyIvInit};
use crypto_bigint::Uint;
use hkdf::Hkdf;
use sha2::Sha256;
use tracing::{debug, error};
use zbus::zvariant::{Array, OwnedValue, Str, Value};
use zeroize::Zeroizing;

use crate::secrets::dh::{DhPrivateKey, DhPublicKey, DiffieHellman};


pub trait CryptoAlgorithm : Debug + Send + Sync {
    /// Obtains the name of the cryptographic algorithm.
    ///
    /// This name is passed to
    /// [`Service::OpenSession`](crate::secrets::proxies::ServiceProxy::open_session).
    fn get_name(&self) -> String;

    /// Obtains the input data for the session.
    ///
    /// This data is passed to
    /// [`Service::OpenSession`](crate::secrets::proxies::ServiceProxy::open_session).
    fn get_session_input(&self) -> OwnedValue;

    /// Sets the output data obtained from creating the session.
    ///
    /// Returns if the data is valid. If it isn't, the client should try another algorithm.
    ///
    /// The data is obtained from
    /// [`Service::OpenSession`](crate::secrets::proxies::ServiceProxy::open_session) (the first
    /// value in the return tuple).
    fn set_session_output(&mut self, output: &Value) -> bool;

    /// Decodes the given secret value with the given parameters and returns the decoded value.
    ///
    /// Returns `None` if decoding fails.
    fn decode_secret(&self, parameters: &[u8], value: &[u8]) -> Option<Zeroizing<Vec<u8>>>;
}


/// The "plain" crypto algorithm, providing no encryption.
#[derive(Debug)]
pub struct PlainCrypto;
impl PlainCrypto {
    pub fn new() -> Self {
        Self
    }
}
impl CryptoAlgorithm for PlainCrypto {
    fn get_name(&self) -> String {
        "plain".to_owned()
    }

    fn get_session_input(&self) -> OwnedValue {
        Str::from_static("").into()
    }

    fn set_session_output(&mut self, output: &Value) -> bool {
        // plain only accepts an empty string as session output
        match output {
            Value::Str(s) if s.len() == 0 => true,
            _ => false,
        }
    }

    fn decode_secret(&self, parameters: &[u8], value: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        // plain only accepts an empty byte slice as parameters
        // and returns the value unchanged
        if parameters.len() == 0 {
            Some(Zeroizing::new(value.to_vec()))
        } else {
            None
        }
    }
}


/// The "dh-ietf1024-sha256-aes128-cbc-pkcs7" crypto algorithm, providing encryption based on
/// Diffie-Hellman and AES-128.
const OAKLEY_2_LIMBS: usize = 16;
const OAKLEY_2_PRIME: Uint<OAKLEY_2_LIMBS> = Uint::from_be_hex(concat!(
    "FFFFFFFF", "FFFFFFFF", "C90FDAA2", "2168C234", "C4C6628B", "80DC1CD1",
    "29024E08", "8A67CC74", "020BBEA6", "3B139B22", "514A0879", "8E3404DD",
    "EF9519B3", "CD3A431B", "302B0A6D", "F25F1437", "4FE1356D", "6D51C245",
    "E485B576", "625E7EC6", "F44C42E9", "A637ED6B", "0BFF5CB6", "F406B7ED",
    "EE386BFB", "5A899FA5", "AE9F2411", "7C4B1FE6", "49286651", "ECE65381",
    "FFFFFFFF", "FFFFFFFF",
));
const OAKLEY_2_GENERATOR: Uint<OAKLEY_2_LIMBS> = Uint::from_u8(2);
pub struct DhIetf1024Sha256Aes128CbcPkcs7Crypto {
    dh: DiffieHellman<OAKLEY_2_LIMBS>,
    privkey: DhPrivateKey<OAKLEY_2_LIMBS>,
    pubkey: DhPublicKey<OAKLEY_2_LIMBS>,
    aes_key: Option<Zeroizing<[u8; 16]>>,
}
impl DhIetf1024Sha256Aes128CbcPkcs7Crypto {
    pub fn new() -> Self {
        // generate a DH keypair using the Second Oakley Group (RFC2409 § 6.2)
        let dh = DiffieHellman::new(OAKLEY_2_PRIME, OAKLEY_2_GENERATOR);
        let privkey = dh.generate_private_key();
        let pubkey = dh.derive_public_key(&privkey);
        Self {
            dh,
            privkey,
            pubkey,
            aes_key: None,
        }
    }
}
impl Debug for DhIetf1024Sha256Aes128CbcPkcs7Crypto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DhIetf1024Sha256Aes128CbcPkcs7Crypto")
            .finish_non_exhaustive()
    }
}
impl CryptoAlgorithm for DhIetf1024Sha256Aes128CbcPkcs7Crypto {
    fn get_name(&self) -> String {
        "dh-ietf1024-sha256-aes128-cbc-pkcs7".to_owned()
    }

    fn get_session_input(&self) -> OwnedValue {
        // my pubkey as a byte array
        let bytes = self.pubkey.to_be_bytes();
        OwnedValue::try_from(Array::from(&*bytes)).unwrap()
    }

    fn set_session_output(&mut self, output: &Value) -> bool {
        // their pubkey as a byte array

        // we can now derive a secret key using DH
        let Some(their_pubkey_array): Option<Array<'_>> = output.try_into().ok() else {
            return false;
        };
        let Some(their_pubkey_bytes): Option<Vec<u8>> = their_pubkey_array.try_into().ok() else {
            return false;
        };
        let Some(their_pubkey) = self.dh.public_key_from_be_bytes(&their_pubkey_bytes) else {
            return false;
        };
        let secret_key = self.dh
            .derive_secret_key(&self.privkey, &their_pubkey);

        // from that, we can derive an AES key using HKDF(salt = NULL, info = "", IKM = secret_key)
        let secret_key_vec: Vec<u8> = secret_key
            .as_limbs()
            .iter()
            .flat_map(|limb| limb.0.to_be_bytes())
            .collect();
        let secret_key = Zeroizing::new(secret_key_vec);
        let hkdf: Hkdf<Sha256> = Hkdf::new(None, secret_key.as_slice());
        let mut aes_key = Zeroizing::new([0u8; 16]);
        hkdf.expand(&[], &mut *aes_key)
            .expect("invalid HKDF OKM size?!");
        self.aes_key = Some(aes_key);
        true
    }

    fn decode_secret(&self, parameters: &[u8], value: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        // parameters is the 16-byte AES128-CBC initialization vector
        // value is the ciphertext with PKCS#7 padding
        if parameters.len() != 16 {
            error!("parameters.len(): expected 16, obtained {}", parameters.len());
            return None;
        }
        let Some(aes_key) = self.aes_key.as_ref() else {
            error!("no AES key set");
            return None;
        };

        let aes128_cbc_pkcs7_dec: Decryptor<Aes128> = cbc::Decryptor::new_from_slices(&**aes_key, parameters)
            .expect("failed to create AES-128 CBC PKCS#7-padding decryptor");
        let mut secret_buf = Zeroizing::new(vec![0u8; value.len()]);
        let Ok(decrypted_slice) = aes128_cbc_pkcs7_dec.decrypt_padded_b2b_mut::<Pkcs7>(value, &mut **secret_buf) else {
            // incorrect padding
            error!("padding is not OK");
            return None;
        };
        let decrypted_slice_len = decrypted_slice.len();
        secret_buf.drain(decrypted_slice_len..);
        Some(secret_buf)
    }
}
