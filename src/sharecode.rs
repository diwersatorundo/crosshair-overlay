use crate::config::{Config, Rgba};
use std::fmt;

pub const DICTIONARY: &[u8; 57] = b"ABCDEFGHJKLMNOPQRSTUVWXYZabcdefhijkmnopqrstuvwxyz23456789";

const CS2_PREFIX: &str = "CS";
const CS2_CHAR_COUNT: usize = 44;
const CS2_BYTE_COUNT: usize = 32;
const LEGACY_BYTE_COUNT: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareFormat {
    Cs2V1,
    LegacyV1,
    LegacyV3,
    LegacyV4,
}

impl ShareFormat {
    pub const fn label_ru(self) -> &'static str {
        match self {
            ShareFormat::Cs2V1 => "CS2 (v1)",
            ShareFormat::LegacyV1 => "CS:GO (legacy v1)",
            ShareFormat::LegacyV3 => "CS2 (legacy v3)",
            ShareFormat::LegacyV4 => "CS2 (legacy v4)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareError {
    InvalidFormat,
    InvalidChecksum,
    UnsupportedVersion,
    InvalidScreenHeight,
    Overflow,
}

impl ShareError {
    pub const fn user_message_ru(&self) -> &'static str {
        match self {
            ShareError::InvalidFormat => "Неверный формат кода",
            ShareError::InvalidChecksum => "Код повреждён (не сходится контрольная сумма)",
            ShareError::UnsupportedVersion => "Неподдерживаемая версия кода",
            ShareError::InvalidScreenHeight => "В коде недопустимая высота экрана",
            ShareError::Overflow => "Код содержит слишком большое число",
        }
    }
}

impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ShareError::InvalidFormat => "invalid format",
            ShareError::InvalidChecksum => "invalid checksum",
            ShareError::UnsupportedVersion => "unsupported version",
            ShareError::InvalidScreenHeight => "invalid screen height",
            ShareError::Overflow => "number overflow",
        })
    }
}

impl std::error::Error for ShareError {}

#[derive(Clone, Debug)]
pub struct Decoded {
    pub config: Config,
    pub format: ShareFormat,
}

pub fn decode(code: &str) -> Result<Decoded, ShareError> {
    let code = code.trim();
    if is_cs2(code) {
        decode_cs2(code)
    } else {
        decode_legacy(code)
    }
}

pub fn is_cs2(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 2 + CS2_CHAR_COUNT
        && bytes.starts_with(CS2_PREFIX.as_bytes())
        && bytes[2..].iter().all(|b| dictionary_index(*b).is_some())
}

fn dictionary_index(byte: u8) -> Option<u8> {
    DICTIONARY
        .iter()
        .position(|c| *c == byte)
        .map(|index| index as u8)
}

fn checksum_ok(bytes: &[u8]) -> bool {
    let sum: u32 = bytes[1..].iter().map(|b| *b as u32).sum();
    bytes[0] as u32 == sum % 256
}

fn chars_to_bytes(chars: &str, byte_len: usize) -> Result<Vec<u8>, ShareError> {
    if !chars
        .as_bytes()
        .iter()
        .all(|b| dictionary_index(*b).is_some())
    {
        return Err(ShareError::InvalidFormat);
    }
    let mut number = vec![0u8; byte_len];
    for byte in chars.as_bytes().iter().rev() {
        let digit = dictionary_index(*byte).ok_or(ShareError::InvalidFormat)? as u32;
        let mut carry = digit;
        for limb in number.iter_mut() {
            let value = *limb as u32 * DICTIONARY.len() as u32 + carry;
            *limb = value as u8;
            carry = value >> 8;
        }
        if carry != 0 {
            return Err(ShareError::Overflow);
        }
    }
    number.reverse();
    Ok(number)
}

