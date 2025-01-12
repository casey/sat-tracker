use {
  super::*,
  serde_hex::{SerHex, Strict},
};

#[derive(DeserializeFromStr)]
pub(crate) enum State {
  Error,
  Sealed,
  Unsealed,
}

impl Display for State {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Error => write!(f, "error"),
      Self::Sealed => write!(f, "sealed"),
      Self::Unsealed => write!(f, "unsealed"),
    }
  }
}

pub(crate) struct StateError(String);

impl Display for StateError {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(f, "invalid state: {}", self.0)
  }
}

impl FromStr for State {
  type Err = StateError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "E" => Ok(Self::Error),
      "S" => Ok(Self::Sealed),
      "U" => Ok(Self::Unsealed),
      _ => Err(StateError(s.into())),
    }
  }
}

pub(crate) enum SignatureError {
  Hex(hex::FromHexError),
  Signature(secp256k1::Error),
}

impl Display for SignatureError {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Hex(err) => write!(f, "invalid signature hex: {err}"),
      Self::Signature(err) => write!(f, "invalid signature: {err}"),
    }
  }
}

#[derive(DeserializeFromStr)]
pub(crate) struct Signature(secp256k1::ecdsa::Signature);

impl FromStr for Signature {
  type Err = SignatureError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(Self(
      secp256k1::ecdsa::Signature::from_compact(&hex::decode(s).map_err(SignatureError::Hex)?)
        .map_err(SignatureError::Signature)?,
    ))
  }
}

impl Display for Signature {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(f, "{}", hex::encode(&self.0.serialize_compact()))
  }
}

#[derive(Deserialize)]
pub(crate) struct Query {
  #[serde(rename = "u")]
  pub(crate) state: State,
  #[serde(rename = "o")]
  pub(crate) slot: u64,
  #[serde(rename = "r")]
  pub(crate) address_suffix: String,
  #[serde(rename = "n", with = "SerHex::<Strict>")]
  pub(crate) nonce: [u8; 8],
  #[serde(rename = "s")]
  pub(crate) signature: Signature,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn query_deserialize() {
    // test string:
    // ?u=S
    // &o=0
    // &r=a5x2tplf
    // &n=7664168a4ef7b8e8
    // &s=42b209c86ab90be6418d36b0accc3a53c11901861b55be95b763799842d403dc17cd1b74695a7ffe2d78965535d6fe7f6aafc77f6143912a163cb65862e8fb53
    todo!()
  }
}
