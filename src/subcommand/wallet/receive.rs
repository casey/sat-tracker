use super::*;

#[derive(Deserialize, Serialize)]
pub struct Output {
  pub address: Address<NetworkUnchecked>,
}

pub(crate) fn run(wallet_name: String, options: Options) -> SubcommandResult {
  let wallet_client = bitcoin_rpc_client_for_wallet_command(wallet_name, &options)?;

  let address =
    wallet_client.get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32m))?;

  Ok(Box::new(Output { address }))
}
