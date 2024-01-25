use super::*;

#[derive(Debug, Parser)]
#[clap(group(ArgGroup::new("restore_source").required(true).args(&["from_descriptors", "from_mnemonic"])))]
pub(crate) struct Restore {
  #[arg(long, conflicts_with_all = &["from_mnemonic", "passphrase"], help = "Restore wallet from a Bitcoin Core <DESCRIPTORS>.")]
  from_descriptors: bool,
  #[arg(long, help = "Restore wallet from <MNEMONIC>.")]
  from_mnemonic: Option<Mnemonic>,
  #[arg(
    long,
    default_value = "",
    help = "Use <PASSPHRASE> when deriving wallet"
  )]
  pub(crate) passphrase: String,
}

impl Restore {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    if wallet.bitcoin_client().is_ok() {
      bail!(
        "cannot restore because wallet named `{}` already exists",
        wallet.name
      );
    }

    match (self.from_descriptors, self.from_mnemonic) {
      (true, None) => {
        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer)?;

        let descriptors: Vec<BitcoinCoreDescriptor> = serde_json::from_str(&buffer)?;

        wallet.initialize_from_descriptors(
          descriptors
            .into_iter()
            .map(|desc| desc.into_inner())
            .collect(),
        )?;
      }
      (false, Some(mnemonic)) => {
        wallet.initialize(mnemonic.to_seed(self.passphrase))?;
      }
      _ => {
        bail!("either a descriptor or a mnemonic is required.");
      }
    }

    Ok(None)
  }
}
