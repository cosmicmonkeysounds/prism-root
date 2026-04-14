//! Base58btc encode/decode.
//!
//! Port of the legacy hand-rolled implementation in
//! `identity/did/identity.ts`. Kept byte-for-byte identical (same
//! alphabet, same leading-zero handling) so existing DID strings
//! round-trip through the Rust version unchanged.

use super::error::IdentityError;

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Encode bytes as base58btc.
pub fn encode_base58(bytes: &[u8]) -> String {
    // Count leading zeros.
    let zeroes = bytes.iter().take_while(|&&b| b == 0).count();

    // Convert to base58.
    let mut digits: Vec<u32> = Vec::new();
    for &byte in bytes {
        let mut carry = byte as u32;
        for digit in digits.iter_mut() {
            carry += *digit << 8;
            *digit = carry % 58;
            carry /= 58;
        }
        while carry > 0 {
            digits.push(carry % 58);
            carry /= 58;
        }
    }

    let mut result = String::with_capacity(zeroes + digits.len());
    for _ in 0..zeroes {
        result.push('1');
    }
    for digit in digits.iter().rev() {
        result.push(BASE58_ALPHABET[*digit as usize] as char);
    }
    result
}

/// Decode a base58btc string back to bytes.
pub fn decode_base58(input: &str) -> Result<Vec<u8>, IdentityError> {
    // Count leading '1's.
    let zeroes = input.chars().take_while(|&c| c == '1').count();

    let mut digits: Vec<u32> = Vec::new();
    for ch in input.chars() {
        let idx = BASE58_ALPHABET
            .iter()
            .position(|&b| b as char == ch)
            .ok_or(IdentityError::InvalidBase58Char(ch))?;

        let mut carry = idx as u32;
        for digit in digits.iter_mut() {
            carry += *digit * 58;
            *digit = carry & 0xff;
            carry >>= 8;
        }
        while carry > 0 {
            digits.push(carry & 0xff);
            carry >>= 8;
        }
    }

    let mut result = vec![0u8; zeroes + digits.len()];
    for (i, digit) in digits.iter().rev().enumerate() {
        result[zeroes + i] = *digit as u8;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_arbitrary_bytes() {
        let input = [0u8, 0, 1, 2, 3, 255, 128, 64];
        let encoded = encode_base58(&input);
        let decoded = decode_base58(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn encodes_empty_array() {
        assert_eq!(encode_base58(&[]), "");
        assert_eq!(decode_base58("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn encodes_leading_zeros_as_one() {
        let input = [0u8, 0, 0, 1];
        let encoded = encode_base58(&input);
        assert!(encoded.starts_with("111"));
    }

    #[test]
    fn throws_on_invalid_base58_character() {
        let err = decode_base58("0OIl").unwrap_err();
        assert!(matches!(err, IdentityError::InvalidBase58Char(_)));
    }
}
