use {
  super::*,
  ord::subcommand::wallet::{create, inscriptions, receive},
  std::ops::Deref,
};

#[test]
fn inscribe_creates_inscriptions() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  core.mine_blocks(1);

  assert_eq!(core.descriptors().len(), 0);

  create_wallet(&core, &ord);

  let (inscription, _) = inscribe(&core, &ord);

  assert_eq!(core.descriptors().len(), 3);

  let request = ord.request(format!("/content/{inscription}"));

  assert_eq!(request.status(), 200);
  assert_eq!(
    request.headers().get("content-type").unwrap(),
    "text/plain;charset=utf-8"
  );
  assert_eq!(request.text().unwrap(), "FOO");
}

#[test]
fn inscribe_works_with_huge_expensive_inscriptions() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks(1)[0].txdata[0].compute_txid();

  CommandBuilder::new(format!(
    "wallet inscribe --file foo.txt --satpoint {txid}:0:0 --fee-rate 10"
  ))
  .write("foo.txt", [0; 350_000])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();
}

#[test]
fn metaprotocol_appears_on_inscription_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks(1)[0].txdata[0].compute_txid();

  let inscribe = CommandBuilder::new(format!(
    "wallet inscribe --file foo.txt --metaprotocol foo --satpoint {txid}:0:0 --fee-rate 10"
  ))
  .write("foo.txt", [0; 350_000])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{}", inscribe.inscriptions[0].id),
    r".*<dt>metaprotocol</dt>\s*<dd>foo</dd>.*",
  );
}

#[test]
fn inscribe_fails_if_bitcoin_core_is_too_old() {
  let core = mockcore::builder().version(240000).build();
  let ord = TestServer::spawn(&core);

  CommandBuilder::new("wallet inscribe --file hello.txt --fee-rate 1")
    .write("hello.txt", "HELLOWORLD")
    .expected_exit_code(1)
    .expected_stderr("error: Bitcoin Core 25.0.0 or newer required, current version is 24.0.0\n")
    .core(&core)
    .ord(&ord)
    .run_and_extract_stdout();
}

#[test]
fn inscribe_no_backup() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  core.mine_blocks(1);

  create_wallet(&core, &ord);

  assert_eq!(core.descriptors().len(), 2);

  CommandBuilder::new("wallet inscribe --file hello.txt --no-backup --fee-rate 1")
    .write("hello.txt", "HELLOWORLD")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  assert_eq!(core.descriptors().len(), 2);
}

#[test]
fn inscribe_unknown_file_extension() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --file pepe.xyz --fee-rate 1")
    .write("pepe.xyz", [1; 520])
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .stderr_regex(r"error: unsupported file extension `\.xyz`, supported extensions: apng .*\n")
    .run_and_extract_stdout();
}

#[test]
fn inscribe_exceeds_chain_limit() {
  let core = mockcore::builder().network(Network::Signet).build();

  let ord = TestServer::spawn_with_args(&core, &["--signet"]);

  create_wallet(&core, &ord);

  CommandBuilder::new("--chain signet wallet inscribe --file degenerate.png --fee-rate 1")
    .write("degenerate.png", [1; 1025])
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .expected_stderr(
      "error: content size of 1025 bytes exceeds 1024 byte limit for signet inscriptions\n",
    )
    .run_and_extract_stdout();
}

#[test]
fn regtest_has_no_content_size_limit() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--regtest"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new("--chain regtest wallet inscribe --file degenerate.png --fee-rate 1")
    .write("degenerate.png", [1; 1025])
    .core(&core)
    .ord(&ord)
    .stdout_regex(".*")
    .run_and_extract_stdout();
}

#[test]
fn mainnet_has_no_content_size_limit() {
  let core = mockcore::builder().network(Network::Bitcoin).build();

  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --file degenerate.png --fee-rate 1")
    .write("degenerate.png", [1; 1025])
    .core(&core)
    .ord(&ord)
    .stdout_regex(".*")
    .run_and_extract_stdout();
}

#[test]
fn inscribe_does_not_use_inscribed_sats_as_cardinal_utxos() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks_with_subsidy(1, 100);

  CommandBuilder::new(
    "wallet inscribe --file degenerate.png --fee-rate 1"
  )
  .core(&core)
  .ord(&ord)
  .write("degenerate.png", [1; 100])
  .expected_exit_code(1)
  .expected_stderr("error: wallet does not contain enough cardinal UTXOs, please add additional funds to wallet.\n")
  .run_and_extract_stdout();
}