fn bytes_to_chars(bytes: &[u8], char_count: usize) -> String {
    let mut number = bytes.to_vec();
    let mut out = String::with_capacity(char_count);
    for _ in 0..char_count {
        let mut remainder = 0u32;
        for byte in number.iter_mut() {
            let value = (remainder << 8) | *byte as u32;
            *byte = (value / DICTIONARY.len() as u32) as u8;
            remainder = value % DICTIONARY.len() as u32;
        }
        out.push(DICTIONARY[remainder as usize] as char);
    }
    out
}

fn decode_cs2(code: &str) -> Result<Decoded, ShareError> {
    let bytes = chars_to_bytes(&code[2..], CS2_BYTE_COUNT)?;
    if !checksum_ok(&bytes) {
        return Err(ShareError::InvalidChecksum);
    }
    if bytes[1] != 1 {
        return Err(ShareError::UnsupportedVersion);
    }
    let raw_screen_height = u16::from_le_bytes([bytes[2], bytes[3]]) as u32;
    if raw_screen_height == 0 {
        return Err(ShareError::InvalidScreenHeight);
    }

    let bits = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
    let gap = i16::from_le_bytes([bytes[14], bytes[15]]) as i32;
    let cfg = Config {
        enabled: true,
        style: (bytes[4] & 0x1f).min(9),
        follow_recoil: bytes[4] & 0x20 != 0,
        center_dot: bytes[4] & 0x40 != 0,
        t_style: bytes[4] & 0x80 != 0,
        outline: ((bytes[13] >> 6) as u8).min(2),
        color: Rgba::new(bytes[5], bytes[6], bytes[7], bytes[8]),
        outline_color: Rgba::new(bytes[9], bytes[10], bytes[11], bytes[12]),
        gap: gap.clamp(-3840, 3840),
        length: bytes[16] as u32,
        thickness: (bytes[13] & 0x3f).min(32) as u32,
        screen_height: raw_screen_height.max(240),
        scale_with_screen: true,
        offset_x: 0,
        offset_y: 0,
        dynamic_spread_limit: bytes[17] as u32,
        split_distance: (bits & 0x7f) as u32,
        inner_split_alpha: ((bits >> 7) & 0x7f).min(100) as f32 / 100.0,
        outer_split_alpha: (((bits >> 14) & 0x7f).min(70) + 30) as f32 / 100.0,
        split_size_ratio: ((bits >> 21) & 0x7f).min(100) as f32 / 100.0,
        scope_dot_scale: ((bytes[22] as f32 + 10.0) / 100.0).min(2.0),
        scope_dot_color_match: (bits >> 28) & 1 == 1,
    }
    .sanitized();

    Ok(Decoded {
        config: cfg,
        format: ShareFormat::Cs2V1,
    })
}

pub fn encode_cs2(cfg: &Config) -> String {
    let cfg = cfg.clone().sanitized();
    let mut bytes = vec![0u8; CS2_BYTE_COUNT];
    bytes[1] = 1;

    let screen_height = cfg.screen_height.clamp(240, 65535) as u16;
    bytes[2..4].copy_from_slice(&screen_height.to_le_bytes());
    bytes[4] = (cfg.style.min(9) & 0x1f)
        | (u8::from(cfg.follow_recoil) << 5)
        | (u8::from(cfg.center_dot) << 6)
        | (u8::from(cfg.t_style) << 7);
    bytes[5] = cfg.color.r;
    bytes[6] = cfg.color.g;
    bytes[7] = cfg.color.b;
    bytes[8] = cfg.color.a;
    bytes[9] = cfg.outline_color.r;
    bytes[10] = cfg.outline_color.g;
    bytes[11] = cfg.outline_color.b;
    bytes[12] = cfg.outline_color.a;
    bytes[13] = (cfg.thickness.min(32) as u8) | (cfg.outline.min(2) << 6);
    let gap = cfg.gap.clamp(-3840, 3840) as i16;
    bytes[14..16].copy_from_slice(&gap.to_le_bytes());
    bytes[16] = cfg.length.min(255) as u8;
    bytes[17] = cfg.dynamic_spread_limit.min(255) as u8;

    let inner = (cfg.inner_split_alpha.clamp(0.0, 1.0) * 100.0).round() as u32;
    let outer = (cfg.outer_split_alpha.clamp(0.3, 1.0) * 100.0).round() as u32 - 30;
    let ratio = (cfg.split_size_ratio.clamp(0.0, 1.0) * 100.0).round() as u32;
    let bits = cfg.split_distance.min(127)
        | (inner.min(100) << 7)
        | (outer.min(70) << 14)
        | (ratio.min(100) << 21)
        | (u32::from(cfg.scope_dot_color_match) << 28);
    bytes[18..22].copy_from_slice(&bits.to_le_bytes());

    let scope_steps = (cfg.scope_dot_scale.clamp(0.1, 2.0) * 100.0).round() as i32 - 10;
    bytes[22] = scope_steps.clamp(0, 255) as u8;

    let checksum: u32 = bytes[1..].iter().map(|b| *b as u32).sum();
    bytes[0] = (checksum % 256) as u8;

    format!("{CS2_PREFIX}{}", bytes_to_chars(&bytes, CS2_CHAR_COUNT))
}

