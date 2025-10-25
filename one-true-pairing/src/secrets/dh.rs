use crypto_bigint::{Concat, Random, Split, Uint};
use crypto_bigint::rand_core::OsRng;
use crypto_bigint::modular::{MontyForm, MontyParams};
use zeroize::Zeroizing;

use crate::secrets::UintExt;


pub struct DiffieHellman<const LIMBS: usize> {
    prime: Uint<LIMBS>,
    generator: Uint<LIMBS>,
    generator_monty: MontyForm<LIMBS>,
    prime_monty_params: MontyParams<LIMBS>,
}
impl<const LIMBS: usize> DiffieHellman<LIMBS> {
    pub fn new<const WIDE_LIMBS: usize>(prime: Uint<LIMBS>, generator: Uint<LIMBS>) -> Self
            where
                Uint<LIMBS> : Concat<Output = Uint<WIDE_LIMBS>>,
                Uint<WIDE_LIMBS> : Split<Output = Uint<LIMBS>> {
        let odd_prime = prime.to_odd()
            .expect("prime must be odd");
        let prime_monty_params = MontyParams::new(odd_prime);
        let generator_monty = MontyForm::new(&generator, prime_monty_params);
        Self {
            prime,
            generator,
            generator_monty,
            prime_monty_params,
        }
    }

    pub fn generate_private_key(&self) -> DhPrivateKey<LIMBS> {
        let q = (self.prime - Uint::ONE) / self.generator;
        let two = Uint::from_u8(2);
        let q_minus_two = q - two;

        loop {
            let rand_p: Uint<LIMBS> = Uint::random(&mut OsRng);

            // big enough for p => divide by 2 to get something that should work with q
            let rand_q = rand_p / two;

            if rand_q >= two && rand_q <= q_minus_two {
                return DhPrivateKey {
                    private_key_uint: rand_q,
                };
            }
        }
    }

    pub fn derive_public_key<const WIDE_LIMBS: usize>(&self, private_key: &DhPrivateKey<LIMBS>) -> DhPublicKey<LIMBS>
            where
                Uint<LIMBS> : Concat<Output = Uint<WIDE_LIMBS>>,
                Uint<WIDE_LIMBS> : Split<Output = Uint<LIMBS>> {
        // generator ** privkey mod prime
        let powered = self.generator_monty.pow(&private_key.private_key_uint);
        DhPublicKey {
            public_key_monty: powered,
        }
    }

    pub fn public_key_from_be_bytes(&self, bytes: &[u8]) -> Option<DhPublicKey<LIMBS>> {
        let limb = crypto_bigint::Limb::from_u8(0);
        let limb_size = std::mem::size_of_val(&limb.0);
        let byte_count = LIMBS * limb_size;

        if bytes.len() > byte_count {
            return None;
        }
        let mut bytes_vec = Zeroizing::new(Vec::with_capacity(byte_count));
        if bytes.len() < byte_count {
            let pad_count = byte_count - bytes.len();
            bytes_vec.extend(std::iter::repeat_n(0x00, pad_count));
        }
        bytes_vec.extend_from_slice(bytes);
        assert_eq!(bytes_vec.len(), byte_count);

        // limbify
        let public_key = Uint::from_be_slice(&bytes_vec);
        let public_key_monty = MontyForm::new(
            &public_key,
            self.prime_monty_params,
        );
        Some(DhPublicKey {
            public_key_monty,
        })
    }

    pub fn derive_secret_key(&self, my_private_key: &DhPrivateKey<LIMBS>, their_public_key: &DhPublicKey<LIMBS>) -> Uint<LIMBS> {
        let secret_key_monty = their_public_key.public_key_monty
            .pow(&my_private_key.private_key_uint);
        secret_key_monty.retrieve()
    }
}

pub struct DhPrivateKey<const LIMBS: usize> {
    private_key_uint: Uint<LIMBS>,
}
impl<const LIMBS: usize> DhPrivateKey<LIMBS> {
    pub fn to_be_bytes_warning_dangerous(&self) -> Zeroizing<Vec<u8>> {
        let private_key_vec = self.private_key_uint
            .to_be_byte_vec();
        Zeroizing::new(private_key_vec)
    }
}

pub struct DhPublicKey<const LIMBS: usize> {
    public_key_monty: MontyForm<LIMBS>,
}
impl<const LIMBS: usize> DhPublicKey<LIMBS> {
    pub fn to_be_bytes(&self) -> Zeroizing<Vec<u8>> {
        let public_key = self.public_key_monty.retrieve();
        let public_key_vec = public_key
            .to_be_byte_vec();
        Zeroizing::new(public_key_vec)
    }
}
