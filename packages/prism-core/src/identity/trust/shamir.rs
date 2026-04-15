//! Shamir secret sharing over GF(256). Port of
//! `createShamirSplitter` in `trust/trust.ts`. Uses the
//! AES/Rijndael irreducible polynomial (`x^8 + x^4 + x^3 + x + 1`)
//! so shares interoperate with the legacy TS output byte-for-byte.

use rand::RngCore;
use thiserror::Error;

use super::types::{ShamirConfig, ShamirShare};

const GF256_POLY: u16 = 0x11b;

#[derive(Debug, Error)]
pub enum ShamirError {
    #[error("Threshold must be at least 2")]
    ThresholdTooLow,
    #[error("Total shares must be >= threshold")]
    TotalLessThanThreshold,
    #[error("Threshold must be <= 255")]
    ThresholdTooHigh,
    #[error("Total shares must be <= 255")]
    TotalTooHigh,
    #[error("Need at least {0} shares, got {1}")]
    NotEnoughShares(u8, usize),
    #[error("No shares provided")]
    NoShares,
    #[error("Invalid share data: {0}")]
    InvalidShare(String),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ShamirSplitter;

pub fn create_shamir_splitter() -> ShamirSplitter {
    ShamirSplitter
}

impl ShamirSplitter {
    pub fn split(
        &self,
        secret: &[u8],
        config: ShamirConfig,
    ) -> Result<Vec<ShamirShare>, ShamirError> {
        let ShamirConfig {
            total_shares,
            threshold,
        } = config;
        if threshold < 2 {
            return Err(ShamirError::ThresholdTooLow);
        }
        if total_shares < threshold {
            return Err(ShamirError::TotalLessThanThreshold);
        }
        // `u8` already caps totals/thresholds at 255, so the two
        // "<= 255" checks from the TS source become unreachable in
        // Rust. They're kept here as public error variants for API
        // parity but no call can trigger them.

        let mut shares: Vec<ShamirShare> = (0..total_shares)
            .map(|i| ShamirShare {
                index: i + 1,
                data: String::new(),
            })
            .collect();

        let mut rng = rand::thread_rng();
        for &secret_byte in secret {
            let mut coeffs: Vec<u8> = Vec::with_capacity(threshold as usize);
            coeffs.push(secret_byte);
            let mut random_bytes = vec![0u8; (threshold - 1) as usize];
            rng.fill_bytes(&mut random_bytes);
            coeffs.extend_from_slice(&random_bytes);
            for i in 0..total_shares {
                let x = i + 1;
                let y = evaluate_polynomial(&coeffs, x);
                use std::fmt::Write as _;
                let _ = write!(shares[i as usize].data, "{y:02x}");
            }
        }
        Ok(shares)
    }