fn decode_legacy(code: &str) -> Result<Decoded, ShareError> {
    let rest = code
        .strip_prefix("CSGO-")
        .ok_or(ShareError::InvalidFormat)?;
    let groups: Vec<&str> = rest.split('-').collect();
    if groups.len() != 5 || groups.iter().any(|group| group.len() != 5) {
        return Err(ShareError::InvalidFormat);
    }
    let chars = groups.concat();
    let bytes = chars_to_bytes(&chars, LEGACY_BYTE_COUNT)?;
    if !checksum_ok(&bytes) {
        return Err(ShareError::InvalidChecksum);
    }

    let (cfg, format) = match bytes[1] {
        1 => (legacy_v1(&bytes), ShareFormat::LegacyV1),
        3 => (legacy_v3(&bytes, false), ShareFormat::LegacyV3),
        4 => (legacy_v3(&bytes, true), ShareFormat::LegacyV4),
        _ => return Err(ShareError::UnsupportedVersion),
    };

    Ok(Decoded {
        config: cfg.sanitized(),
        format,
    })
}

fn legacy_v1(bytes: &[u8]) -> Config {
    let gap = (bytes[2] as i8) as f32 / 10.0;
    let outline_enabled = bytes[10] & 8 != 0;
    let alpha_enabled = (bytes[13] >> 4) & 4 != 0;
    let style = (bytes[13] & 0xf) >> 1;
    let mapped_style = match style {
        0 => 0,
        1 | 4 => 4,
        2 => 2,
        3 => 0,
        _ => 4,
    };

    Config {
        style: mapped_style,
        follow_recoil: (bytes[8] >> 4) & 8 == 8,
        center_dot: (bytes[13] >> 4) & 1 == 1,
        t_style: (bytes[13] >> 4) & 8 == 8,
        outline: u8::from(outline_enabled),
        color: Rgba::new(
            bytes[4],
            bytes[5],
            bytes[6],
            if alpha_enabled { bytes[7] } else { 255 },
        ),
        outline_color: Rgba::new(0, 0, 0, 255),
        gap: gap.round() as i32,
        length: (bytes[14] as f32 / 10.0).round() as u32,
        thickness: (bytes[12] as f32 / 10.0).round().clamp(0.0, 32.0) as u32,
        screen_height: 1080,
        dynamic_spread_limit: 255,
        split_distance: (bytes[8] & 7) as u32,
        inner_split_alpha: ((bytes[10] >> 4) as f32 / 10.0).clamp(0.0, 1.0),
        outer_split_alpha: ((bytes[11] & 0xf) as f32 / 10.0).clamp(0.3, 1.0),
        split_size_ratio: ((bytes[11] >> 4) as f32 / 10.0).clamp(0.0, 1.0),
        ..Config::default()
    }
}

