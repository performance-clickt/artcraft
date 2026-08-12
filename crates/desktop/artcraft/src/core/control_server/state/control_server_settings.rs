use rand::RngCore;
use std::fmt::Write;

const CONTROL_SERVER_TOKEN_BYTE_LENGTH: usize = 32;

/// The bound port and the per-launch bearer token for the embedded control server.
/// NB: The token is a secret shared only through the 0600 discovery file — never log it.
#[derive(Clone)]
pub struct ControlServerSettings {
  port: u16,
  token: String,
}

impl ControlServerSettings {
  pub fn new_with_generated_token(port: u16) -> Self {
    Self {
      port,
      token: generate_control_server_token(),
    }
  }

  pub fn port(&self) -> u16 {
    self.port
  }

  pub fn token(&self) -> &str {
    &self.token
  }
}

fn generate_control_server_token() -> String {
  let mut bytes = [0u8; CONTROL_SERVER_TOKEN_BYTE_LENGTH];
  rand::rng().fill_bytes(&mut bytes);

  let mut token = String::with_capacity(CONTROL_SERVER_TOKEN_BYTE_LENGTH * 2);
  for byte in bytes {
    let _ = write!(token, "{:02x}", byte);
  }

  token
}

#[cfg(test)]
mod tests {
  use super::*;

  const TEST_PORT: u16 = 51234;

  #[test]
  fn test_generated_token_is_32_bytes_of_hex() {
    let settings = ControlServerSettings::new_with_generated_token(TEST_PORT);

    assert_eq!(settings.port(), TEST_PORT);
    assert_eq!(settings.token().len(), CONTROL_SERVER_TOKEN_BYTE_LENGTH * 2);
    assert!(settings.token().chars().all(|c| c.is_ascii_hexdigit()));
  }

  #[test]
  fn test_generated_tokens_differ_per_launch() {
    let first = ControlServerSettings::new_with_generated_token(TEST_PORT);
    let second = ControlServerSettings::new_with_generated_token(TEST_PORT);

    assert_ne!(first.token(), second.token());
  }
}