#[test]
fn refuse_to_reinscribe_sats() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (_, reveal) = inscribe(&core, &ord);

  core.mine_blocks_with_subsidy(1, 100);

  CommandBuilder::new(format!(
    "wallet inscribe --satpoint {reveal}:0:0 --file hello.txt --fee-rate 1"
  ))
  .write("hello.txt", "HELLOWORLD")
  .core(&core)
  .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr(format!("error: sat at {reveal}:0:0 already inscribed\n"))
  .run_and_extract_stdout();
}

#[test]
fn refuse_to_inscribe_already_inscribed_utxo() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (inscription, reveal) = inscribe(&core, &ord);

  let output = OutPoint {
    txid: reveal,
    vout: 0,
  };

  CommandBuilder::new(format!(
    "wallet inscribe --satpoint {output}:55555 --file hello.txt --fee-rate 1"
  ))
  .write("hello.txt", "HELLOWORLD")
  .core(&core)
  .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr(format!(
    "error: utxo {output} with sat {output}:0 already inscribed with the following inscriptions:\n{inscription}\n",
  ))
  .run_and_extract_stdout();
}

#[test]
fn inscribe_with_optional_satpoint_arg() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks(1)[0].txdata[0].compute_txid();

  let Batch { inscriptions, .. } = CommandBuilder::new(format!(
    "wallet inscribe --file foo.txt --satpoint {txid}:0:10000 --fee-rate 1"
  ))
  .write("foo.txt", "FOO")
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output();
  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.assert_response_regex(
    "/sat/5000010000",
    format!(".*<a href=/inscription/{inscription}>.*"),
  );

  ord.assert_response_regex(format!("/content/{inscription}",), "FOO");

  ord.assert_response_regex(
    format!("/inscription/{}", Sat(5000010000).name()),
    ".*<title>Inscription 0</title>.*",
  );
}

#[test]
fn inscribe_with_fee_rate() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let output =
    CommandBuilder::new("--index-sats wallet inscribe --file degenerate.png --fee-rate 2.0")
      .write("degenerate.png", [1; 520])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Batch>();

  let tx1 = &core.mempool()[0];
  let mut fee = Amount::ZERO;
  for input in &tx1.input {
    fee += core.get_utxo_amount(&input.previous_output).unwrap();
  }
  for output in &tx1.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx1.vsize() as f64;

  pretty_assert_eq!(fee_rate, 2.0);

  let tx2 = &core.mempool()[1];
  let mut fee = Amount::ZERO;
  for input in &tx2.input {
    fee += tx1.output[input.previous_output.vout as usize].value;
  }
  for output in &tx2.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx2.vsize() as f64;

  pretty_assert_eq!(fee_rate, 2.0);
  assert_eq!(
    ord::FeeRate::try_from(2.0)
      .unwrap()
      .fee(tx1.vsize() + tx2.vsize())
      .to_sat(),
    output.total_fees
  );
}

#[test]
fn inscribe_with_commit_fee_rate() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new(
    "--index-sats wallet inscribe --file degenerate.png --commit-fee-rate 2.0 --fee-rate 1",
  )
  .write("degenerate.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  let tx1 = &core.mempool()[0];
  let mut fee = Amount::ZERO;
  for input in &tx1.input {
    fee += core.get_utxo_amount(&input.previous_output).unwrap();
  }
  for output in &tx1.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx1.vsize() as f64;

  pretty_assert_eq!(fee_rate, 2.0);

  let tx2 = &core.mempool()[1];
  let mut fee = Amount::ZERO;
  for input in &tx2.input {
    fee += tx1.output[input.previous_output.vout as usize].value;
  }
  for output in &tx2.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx2.vsize() as f64;

  pretty_assert_eq!(fee_rate, 1.0);
}

#[test]
fn inscribe_with_wallet_named_foo() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  CommandBuilder::new("wallet --name foo create")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<create::Output>();

  core.mine_blocks(1);

  CommandBuilder::new("wallet --name foo inscribe --file degenerate.png --fee-rate 1")
    .write("degenerate.png", [1; 520])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();
}

