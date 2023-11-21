use super::*;

#[derive(Clone)]
pub(crate) struct PageConfig {
  pub(crate) chain: Chain,
  pub(crate) domain: Option<String>,
  pub(crate) index_sats: bool,
  pub(crate) content_security_policy_origin: Option<String>,
}

impl Default for PageConfig {
  fn default() -> Self {
    Self {
      chain: Chain::Mainnet,
      domain: None,
      index_sats: false,
      content_security_policy_origin: None,
    }
  }
}