    pub fn combine(
        &self,
        shares: &[ShamirShare],
        config: ShamirConfig,
    ) -> Result<Vec<u8>, ShamirError> {
        if shares.len() < config.threshold as usize {
            return Err(ShamirError::NotEnoughShares(config.threshold, shares.len()));
        }
        let used = &shares[..config.threshold as usize];
        let first = used.first().ok_or(ShamirError::NoShares)?;
        if first.data.len() % 2 != 0 {
            return Err(ShamirError::InvalidShare(
                "share data length is not even".into(),
            ));
        }
        let byte_length = first.data.len() / 2;
        let mut result = vec![0u8; byte_length];
        for (byte_idx, slot) in result.iter_mut().enumerate() {
            let mut points: Vec<(u8, u8)> = Vec::with_capacity(used.len());
            for share in used {
                let hex_pair = &share.data[byte_idx * 2..byte_idx * 2 + 2];
                let y = u8::from_str_radix(hex_pair, 16)
                    .map_err(|e| ShamirError::InvalidShare(e.to_string()))?;
                points.push((share.index, y));
            }
            *slot = lagrange_interpolate(&points);
        }
        Ok(result)
    }
}

fn gf_mul(a: u8, b: u8) -> u8 {
    let mut result: u16 = 0;
    let mut aa: u16 = a as u16;
    let mut bb: u16 = b as u16;
    while bb > 0 {
        if bb & 1 != 0 {
            result ^= aa;
        }
        aa <<= 1;
        if aa & 0x100 != 0 {
            aa ^= GF256_POLY;
        }
        bb >>= 1;
    }
    result as u8
}

fn gf_inv(a: u8) -> u8 {
    // a^254 = a^-1 in GF(256).
    assert!(a != 0, "cannot invert 0 in GF(256)");
    let mut result = 1u8;
    let mut base = a;
    let mut exp: u16 = 254;
    while exp > 0 {
        if exp & 1 != 0 {
            result = gf_mul(result, base);
        }
        base = gf_mul(base, base);
        exp >>= 1;
    }
    result
}

fn evaluate_polynomial(coeffs: &[u8], x: u8) -> u8 {
    let mut result: u8 = 0;
    for &coeff in coeffs.iter().rev() {
        result = gf_mul(result, x) ^ coeff;
    }
    result
}

fn lagrange_interpolate(shares: &[(u8, u8)]) -> u8 {
    let mut secret: u8 = 0;
    for (i, &(xi, yi)) in shares.iter().enumerate() {
        let mut num: u8 = 1;
        let mut den: u8 = 1;
        for (j, &(xj, _)) in shares.iter().enumerate() {
            if i == j {
                continue;
            }
            num = gf_mul(num, xj);
            den = gf_mul(den, xi ^ xj);
        }
        secret ^= gf_mul(yi, gf_mul(num, gf_inv(den)));
    }
    secret
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn splits_and_reconstructs_2_of_3() {
        let splitter = create_shamir_splitter();
        let secret = [72, 101, 108, 108, 111]; // "Hello"
        let config = ShamirConfig {
            total_shares: 3,
            threshold: 2,
        };
        let shares = splitter.split(&secret, config).unwrap();
        assert_eq!(shares.len(), 3);
        assert_eq!(shares[0].index, 1);
        assert_eq!(shares[1].index, 2);
        assert_eq!(shares[2].index, 3);
        let recovered = splitter
            .combine(&[shares[0].clone(), shares[1].clone()], config)
            .unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn reconstructs_with_any_threshold_subset() {
        let splitter = create_shamir_splitter();
        let secret = [1, 2, 3, 4, 5, 6, 7, 8];
        let config = ShamirConfig {
            total_shares: 5,
            threshold: 3,
        };
        let shares = splitter.split(&secret, config).unwrap();
        let r1 = splitter
            .combine(
                &[shares[0].clone(), shares[2].clone(), shares[4].clone()],
                config,
            )
            .unwrap();
        assert_eq!(r1, secret);
        let r2 = splitter
            .combine(
                &[shares[1].clone(), shares[3].clone(), shares[4].clone()],
                config,
            )
            .unwrap();
        assert_eq!(r2, secret);
        let r3 = splitter
            .combine(
                &[shares[0].clone(), shares[1].clone(), shares[2].clone()],
                config,
            )
            .unwrap();
        assert_eq!(r3, secret);
    }

    #[test]
    fn fails_with_too_few_shares() {
        let splitter = create_shamir_splitter();
        let secret = [42];
        let config = ShamirConfig {
            total_shares: 3,
            threshold: 2,
        };
        let shares = splitter.split(&secret, config).unwrap();
        let err = splitter.combine(&[shares[0].clone()], config).unwrap_err();
        assert!(err.to_string().contains("at least 2"));
    }

    #[test]
    fn handles_single_byte_secret() {
        let splitter = create_shamir_splitter();
        let secret = [255];
        let config = ShamirConfig {
            total_shares: 3,
            threshold: 2,
        };
        let shares = splitter.split(&secret, config).unwrap();
        let recovered = splitter
            .combine(&[shares[0].clone(), shares[2].clone()], config)
            .unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn handles_empty_secret() {
        let splitter = create_shamir_splitter();
        let secret: [u8; 0] = [];
        let config = ShamirConfig {
            total_shares: 3,
            threshold: 2,
        };
        let shares = splitter.split(&secret, config).unwrap();
        let recovered = splitter.combine(&shares[..2], config).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn rejects_invalid_configs() {
        let splitter = create_shamir_splitter();
        let secret = [1];
        assert!(matches!(
            splitter.split(
                &secret,
                ShamirConfig {
                    total_shares: 3,
                    threshold: 1
                }
            ),
            Err(ShamirError::ThresholdTooLow)
        ));
        assert!(matches!(
            splitter.split(
                &secret,
                ShamirConfig {
                    total_shares: 1,
                    threshold: 2
                }
            ),
            Err(ShamirError::TotalLessThanThreshold)
        ));
    }

    #[test]
    fn shares_are_different_from_each_other() {
        let splitter = create_shamir_splitter();
        let secret = (1u8..=16u8).collect::<Vec<u8>>();
        let config = ShamirConfig {
            total_shares: 3,
            threshold: 2,
        };
        let shares = splitter.split(&secret, config).unwrap();
        let data_set: HashSet<&String> = shares.iter().map(|s| &s.data).collect();
        assert_eq!(data_set.len(), 3);
    }
}