#[test]
fn inscribe_with_dry_run_flag() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let inscribe =
    CommandBuilder::new("wallet inscribe --dry-run --file degenerate.png --fee-rate 1")
      .write("degenerate.png", [1; 520])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Batch>();

  assert!(inscribe.commit_psbt.is_some());
  assert!(inscribe.reveal_psbt.is_some());

  assert!(core.mempool().is_empty());

  let inscribe = CommandBuilder::new("wallet inscribe --file degenerate.png --fee-rate 1")
    .write("degenerate.png", [1; 520])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  assert!(inscribe.commit_psbt.is_none());
  assert!(inscribe.reveal_psbt.is_none());

  assert_eq!(core.mempool().len(), 2);
}

#[test]
fn inscribe_with_dry_run_flag_fees_increase() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let total_fee_dry_run =
    CommandBuilder::new("wallet inscribe --dry-run --file degenerate.png --fee-rate 1")
      .write("degenerate.png", [1; 520])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Batch>()
      .total_fees;

  let total_fee_normal =
    CommandBuilder::new("wallet inscribe --dry-run --file degenerate.png --fee-rate 1.1")
      .write("degenerate.png", [1; 520])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Batch>()
      .total_fees;

  assert!(total_fee_dry_run < total_fee_normal);
}

#[test]
fn inscribe_to_specific_destination() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let addresses = CommandBuilder::new("wallet receive")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<receive::Output>()
    .addresses;

  let destination = addresses.first().unwrap();

  let txid = CommandBuilder::new(format!(
    "wallet inscribe --destination {} --file degenerate.png --fee-rate 1",
    destination.clone().assume_checked()
  ))
  .write("degenerate.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>()
  .reveal;

  let reveal_tx = &core.mempool()[1]; // item 0 is the commit, item 1 is the reveal.
  assert_eq!(reveal_tx.compute_txid(), txid);
  assert_eq!(
    reveal_tx.output.first().unwrap().script_pubkey,
    destination.assume_checked_ref().script_pubkey()
  );
}

#[test]
fn inscribe_to_address_on_different_network() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new(
    "wallet inscribe --destination tb1qsgx55dp6gn53tsmyjjv4c2ye403hgxynxs0dnm --file degenerate.png --fee-rate 1"
  )
  .write("degenerate.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .expected_exit_code(1)
  .stderr_regex("error: validation error\n\nbecause:\n- address tb1qsgx55dp6gn53tsmyjjv4c2ye403hgxynxs0dnm is not valid on bitcoin\n")
  .run_and_extract_stdout();
}

#[test]
fn inscribe_with_no_limit() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let one_megger = std::iter::repeat(0).take(1_000_000).collect::<Vec<u8>>();
  CommandBuilder::new("wallet inscribe --no-limit --file degenerate.png --fee-rate 1")
    .write("degenerate.png", one_megger)
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();
}

#[test]
fn inscribe_works_with_postage() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);
  core.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --file foo.txt --postage 5btc --fee-rate 10".to_string())
    .write("foo.txt", [0; 350])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  let inscriptions = CommandBuilder::new("wallet inscriptions".to_string())
    .write("foo.txt", [0; 350])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Vec<inscriptions::Output>>();

  pretty_assert_eq!(inscriptions[0].postage, 5 * COIN_VALUE);
}

#[test]
fn inscribe_with_non_existent_parent_inscription() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let parent_id = "0000000000000000000000000000000000000000000000000000000000000000i0";

  CommandBuilder::new(format!(
    "wallet inscribe --fee-rate 1.0 --parent {parent_id} --file child.png"
  ))
  .write("child.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .expected_stderr(format!("error: parent {parent_id} does not exist\n"))
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn inscribe_with_parent_inscription_and_fee_rate() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let parent_output = CommandBuilder::new("wallet inscribe --fee-rate 5.0 --file parent.png")
    .write("parent.png", [1; 520])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  assert_eq!(core.descriptors().len(), 3);
  let parent_id = parent_output.inscriptions[0].id;

  let commit_tx = &core.mempool()[0];
  let reveal_tx = &core.mempool()[1];

  assert_eq!(
    ord::FeeRate::try_from(5.0)
      .unwrap()
      .fee(commit_tx.vsize() + reveal_tx.vsize())
      .to_sat(),
    parent_output.total_fees
  );

  core.mine_blocks(1);

  let child_output = CommandBuilder::new(format!(
    "wallet inscribe --fee-rate 7.3 --parent {parent_id} --file child.png"
  ))
  .write("child.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  assert_eq!(core.descriptors().len(), 4);
  assert_eq!(parent_id, *child_output.parents.first().unwrap());

  let commit_tx = &core.mempool()[0];
  let reveal_tx = &core.mempool()[1];

  assert_eq!(
    ord::FeeRate::try_from(7.3)
      .unwrap()
      .fee(commit_tx.vsize() + reveal_tx.vsize())
      .to_sat(),
    child_output.total_fees
  );

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{}", child_output.parents.first().unwrap()),
    format!(
      ".*<dt>children</dt>.*<a href=/inscription/{}>.*",
      child_output.inscriptions[0].id
    ),
  );

  ord.assert_response_regex(
    format!("/inscription/{}", child_output.inscriptions[0].id),
    format!(
      ".*<dt>parents</dt>.*<a href=/inscription/{}>.*",
      child_output.parents.first().unwrap()
    ),
  );
}

