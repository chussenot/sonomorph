//! G.711 mu-law: how chip8 samples are stored, one byte a sample.
//!
//! The engine renders at 16-bit precision. Linear 8-bit storage would keep
//! only the top byte, so a quiet passage (a break, a fading tail) would
//! sound at a few bits. Mu-law spends the same byte logarithmically: about
//! 13 bits of range, with a noise floor that follows the signal down.
//!
//! Every function here is `const` or allocation-free, so the mixer can
//! decode in the audio callback, and a build script can encode.

const BIAS: i32 = 0x84;
const CLIP: i32 = 32_635;

/// The byte for silence.
pub const SILENCE: u8 = 0xFF;

/// One 16-bit sample as mu-law.
pub const fn encode(sample: i16) -> u8 {
    let mut s = sample as i32;
    let sign = if s < 0 {
        s = -s;
        0x80
    } else {
        0
    };
    s = if s < CLIP { s } else { CLIP } + BIAS;
    let mut exponent = 7;
    let mut mask = 0x4000;
    while exponent > 0 && s & mask == 0 {
        exponent -= 1;
        mask >>= 1;
    }
    let mantissa = (s >> (exponent + 3)) & 0x0F;
    !((sign | (exponent << 4) | mantissa) as u8)
}

/// One mu-law byte as its 16-bit sample.
pub const fn decode(byte: u8) -> i16 {
    let u = !byte;
    let exponent = (u >> 4) & 0x07;
    let mantissa = (u & 0x0F) as i32;
    let magnitude = (((mantissa << 3) + BIAS) << exponent) - BIAS;
    if u & 0x80 != 0 {
        -magnitude as i16
    } else {
        magnitude as i16
    }
}

/// Every mu-law byte as a sample in `-1.0..1.0`: what the mixer reads.
pub static TO_F32: [f32; 256] = {
    let mut table = [0.0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = decode(i as u8) as f32 / 32_768.0;
        i += 1;
    }
    table
};

/// One mu-law byte as a sample in `-1.0..1.0`.
#[inline]
pub fn sample(byte: u8) -> f32 {
    TO_F32[usize::from(byte)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_ff_and_decodes_to_zero() {
        assert_eq!(encode(0), SILENCE);
        assert_eq!(decode(SILENCE), 0);
        assert_eq!(sample(SILENCE), 0.0);
    }

    #[test]
    fn a_round_trip_keeps_the_sign_and_about_13_bits() {
        for s in (i16::MIN..=i16::MAX).step_by(7) {
            let back = i32::from(decode(encode(s)));
            let s = i32::from(s).clamp(-CLIP, CLIP);
            assert!(
                back.signum() == s.signum() || back.abs() <= 8,
                "{s} -> {back}"
            );
            // The step grows with the level: 1/16 of the segment's size.
            let tolerance = (s.abs() / 16).max(8);
            assert!((back - s).abs() <= tolerance, "{s} -> {back}");
        }
    }

    #[test]
    fn every_byte_survives_decode_then_encode() {
        for byte in 0..=255u8 {
            // 0x7F and 0xFF are both zero; zero encodes as 0xFF.
            let expected = if byte == 0x7F { SILENCE } else { byte };
            assert_eq!(encode(decode(byte)), expected, "{byte:#04x}");
        }
    }

    #[test]
    fn the_table_is_symmetric_and_in_range() {
        for byte in 0..=127u8 {
            assert_eq!(sample(byte), -sample(byte | 0x80), "{byte:#04x}");
        }
        assert!(TO_F32.iter().all(|s| (-1.0..1.0).contains(s)));
    }
}
