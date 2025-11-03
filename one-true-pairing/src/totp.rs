use std::time::SystemTime;

use hmac::{Hmac, Mac};
use hmac::digest::DynDigest;
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use tracing::warn;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};


macro_rules! impl_hmac {
    ($t:ty, $key:expr, $text:expr) => {
        {
            let mut hmac: Hmac<$t> = Hmac::new_from_slice($key)
                .expect("failed to initialize HMAC");
            DynDigest::update(&mut hmac, $text);
            let mut buf = Zeroizing::new(vec![0u8; hmac.output_size()]);
            DynDigest::finalize_into(hmac, buf.as_mut_slice())
                .expect("HMAC lied about output size");
            buf
        }
    };
}


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Algorithm {
    #[default] Sha1,
    Sha256,
    Sha512,
}
impl Algorithm {
    pub fn hmac(&self, key: &[u8], text: &[u8]) -> Zeroizing<Vec<u8>> {
        match self {
            Self::Sha1 => {
                impl_hmac!(Sha1, key, text)
            },
            Self::Sha256 => {
                impl_hmac!(Sha256, key, text)
            },
            Self::Sha512 => {
                impl_hmac!(Sha512, key, text)
            },
        }
    }
}


#[derive(Clone, Debug, Eq, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct TotpParameters {
    pub key: Zeroizing<Vec<u8>>,
    pub url_issuer: Option<Zeroizing<String>>,
    pub username: Option<Zeroizing<String>>,
    pub attrib_issuer: Option<Zeroizing<String>>,
    pub algorithm: Option<Zeroizing<String>>,
    pub digits: Option<u8>,
    pub period_seconds: Option<u64>,
}
impl TotpParameters {
    pub const DEFAULT_ALGORITHM: &str = "SHA1";
    pub const DEFAULT_DIGITS: u8 = 6;
    pub const DEFAULT_PERIOD_SECONDS: u64 = 30;

    pub fn try_from_otpauth_url(url: &str) -> Option<TotpParameters> {
        const PREFIX: &str = "otpauth://totp/";
        let prefixless_u = url.strip_prefix(PREFIX)?;
        let (issuer_username_u, params_str_u) = prefixless_u.split_once('?')
            .unwrap_or((prefixless_u, ""));
        let (url_issuer_u, username_u) = issuer_username_u.split_once(':')
            .unwrap_or((issuer_username_u, ""));

        let mut secret = None;
        let mut attrib_issuer = None;
        let mut algorithm = None;
        let mut digits = None;
        let mut period_seconds = None;
        for property_u in params_str_u.split('&') {
            let Some((key_u, value_u)) = property_u.split_once('=')
                else { continue };
            let key_bytes = urldecode(key_u, true);
            let Some(key) = zv_to_string(key_bytes)
                else { continue };
            let value_bytes = urldecode(value_u, true);
            let Some(value) = zv_to_string(value_bytes)
                else { continue };

            if key.as_str() == "secret" {
                secret = Some(value);
            } else if key.as_str() == "issuer" {
                attrib_issuer = Some(value);
            } else if key.as_str() == "algorithm" {
                algorithm = Some(value);
            } else if key.as_str() == "digits" {
                let Ok(digits_value): Result<u8, _> = value.parse()
                    else { return None };
                if digits_value < 6 || digits_value > 8 {
                    return None;
                }
                digits = Some(digits_value);
            } else if key.as_str() == "period" {
                let Ok(period_seconds_value): Result<u64, _> = value.parse()
                    else { return None };
                if period_seconds_value == 0 {
                    // nice try attempting to trigger a division-by-zero
                    warn!("refusing to process a TOTP URI with a period of 0");
                    return None;
                }
                period_seconds = Some(period_seconds_value);
            } else {
                // FIXME: blow up on unknown attributes?
            }
        }

        let Some(actual_secret) = secret else {
            warn!("cannot process a TOTP URI without a secret");
            return None;
        };
        let Some(key) = decode_base32(&actual_secret) else {
            warn!("cannot process a TOTP URI with a secret that is invalid base-32");
            return None;
        };

        let url_issuer = if url_issuer_u.len() > 0 {
            zv_to_string(urldecode(url_issuer_u, false))
        } else {
            None
        };
        let username = if username_u.len() > 0 {
            zv_to_string(urldecode(username_u, false))
        } else {
            None
        };

        Some(Self {
            key,
            url_issuer,
            username,
            attrib_issuer,
            algorithm,
            digits,
            period_seconds,
        })
    }
}

fn urldecode(value: &str, plus: bool) -> Zeroizing<Vec<u8>> {
    // at worst, value contains no escapes, which means the lengths are the same
    // otherwise, each escape reduces 3 bytes to 1
    let mut bytes = Zeroizing::new(Vec::with_capacity(value.len()));
    let mut iter = value.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            // escape?
            let top = match iter.next() {
                Some(t) => t,
                None => {
                    bytes.push(b'%');
                    continue;
                },
            };
            let top_nibble = hex_to_nibble(top);
            if top_nibble == 0xFF {
                // invalid nibble
                bytes.push(b'%');
                bytes.push(top);
                continue;
            }
            let bottom = match iter.next() {
                Some(b) => b,
                None => {
                    bytes.push(b'%');
                    bytes.push(top);
                    continue;
                },
            };
            let bottom_nibble = hex_to_nibble(bottom);
            if bottom_nibble == 0xFF {
                // invalid nibble
                bytes.push(b'%');
                bytes.push(top);
                bytes.push(bottom);
                continue;
            }
            bytes.push((top_nibble << 4) | bottom_nibble);
        } else if b == b'+' && plus {
            // transform pluses to spaces
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    bytes
}