#[test]
fn reinscribe_with_flag() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  core.mine_blocks(1);

  assert_eq!(core.descriptors().len(), 0);

  create_wallet(&core, &ord);

  let inscribe = CommandBuilder::new("wallet inscribe --file tulip.png --fee-rate 5.0 ")
    .write("tulip.png", [1; 520])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  assert_eq!(core.descriptors().len(), 3);

  let txid = core.mine_blocks(1)[0].txdata[2].compute_txid();

  let request = ord.request(format!("/content/{}", inscribe.inscriptions[0].id));

  assert_eq!(request.status(), 200);

  let reinscribe = CommandBuilder::new(format!(
    "wallet inscribe --file orchid.png --fee-rate 1.1 --reinscribe --satpoint {txid}:0:0"
  ))
  .write("orchid.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  let request = ord.request(format!("/content/{}", reinscribe.inscriptions[0].id));

  assert_eq!(request.status(), 200);
  ord.assert_response_regex(
    format!("/sat/{}", 50 * COIN_VALUE),
    format!(
      ".*<dt>inscriptions</dt>.*<a href=/inscription/{}>.*<a href=/inscription/{}>.*",
      inscribe.inscriptions[0].id, reinscribe.inscriptions[0].id
    ),
  );

  let inscriptions = CommandBuilder::new("wallet inscriptions")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Inscriptions>();

  assert_eq!(inscriptions[0].inscription, inscribe.inscriptions[0].id);
  assert_eq!(inscriptions[1].inscription, reinscribe.inscriptions[0].id);
}

#[test]
fn with_reinscribe_flag_but_not_actually_a_reinscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --file tulip.png --fee-rate 5.0 ")
    .write("tulip.png", [1; 520])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  let coinbase = core.mine_blocks(1)[0].txdata[0].compute_txid();

  CommandBuilder::new(format!(
    "wallet inscribe --file orchid.png --fee-rate 1.1 --reinscribe --satpoint {coinbase}:0:0"
  ))
  .write("orchid.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .expected_exit_code(1)
  .stderr_regex("error: reinscribe flag set but this would not be a reinscription.*")
  .run_and_extract_stdout();
}

#[test]
fn try_reinscribe_without_flag() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let reveal_txid = CommandBuilder::new("wallet inscribe --file tulip.png --fee-rate 5.0 ")
    .write("tulip.png", [1; 520])
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>()
    .reveal;

  assert_eq!(core.descriptors().len(), 3);

  core.mine_blocks(1);

  CommandBuilder::new(format!(
    "wallet inscribe --file orchid.png --fee-rate 1.1 --satpoint {reveal_txid}:0:0"
  ))
  .write("orchid.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .expected_exit_code(1)
  .stderr_regex(format!(
    "error: sat at {reveal_txid}:0:0 already inscribed.*"
  ))
  .run_and_extract_stdout();
}

#[test]
fn no_metadata_appears_on_inscription_page_if_no_metadata_is_passed() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let Batch { inscriptions, .. } =
    CommandBuilder::new("wallet inscribe --fee-rate 1 --file content.png")
      .write("content.png", [1; 520])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  assert!(!ord
    .request(format!("/inscription/{inscription}"),)
    .text()
    .unwrap()
    .contains("metadata"));
}

#[test]
fn json_metadata_appears_on_inscription_page() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let Batch { inscriptions, .. } = CommandBuilder::new(
    "wallet inscribe --fee-rate 1 --json-metadata metadata.json --file content.png",
  )
  .write("content.png", [1; 520])
  .write("metadata.json", r#"{"foo": "bar", "baz": 1}"#)
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    ".*<dt>metadata</dt>.*<dl><dt>foo</dt><dd>bar</dd><dt>baz</dt><dd>1</dd></dl>.*",
  );
}

