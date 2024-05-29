use crypto_bigint::{Encoding, NonZero, U256};
use rand::{rngs::StdRng, Rng, SeedableRng};
use starknet::core::{
    crypto::{EcdsaSignError, ExtendedSignature},
    types::FieldElement,
};
use starknet_crypto::{rfc6979_generate_k, sign, SignError};

pub fn non_determinist_ecdsa_sign(
    private_key: &FieldElement,
    message_hash: &FieldElement,
) -> Result<ExtendedSignature, EcdsaSignError> {
    // Seed-retry logic ported from `cairo-lang`
    let mut seed = Some(from_random());
    loop {
        let k = rfc6979_generate_k(message_hash, private_key, seed.as_ref());

        match sign(private_key, message_hash, &k) {
            Ok(sig) => {
                return Ok(sig);
            }
            Err(SignError::InvalidMessageHash) => {
                return Err(EcdsaSignError::MessageHashOutOfRange)
            }
            Err(SignError::InvalidK) => {
                // Bump seed and retry
                seed = match seed {
                    Some(prev_seed) => Some(prev_seed + FieldElement::ONE),
                    None => Some(FieldElement::ONE),
                };
            }
        };
    }
}

fn from_random() -> FieldElement {
    const PRIME: NonZero<U256> = NonZero::from_uint(U256::from_be_hex(
        "0800000000000011000000000000000000000000000000000000000000000001",
    ));

    let mut rng = StdRng::from_entropy();
    let mut buffer = [0u8; 32];
    rng.fill(&mut buffer);

    let random_u256 = U256::from_be_slice(&buffer);
    let secret_scalar = random_u256.rem(&PRIME);

    // It's safe to unwrap here as we're 100% sure it's not out of range
    let secret_scalar = FieldElement::from_byte_slice_be(&secret_scalar.to_be_bytes()).unwrap();

    secret_scalar
}
