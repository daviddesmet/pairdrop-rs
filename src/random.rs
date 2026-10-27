//! Random tokens, ported from `randomizer` in upstream's `server/helper.js`.

use rand::Rng;

/// The printable subset upstream keeps: `-`, `/`, digits, `@`, A-Z, a-z.
const TOKEN_ALPHABET: &[u8] = b"-/0123456789@ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Room secrets: 256 characters, validated client-side against `^[\x00-\x7F]{64,256}$`.
pub fn room_secret() -> String {
    string_from(TOKEN_ALPHABET, 256)
}

/// Public room ids: five lowercase letters.
pub fn public_room_id() -> String {
    string_from(LETTERS, 5).to_lowercase()
}

/// Pair keys: six digits, leading zeros kept.
pub fn pair_key() -> String {
    format!("{:06}", rand::rng().random_range(0..1_000_000))
}

fn string_from(alphabet: &[u8], length: usize) -> String {
    let mut rng = rand::rng();
    (0..length)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{pair_key, public_room_id, room_secret};

    #[test]
    fn tokens_have_the_shapes_the_client_expects() {
        let secret = room_secret();
        assert_eq!(secret.chars().count(), 256);
        assert!(secret.is_ascii());
        assert_ne!(secret, room_secret());

        let id = public_room_id();
        assert_eq!(id.len(), 5);
        assert!(id.chars().all(|c| c.is_ascii_lowercase()));

        let key = pair_key();
        assert_eq!(key.len(), 6);
        assert!(key.chars().all(|c| c.is_ascii_digit()));
    }
}