#[test]
fn cbor_metadata_appears_on_inscription_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let Batch { inscriptions, .. } = CommandBuilder::new(
    "wallet inscribe --fee-rate 1 --cbor-metadata metadata.cbor --file content.png",
  )
  .write("content.png", [1; 520])
  .write(
    "metadata.cbor",
    [
      0xA2, 0x63, b'f', b'o', b'o', 0x63, b'b', b'a', b'r', 0x63, b'b', b'a', b'z', 0x01,
    ],
  )
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    ".*<dt>metadata</dt>.*<dl><dt>foo</dt><dd>bar</dd><dt>baz</dt><dd>1</dd></dl>.*",
  );
}

#[test]
fn error_message_when_parsing_json_metadata_is_reasonable() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  CommandBuilder::new(
    "wallet inscribe --fee-rate 1 --json-metadata metadata.json --file content.png",
  )
  .write("content.png", [1; 520])
  .write("metadata.json", "{")
  .core(&core)
  .ord(&ord)
  .stderr_regex(".*failed to parse JSON metadata.*")
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn error_message_when_parsing_cbor_metadata_is_reasonable() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  CommandBuilder::new(
    "wallet inscribe --fee-rate 1 --cbor-metadata metadata.cbor --file content.png",
  )
  .write("content.png", [1; 520])
  .write("metadata.cbor", [0x61])
  .core(&core)
  .ord(&ord)
  .stderr_regex(".*failed to parse CBOR metadata.*")
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn inscribe_does_not_pick_locked_utxos() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let coinbase_tx = &core.mine_blocks(1)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.compute_txid(), 0);

  core.lock(outpoint);

  CommandBuilder::new("wallet inscribe --file hello.txt --fee-rate 1")
    .core(&core)
    .ord(&ord)
    .write("hello.txt", "HELLOWORLD")
    .expected_exit_code(1)
    .stderr_regex("error: wallet contains no cardinal utxos\n")
    .run_and_extract_stdout();
}

#[test]
fn inscribe_can_compress() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let Batch { inscriptions, .. } =
    CommandBuilder::new("wallet inscribe --compress --file foo.txt --fee-rate 1".to_string())
      .write("foo.txt", [0; 350_000])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.sync_server();

  let client = reqwest::blocking::Client::builder()
    .brotli(false)
    .build()
    .unwrap();

  let response = client
    .get(
      ord
        .url()
        .join(format!("/content/{inscription}",).as_ref())
        .unwrap(),
    )
    .send()
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
  assert_regex_match!(
    response.text().unwrap(),
    "inscription content encoding `br` is not acceptable. `Accept-Encoding` header not present"
  );

  let client = reqwest::blocking::Client::builder()
    .brotli(true)
    .build()
    .unwrap();

  let response = client
    .get(
      ord
        .url()
        .join(format!("/content/{inscription}",).as_ref())
        .unwrap(),
    )
    .send()
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.bytes().unwrap().deref(), [0; 350_000]);
}

#[test]
fn inscriptions_are_not_compressed_if_no_space_is_saved_by_compression() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let Batch { inscriptions, .. } =
    CommandBuilder::new("wallet inscribe --compress --file foo.txt --fee-rate 1".to_string())
      .write("foo.txt", "foo")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.sync_server();

  let client = reqwest::blocking::Client::builder()
    .brotli(false)
    .build()
    .unwrap();

  let response = client
    .get(
      ord
        .url()
        .join(format!("/content/{inscription}",).as_ref())
        .unwrap(),
    )
    .send()
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.text().unwrap(), "foo");
}

#[test]
fn inscribe_with_sat_arg() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(2);

  let Batch { inscriptions, .. } = CommandBuilder::new(
    "--index-sats wallet inscribe --file foo.txt --sat 5010000000 --fee-rate 1",
  )
  .write("foo.txt", "FOO")
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.assert_response_regex(
    "/sat/5010000000",
    format!(".*<a href=/inscription/{inscription}>.*"),
  );

  ord.assert_response_regex(format!("/content/{inscription}",), "FOO");
}

