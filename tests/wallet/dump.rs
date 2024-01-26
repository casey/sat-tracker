use super::*;

#[test]
fn dumped_descriptors_match_wallet() {
  let bitcoin_rpc_server = test_bitcoincore_rpc::spawn();
  let ord_rpc_server = TestServer::spawn(&bitcoin_rpc_server);

  create_wallet(&bitcoin_rpc_server, &ord_rpc_server);

  let output = CommandBuilder::new("wallet dump")
    .bitcoin_rpc_server(&bitcoin_rpc_server)
    .stderr_regex(".*")
    .run_and_deserialize_output::<BitcoinCoreDescriptors>();

  assert!(bitcoin_rpc_server
    .descriptors()
    .iter()
    .zip(output.descriptors.iter())
    .all(|(wallet_descriptor, output_descriptor)| *wallet_descriptor == output_descriptor.desc));
}

#[test]
fn dumped_descriptors_restore() {
  let bitcoin_rpc_server = test_bitcoincore_rpc::spawn();
  let ord_rpc_server = TestServer::spawn(&bitcoin_rpc_server);

  create_wallet(&bitcoin_rpc_server, &ord_rpc_server);

  let output = CommandBuilder::new("wallet dump")
    .bitcoin_rpc_server(&bitcoin_rpc_server)
    .stderr_regex(".*")
    .run_and_deserialize_output::<BitcoinCoreDescriptors>();

  let bitcoin_rpc_server = test_bitcoincore_rpc::spawn();

  CommandBuilder::new("wallet restore --from-descriptors")
    .stdin(serde_json::to_string(&output).unwrap().as_bytes().to_vec())
    .bitcoin_rpc_server(&bitcoin_rpc_server)
    .run_and_extract_stdout();

  assert!(bitcoin_rpc_server
    .descriptors()
    .iter()
    .zip(output.descriptors.iter())
    .all(|(wallet_descriptor, output_descriptor)| *wallet_descriptor == output_descriptor.desc));
}

#[test]
fn dump_and_restore_descriptors_with_compact() {
  let bitcoin_rpc_server = test_bitcoincore_rpc::spawn();
  let ord_rpc_server = TestServer::spawn(&bitcoin_rpc_server);

  create_wallet(&bitcoin_rpc_server, &ord_rpc_server);

  let output = CommandBuilder::new("--compact wallet dump")
    .bitcoin_rpc_server(&bitcoin_rpc_server)
    .stderr_regex(".*")
    .run_and_deserialize_output::<BitcoinCoreDescriptors>();

  let bitcoin_rpc_server = test_bitcoincore_rpc::spawn();

  CommandBuilder::new("wallet restore --from-descriptors")
    .stdin(serde_json::to_string(&output).unwrap().as_bytes().to_vec())
    .bitcoin_rpc_server(&bitcoin_rpc_server)
    .run_and_extract_stdout();

  assert!(bitcoin_rpc_server
    .descriptors()
    .iter()
    .zip(output.descriptors.iter())
    .all(|(wallet_descriptor, output_descriptor)| *wallet_descriptor == output_descriptor.desc));
}
