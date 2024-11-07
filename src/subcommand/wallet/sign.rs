use {
  super::*,
  base64::{engine::general_purpose, Engine},
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
  pub address: Address<NetworkUnchecked>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub inscription: Option<InscriptionId>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub output: Option<OutPoint>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
  pub witness: String,
}

#[derive(Debug, Parser)]
#[clap(
group(
  ArgGroup::new("input")
    .required(true)
    .args(&["message", "file"])),
group(
  ArgGroup::new("signer")
    .required(true)
    .args(&["address", "inscription", "output"]))
)]
pub(crate) struct Sign {
  #[arg(long, help = "Sign for <ADDRESS>.")]
  address: Option<Address<NetworkUnchecked>>,
  #[arg(long, help = "Sign for <INSCRIPTION>.")]
  inscription: Option<InscriptionId>,
  #[arg(long, help = "Sign for <UTXO>.")]
  output: Option<OutPoint>,
  #[arg(long, help = "Sign <MESSAGE>.")]
  message: Option<String>,
  #[arg(long, help = "Sign contents of <FILE>.")]
  file: Option<PathBuf>,
}

impl Sign {
  pub(crate) fn run(&self, wallet: Wallet) -> SubcommandResult {
    let address = if let Some(address) = &self.address {
      address.clone().require_network(wallet.chain().network())?
    } else if let Some(inscription) = &self.inscription {
      Address::from_str(
        &wallet
          .inscription_info()
          .get(inscription)
          .ok_or_else(|| anyhow!("inscription {inscription} not in wallet"))?
          .address
          .clone()
          .ok_or_else(|| anyhow!("inscription {inscription} in an output without address"))?,
      )?
      .require_network(wallet.chain().network())?
    } else if let Some(output) = self.output {
      wallet.chain().address_from_script(
        &wallet
          .utxos()
          .get(&output)
          .ok_or_else(|| anyhow!("output {output} has no address"))?
          .script_pubkey,
      )?
    } else {
      unreachable!()
    };

    let message = if let Some(message) = &self.message {
      message.as_bytes()
    } else if let Some(file) = &self.file {
      &fs::read(file)?
    } else {
      unreachable!()
    };

    let to_spend = bip322::create_to_spend(&address, message)?;

    let to_sign = bip322::create_to_sign(&to_spend, None)?;

    let result = wallet.bitcoin_client().sign_raw_transaction_with_wallet(
      &to_sign.extract_tx()?,
      Some(&[bitcoincore_rpc::json::SignRawTransactionInput {
        txid: to_spend.compute_txid(),
        vout: 0,
        script_pub_key: address.script_pubkey(),
        redeem_script: None,
        amount: Some(Amount::ZERO),
      }]),
      None,
    )?;

    let mut buffer = Vec::new();

    Transaction::consensus_decode(&mut result.hex.as_slice())?.input[0]
      .witness
      .consensus_encode(&mut buffer)?;

    Ok(Some(Box::new(Output {
      address: address.as_unchecked().clone(),
      inscription: self.inscription,
      output: self.output,
      message: self.message.clone(),
      witness: general_purpose::STANDARD.encode(buffer),
    })))
  }
}