#[test]
fn inscribe_with_sat_arg_fails_if_no_index_or_not_found() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  CommandBuilder::new("wallet inscribe --file foo.txt --sat 5010000000 --fee-rate 1")
    .write("foo.txt", "FOO")
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .expected_stderr("error: ord index must be built with `--index-sats` to use `--sat`\n")
    .run_and_extract_stdout();

  CommandBuilder::new("--index-sats wallet inscribe --sat 5000000000 --file foo.txt --fee-rate 1")
    .write("foo.txt", "FOO")
    .core(&core)
    .ord(&TestServer::spawn_with_server_args(
      &core,
      &["--index-sats"],
      &[],
    ))
    .expected_exit_code(1)
    .expected_stderr("error: could not find sat `5000000000` in wallet outputs\n")
    .run_and_extract_stdout();
}

#[test]
fn server_can_decompress_brotli() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let Batch { inscriptions, .. } =
    CommandBuilder::new("wallet inscribe --compress --file foo.txt --fee-rate 1".to_string())
      .write("foo.txt", [0; 350_000])
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output();

  let inscription = inscriptions[0].id;

  core.mine_blocks(1);

  ord.sync_server();

  let client = reqwest::blocking::Client::builder()
    .brotli(false)
    .build()
    .unwrap();

  let response = client
    .get(
      ord
        .url()
        .join(format!("/content/{inscription}",).as_ref())
        .unwrap(),
    )
    .send()
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);

  let test_server = TestServer::spawn_with_server_args(&core, &[], &["--decompress"]);

  test_server.sync_server();

  let client = reqwest::blocking::Client::builder()
    .brotli(false)
    .build()
    .unwrap();

  let response = client
    .get(
      test_server
        .url()
        .join(format!("/content/{inscription}",).as_ref())
        .unwrap(),
    )
    .send()
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.bytes().unwrap().deref(), [0; 350_000]);
}

#[test]
fn file_inscribe_with_delegate_inscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (delegate, _) = inscribe(&core, &ord);

  let inscribe = CommandBuilder::new(format!(
    "wallet inscribe --fee-rate 1.0 --delegate {delegate} --file inscription.txt"
  ))
  .write("inscription.txt", "INSCRIPTION")
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{}", inscribe.inscriptions[0].id),
    format!(r#".*<dt>delegates</dt>\s*<dd><a href=/inscription/{delegate}>{delegate}</a></dd>.*"#,),
  );

  ord.assert_response(format!("/content/{}", inscribe.inscriptions[0].id), "FOO");
}

#[test]
fn file_inscribe_with_only_delegate() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (delegate, _) = inscribe(&core, &ord);

  let inscribe = CommandBuilder::new(format!(
    "wallet inscribe --fee-rate 1.0 --delegate {delegate}"
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{}", inscribe.inscriptions[0].id),
    format!(r#".*<dt>delegates</dt>\s*<dd><a href=/inscription/{delegate}>{delegate}</a></dd>.*"#,),
  );

  ord.assert_response(format!("/content/{}", inscribe.inscriptions[0].id), "FOO");
}

#[test]
fn inscription_with_delegate_returns_effective_content_type() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);
  create_wallet(&core, &ord);

  core.mine_blocks(1);
  let (delegate, _) = inscribe(&core, &ord);

  let inscribe = CommandBuilder::new(format!(
    "wallet inscribe --fee-rate 1.0 --delegate {delegate} --file meow.wav"
  ))
  .write("meow.wav", [0; 2048])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  let inscription_id = inscribe.inscriptions[0].id;
  let json_response = ord.json_request(format!("/inscription/{}", inscription_id));

  let inscription_json: api::Inscription =
    serde_json::from_str(&json_response.text().unwrap()).unwrap();
  assert_regex_match!(inscription_json.address.unwrap(), r"bc1p.*");

  assert_eq!(inscription_json.content_type, Some("audio/wav".to_string()));
  assert_eq!(
    inscription_json.effective_content_type,
    Some("text/plain;charset=utf-8".to_string())
  );
}

#[test]
fn file_inscribe_with_non_existent_delegate_inscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let delegate = "0000000000000000000000000000000000000000000000000000000000000000i0";

  CommandBuilder::new(format!(
    "wallet inscribe --fee-rate 1.0 --delegate {delegate} --file child.png"
  ))
  .write("child.png", [1; 520])
  .core(&core)
  .ord(&ord)
  .expected_stderr(format!("error: delegate {delegate} does not exist\n"))
  .expected_exit_code(1)
  .run_and_extract_stdout();
}