fn legacy_v3(bytes: &[u8], v4: bool) -> Config {
    let bits = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);
    Config {
        style: (bytes[2] & 0xf).min(9),
        follow_recoil: bytes[2] & 0x10 != 0,
        center_dot: bytes[2] & 0x40 != 0,
        t_style: bytes[2] & 0x80 != 0,
        outline: if v4 {
            ((bytes[13] >> 4) & 3).min(2)
        } else if bytes[2] & 0x20 != 0 {
            1
        } else {
            0
        },
        color: Rgba::new(bytes[3], bytes[4], bytes[5], bytes[6]),
        outline_color: Rgba::new(0, 0, 0, 255),
        gap: bytes[7] as i32,
        length: bytes[8] as u32,
        thickness: (bits >> 23) as u32 & 0x1f,
        screen_height: (u16::from_le_bytes([bytes[14], bytes[15]]) as u32).max(240),
        dynamic_spread_limit: bytes[9] as u32,
        split_distance: (bits & 0x7f) as u32,
        inner_split_alpha: (((bits >> 7) & 0x1f) as f32 / 20.0).clamp(0.0, 1.0),
        outer_split_alpha: (((((bits >> 12) & 0xf) as f32) + 6.0) / 20.0).clamp(0.3, 1.0),
        split_size_ratio: (((bits >> 16) & 0x7f) as f32 / 100.0).clamp(0.0, 1.0),
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_is_valid() {
        assert_eq!(DICTIONARY.len(), 57);
    }

    #[test]
    fn base57_roundtrip() {
        let bytes: Vec<u8> = (0..32).map(|i| (i * 7 + 3) as u8).collect();
        let chars = bytes_to_chars(&bytes, 44);
        assert_eq!(chars.len(), 44);
        let decoded = chars_to_bytes(&chars, 32).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn decodes_real_cs2_code() {
        let decoded = decode("CSvbPubOq37zTGqtsPTP5QTrp5CB4xFXiKRLfzJsm49ZRe").unwrap();
        assert_eq!(decoded.format, ShareFormat::Cs2V1);
        let cfg = decoded.config;
        assert_eq!(cfg.style, 2);
        assert_eq!(cfg.outline, 0);
        assert_eq!(cfg.color, Rgba::new(255, 0, 0, 255));
        assert_eq!(cfg.gap, 0);
        assert_eq!(cfg.length, 5);
        assert_eq!(cfg.thickness, 1);
        assert_eq!(cfg.screen_height, 768);
        assert!(cfg.center_dot);
        assert_eq!(cfg.split_distance, 3);
        assert_eq!(cfg.dynamic_spread_limit, 181);
        assert_eq!(cfg.inner_split_alpha, 1.0);
        assert_eq!(cfg.outer_split_alpha, 0.35);
        assert_eq!(cfg.scope_dot_scale, 1.0);
    }

    #[test]
    fn reencodes_real_cs2_codes() {
        for code in [
            "CSvbPubOq37zTGqtsPTP5QTrp5CB4xFXiKRLfzJsm49ZRe",
            "CSoYQNXCQtL5b44fPZKwPPZZCUpsywopyvJW3koh75uFih",
            "CSHt3ObzKS3ikGJLRBVrKjPUrhOCikyke53na8cJFtNZKa",
            "CSAB6Oby2JUDOrBRy4jRcsR6JOQdhbszKF9GuKQGhKVwAc",
        ] {
            let cfg = decode(code).unwrap().config;
            assert_eq!(encode_cs2(&cfg), code);
        }
    }

    #[test]
    fn rejects_bad_checksum() {
        assert_eq!(
            decode("CSkSCHQKunGr2yPsJr2U3RTd8ycMA2pEs9jWnxK5KjMmWL").unwrap_err(),
            ShareError::InvalidChecksum
        );
    }

    #[test]
    fn rejects_invalid_codes() {
        assert!(decode("hello").is_err());
        assert!(decode("CSGO-12345-12345-12345-12345-12345").is_err());
        assert!(decode("CSuZhVyU8icUwjTjopBSVi6ctEbq5GiOXJBRdC6LNLKaFL").is_err());
    }

    #[test]
    fn decodes_legacy_v1() {
        let decoded = decode("CSGO-Cn37R-YE7vo-pLCAL-aURmZ-z6zkG").unwrap();
        assert_eq!(decoded.format, ShareFormat::LegacyV1);
        let cfg = decoded.config;
        assert_eq!(cfg.style, 2);
        assert_eq!(cfg.gap, -1);
        assert_eq!(cfg.length, 5);
        assert_eq!(cfg.thickness, 1);
        assert_eq!(cfg.color, Rgba::new(175, 81, 213, 137));
        assert_eq!(cfg.outline, 1);
        assert!(cfg.center_dot);
        assert!(cfg.t_style);
        assert_eq!(cfg.split_distance, 6);
    }

    #[test]
    fn decodes_legacy_v3_and_v4() {
        let v3 = decode("CSGO-MnUCC-89iG7-2cVar-wy7Yn-amCpF").unwrap();
        assert_eq!(v3.format, ShareFormat::LegacyV3);
        assert_eq!(v3.config.style, 6);
        assert_eq!(v3.config.color, Rgba::new(252, 15, 192, 255));
        assert_eq!(v3.config.gap, 4);
        assert_eq!(v3.config.length, 8);
        assert_eq!(v3.config.thickness, 3);
        assert_eq!(v3.config.screen_height, 1080);
        assert_eq!(v3.config.outer_split_alpha, 0.45);

        let v4 = decode("CSGO-G8oAC-RyvWc-Hi3CZ-voSwn-QJbfE").unwrap();
        assert_eq!(v4.format, ShareFormat::LegacyV4);
        assert_eq!(v4.config.style, 8);
        assert_eq!(v4.config.outline, 1);
        assert_eq!(v4.config.gap, 25);
        assert_eq!(v4.config.length, 7);
        assert_eq!(v4.config.thickness, 20);
        assert_eq!(v4.config.split_distance, 3);
    }

    #[test]
    fn roundtrips_custom_config() {
        let cfg = Config {
            style: 9,
            follow_recoil: true,
            center_dot: true,
            t_style: true,
            outline: 2,
            color: Rgba::new(10, 20, 30, 200),
            outline_color: Rgba::new(40, 50, 60, 128),
            gap: -7,
            length: 255,
            thickness: 12,
            screen_height: 1440,
            dynamic_spread_limit: 64,
            split_distance: 127,
            inner_split_alpha: 0.29,
            outer_split_alpha: 0.97,
            split_size_ratio: 0.63,
            scope_dot_scale: 2.0,
            scope_dot_color_match: true,
            ..Config::default()
        };
        let decoded = decode(&encode_cs2(&cfg)).unwrap().config;
        assert_eq!(decoded.style, cfg.style);
        assert_eq!(decoded.gap, cfg.gap);
        assert_eq!(decoded.length, cfg.length);
        assert_eq!(decoded.thickness, cfg.thickness);
        assert_eq!(decoded.screen_height, cfg.screen_height);
        assert_eq!(decoded.color, cfg.color);
        assert_eq!(decoded.outline_color, cfg.outline_color);
        assert_eq!(decoded.split_distance, cfg.split_distance);
        assert_eq!(decoded.inner_split_alpha, cfg.inner_split_alpha);
        assert_eq!(decoded.outer_split_alpha, cfg.outer_split_alpha);
        assert_eq!(decoded.split_size_ratio, cfg.split_size_ratio);
        assert_eq!(decoded.scope_dot_scale, cfg.scope_dot_scale);
        assert!(decoded.scope_dot_color_match);
        assert!(decoded.follow_recoil);
    }
}
