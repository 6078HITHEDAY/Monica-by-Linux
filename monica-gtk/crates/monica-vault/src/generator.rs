//! Password generator matching Monica.Core `PasswordGeneratorService`.

use secrecy::SecretString;

const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.?";

const MIN_LENGTH: usize = 8;
const MAX_LENGTH: usize = 128;
const DEFAULT_LENGTH: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorOptions {
    pub length: usize,
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self {
            length: DEFAULT_LENGTH,
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordStrength {
    pub score: u8,
    pub label: &'static str,
}

pub fn generate_password(options: GeneratorOptions) -> SecretString {
    let length = options.length.clamp(MIN_LENGTH, MAX_LENGTH);
    let mut groups: Vec<&[u8]> = Vec::new();
    if options.uppercase {
        groups.push(UPPERCASE);
    }
    if options.lowercase {
        groups.push(LOWERCASE);
    }
    if options.digits {
        groups.push(DIGITS);
    }
    if options.symbols {
        groups.push(SYMBOLS);
    }
    if groups.is_empty() {
        groups.push(LOWERCASE);
    }

    let alphabet_len: usize = groups.iter().map(|group| group.len()).sum();
    let mut out = Vec::with_capacity(length);
    for group in &groups {
        out.push(group[random_index(group.len())]);
    }
    while out.len() < length {
        let mut pick = random_index(alphabet_len);
        for group in &groups {
            if pick < group.len() {
                out.push(group[pick]);
                break;
            }
            pick -= group.len();
        }
    }
    shuffle(&mut out);
    out.truncate(length);
    SecretString::from(String::from_utf8(out).unwrap_or_default())
}

pub fn analyze_password(password: &str) -> PasswordStrength {
    let mut score = 0_u8;
    if password.len() >= 12 {
        score += 1;
    }
    let has_lower = password.chars().any(|ch| ch.is_ascii_lowercase());
    let has_upper = password.chars().any(|ch| ch.is_ascii_uppercase());
    if has_lower && has_upper {
        score += 1;
    }
    if password.chars().any(|ch| ch.is_ascii_digit()) {
        score += 1;
    }
    if password.chars().any(|ch| !ch.is_ascii_alphanumeric()) {
        score += 1;
    }
    if password.len() >= 20 {
        score += 1;
    }
    let label = match score {
        5 => "极强",
        4 => "强",
        3 => "中",
        2 => "弱",
        _ => "很弱",
    };
    PasswordStrength { score, label }
}

fn random_u32() -> u32 {
    let mut bytes = [0_u8; 4];
    getrandom::getrandom(&mut bytes).expect("OS RNG");
    u32::from_le_bytes(bytes)
}

fn random_index(len: usize) -> usize {
    debug_assert!(len > 0);
    let bound = len as u32;
    let max = u32::MAX - (u32::MAX % bound);
    loop {
        let value = random_u32();
        if value < max {
            return (value % bound) as usize;
        }
    }
}

fn shuffle(bytes: &mut [u8]) {
    for index in (1..bytes.len()).rev() {
        let swap = random_index(index + 1);
        bytes.swap(index, swap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn default_options_cover_all_charsets() {
        let secret = generate_password(GeneratorOptions::default());
        let password = secret.expose_secret();
        assert_eq!(password.len(), DEFAULT_LENGTH);
        assert!(password.chars().any(|ch| ch.is_ascii_uppercase()));
        assert!(password.chars().any(|ch| ch.is_ascii_lowercase()));
        assert!(password.chars().any(|ch| ch.is_ascii_digit()));
        assert!(password.chars().any(|ch| !ch.is_ascii_alphanumeric()));
    }

    #[test]
    fn empty_charset_falls_back_to_lowercase() {
        let secret = generate_password(GeneratorOptions {
            length: 12,
            uppercase: false,
            lowercase: false,
            digits: false,
            symbols: false,
        });
        let password = secret.expose_secret();
        assert_eq!(password.len(), 12);
        assert!(password.chars().all(|ch| ch.is_ascii_lowercase()));
    }

    #[test]
    fn strength_labels_match_score() {
        assert_eq!(analyze_password("short").label, "很弱");
        assert_eq!(analyze_password("Abcdefghijk1!").label, "强");
        assert_eq!(analyze_password("Abcdefghijk1!xxxxzzz").label, "极强");
    }
}