fn zv_to_string(zv: Zeroizing<Vec<u8>>) -> Option<Zeroizing<String>> {
    // as_slice does not copy
    let zv_slice = zv.as_slice();

    // std::str::from_utf8 does not copy
    // (hopefully Err(_) does not leak too much)
    let zv_str = std::str::from_utf8(zv_slice).ok()?;

    // .to_owned() copies but we wrap it in Zeroizing
    Some(Zeroizing::new(zv_str.to_owned()))
}

fn hex_to_nibble(hex: u8) -> u8 {
    if hex >= b'0' && hex <= b'9' {
        hex - b'0'
    } else if hex >= b'A' && hex <= b'F' {
        hex - b'A' + 10
    } else if hex >= b'a' && hex <= b'f' {
        hex - b'a' + 10
    } else {
        // sentinel value
        0xFF
    }
}

fn decode_base32(b32: &str) -> Option<Zeroizing<Vec<u8>>> {
    let mut ret = Zeroizing::new(Vec::with_capacity(b32.len()));

    // check charset
    let charset_ok = b32.bytes().all(|b|
        (b >= b'A' && b <= b'Z')
        || (b >= b'a' && b <= b'z')
        || (b >= b'2' && b <= b'7')
    );
    if !charset_ok {
        return None;
    }

    // ratio: 8 to 5
    for chunk in b32.as_bytes().chunks(8) {
        let mut value = 0u64;
        for &b in chunk {
            value <<= 5;
            if b >= b'A' && b <= b'Z' {
                value |= u64::from(b - b'A');
            } else if b >= b'a' && b <= b'z' {
                value |= u64::from(b - b'a');
            } else {
                assert!(b >= b'2' && b <= b'7');
                value |= u64::from(b - b'2' + 26);
            }
        }

        match chunk.len() {
            1 => {
                // invalid, need at least 2 base32 chars to encode 1 byte
                return None;
            },
            2 => {
                // 1 byte; 10 - 8 = 2 bits to toss
                ret.push(u8::try_from((value >> (0 + 2)) & 0xFF).unwrap());
            },
            3 => {
                // invalid, need at least 4 base32 chars to encode 2 bytes
                return None;
            },
            4 => {
                // 2 bytes; 20 - 16 = 4 bits to toss
                ret.push(u8::try_from((value >> (8 + 4)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> (0 + 4)) & 0xFF).unwrap());
            },
            5 => {
                // 3 bytes; 25 - 24 = 1 bit to toss
                ret.push(u8::try_from((value >> (16 + 1)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> ( 8 + 1)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> ( 0 + 1)) & 0xFF).unwrap());
            },
            6 => {
                // invalid, need at least 7 base32 chars to encode 4 bytes
                return None;
            },
            7 => {
                // 4 bytes; 35 - 32 = 3 bits to toss
                ret.push(u8::try_from((value >> (24 + 3)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> (16 + 3)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> ( 8 + 3)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> ( 0 + 3)) & 0xFF).unwrap());
            },
            8 => {
                // 5 bytes; 40 - 40 = 0 bits to toss
                ret.push(u8::try_from((value >> (32 + 0)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> (24 + 0)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> (16 + 0)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> ( 8 + 0)) & 0xFF).unwrap());
                ret.push(u8::try_from((value >> ( 0 + 0)) & 0xFF).unwrap());
            },
            _ => unreachable!(),
        }
    }

    Some(ret)
}


// RFC4226
pub fn hotp(
    hmac_algorithm: Algorithm,
    shared_secret: &[u8],
    counter: u64,
    digits: u8,
) -> u32 {
    assert!(digits >= 6 && digits <= 8);

    // HMAC
    let counter_be_bytes = counter.to_be_bytes();
    let hmac = hmac_algorithm.hmac(shared_secret, &counter_be_bytes);

    // Dynamic Truncation
    // obtain the offset from the lowest 4 bits of the last byte
    let offset = usize::from((*hmac.last().unwrap()) & 0xF);
    // obtain 4 bytes beginning at that offset as big-endian u32
    let slice = &hmac[offset..offset+4];
    let mut arr: [u8; 4] = slice.try_into().unwrap();
    let mut truncated = u32::from_be_bytes(arr);
    arr.zeroize();
    // strip off the top bit to insure against signed/unsigned confusion
    truncated &= 0x7FFF_FFFF;

    // modulo
    let ret = match digits {
        6 => {
            let r = truncated % 1_000_000;
            truncated.zeroize();
            r
        },
        7 => {
            let r = truncated % 10_000_000;
            truncated.zeroize();
            r
        },
        8 => {
            let r = truncated % 100_000_000;
            truncated.zeroize();
            r
        },
        _ => unreachable!(),
    };
    ret
}

pub fn totp(
    hmac_algorithm: Algorithm,
    shared_secret: &[u8],
    unix_time: u64,
    period_s: u64,
    digits: u8,
) -> u32 {
    let counter = unix_time / period_s;
    hotp(hmac_algorithm, shared_secret, counter, digits)
}

pub fn totp_now(
    hmac_algorithm: Algorithm,
    shared_secret: &[u8],
    period_s: u64,
    digits: u8,
) -> u32 {
    let unix_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("sorry, system dates before 1970 are not supported");
    totp(hmac_algorithm, shared_secret, unix_time.as_secs(), period_s, digits)
}
