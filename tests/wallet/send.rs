use super::*;

#[test]
fn inscriptions_can_be_sent() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription}",
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(r".*")
  .run_and_deserialize_output::<Send>();

  let txid = core.mempool()[0].compute_txid();
  assert_eq!(txid, output.txid);

  core.mine_blocks(1);

  let send_txid = output.txid;

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      ".*<h1>Inscription 0</h1>.*<dl>.*
  <dt>content length</dt>
  <dd>3 bytes</dd>
  <dt>content type</dt>
  <dd>text/plain;charset=utf-8</dd>
  .*
  <dt>location</dt>
  <dd><a class=collapse href=/satpoint/{send_txid}:0:0>{send_txid}:0:0</a></dd>
  .*
</dl>
.*",
    ),
  );
}

#[test]
fn send_unknown_inscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks(1)[0].txdata[0].compute_txid();

  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {txid}i0"
  ))
  .core(&core)
  .ord(&ord)
  .expected_stderr(format!("error: inscription {txid}i0 not found\n"))
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn send_inscribed_inscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {inscription}",
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let send_txid = output.txid;

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(".*<h1>Inscription 0</h1>.*<dt>location</dt>.*{send_txid}:0:0</a></dd>.*",),
  );
}

#[test]
fn send_uninscribed_sat() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  let sat = Sat(1);

  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {}",
    sat.name(),
  ))
  .core(&core)
  .ord(&ord)
  .expected_stderr(format!(
    "error: could not find sat `{sat}` in wallet outputs\n"
  ))
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn send_inscription_by_sat() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, txid) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let sat_list = sats(&core, &ord);

  let sat = sat_list.iter().find(|s| s.output.txid == txid).unwrap().sat;

  let address = "bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv";

  let output = CommandBuilder::new(format!("wallet send --fee-rate 1 {address} {}", sat.name()))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let send_txid = output.txid;

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      ".*<h1>Inscription 0</h1>.*<dt>address</dt>.*<dd><a class=collapse href=/address/{address}>{address}</a></dd>.*<dt>location</dt>.*<dd><a class=collapse href=/satpoint/{send_txid}:0:0>{send_txid}:0:0</a></dd>.*",
    ),
  );
}

#[test]
fn send_on_mainnnet_works_with_wallet_named_foo() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  let txid = core.mine_blocks(1)[0].txdata[0].compute_txid();

  CommandBuilder::new("wallet --name foo create")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Create>();

  CommandBuilder::new(format!(
    "wallet --name foo send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {txid}:0:0"
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();
}

#[test]
fn send_addresses_must_be_valid_for_network() {
  let core = mockcore::builder().build();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks_with_subsidy(1, 1_000)[0].txdata[0].compute_txid();

  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 tb1q6en7qjxgw4ev8xwx94pzdry6a6ky7wlfeqzunz {txid}:0:0"
  ))
  .core(&core)
    .ord(&ord)
  .expected_stderr(
    "error: validation error\n\nbecause:\n- address tb1q6en7qjxgw4ev8xwx94pzdry6a6ky7wlfeqzunz is not valid on bitcoin\n",
  )
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn send_on_mainnnet_works_with_wallet_named_ord() {
  let core = mockcore::builder().build();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks_with_subsidy(1, 1_000_000)[0].txdata[0].compute_txid();

  let output = CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {txid}:0:0"
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  assert_eq!(core.mempool()[0].compute_txid(), output.txid);
}

#[test]
fn send_does_not_use_inscribed_sats_as_cardinal_utxos() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let txid = core.mine_blocks_with_subsidy(1, 10_000)[0].txdata[0].compute_txid();
  CommandBuilder::new(format!(
    "wallet inscribe --satpoint {txid}:0:0 --file degenerate.png --fee-rate 0"
  ))
  .write("degenerate.png", [1; 100])
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>();

  let txid = core.mine_blocks_with_subsidy(1, 100)[0].txdata[0].compute_txid();
  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {txid}:0:0"
  ))
  .core(&core)
    .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr("error: wallet does not contain enough cardinal UTXOs, please add additional funds to wallet.\n")
  .run_and_extract_stdout();
}

#[test]
fn do_not_send_within_dust_limit_of_an_inscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let (inscription, reveal) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = OutPoint {
    txid: reveal,
    vout: 0,
  };

  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {output}:329"
  ))
  .core(&core)
  .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr(format!(
    "error: cannot send {output}:329 without also sending inscription {inscription} at {output}:0\n"
  ))
  .run_and_extract_stdout();
}

#[test]
fn can_send_after_dust_limit_from_an_inscription() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let (_, reveal) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = OutPoint {
    txid: reveal,
    vout: 0,
  };

  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {output}:330"
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();
}

#[test]
fn splitting_merged_inscriptions_is_possible() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let inscribe = CommandBuilder::new("wallet batch --fee-rate 0 --batch batch.yaml")
    .write("inscription.txt", "INSCRIPTION")
    .write(
      "batch.yaml",
      "\
mode: shared-output

inscriptions:
- file: inscription.txt
- file: inscription.txt
- file: inscription.txt
",
    )
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  let reveal_txid = inscribe.reveal;

  let destination = inscribe.inscriptions[0].destination.clone();

  core.mine_blocks(1);

  let response = ord.json_request(format!("/output/{}:0", reveal_txid));
  assert_eq!(response.status(), StatusCode::OK);

  let output_json: api::Output = serde_json::from_str(&response.text().unwrap()).unwrap();

  pretty_assert_eq!(
    output_json,
    api::Output {
      address: Some(destination.clone()),
      outpoint: OutPoint {
        txid: reveal_txid,
        vout: 0
      },
      inscriptions: Some(vec![
        InscriptionId {
          txid: reveal_txid,
          index: 0
        },
        InscriptionId {
          txid: reveal_txid,
          index: 1
        },
        InscriptionId {
          txid: reveal_txid,
          index: 2
        },
      ]),
      indexed: true,
      runes: None,
      sat_ranges: Some(vec![(5_000_000_000, 5_000_030_000)]),
      script_pubkey: destination.assume_checked_ref().script_pubkey(),
      spent: false,
      transaction: reveal_txid,
      value: 30_000,
    }
  );

  // try and fail to send first
  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {}i0",
    reveal_txid,
  ))
  .core(&core)
    .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr(format!(
    "error: cannot send {reveal_txid}:0:0 without also sending inscription {reveal_txid}i2 at {reveal_txid}:0:20000\n",
  ))
  .run_and_extract_stdout();

  // splitting out last
  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {}i2",
    reveal_txid,
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  // splitting second to last
  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {}i1",
    reveal_txid,
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  // splitting send first
  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {}i0",
    reveal_txid,
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();
}

#[test]
fn inscriptions_cannot_be_sent_by_satpoint() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let (_, reveal) = inscribe(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {reveal}:0:0"
  ))
  .core(&core)
  .ord(&ord)
  .expected_stderr("error: inscriptions must be sent by inscription ID\n")
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn send_btc_with_fee_rate() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new(
    "wallet send --fee-rate 13.3 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 2btc",
  )
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  let tx = &core.mempool()[0];

  let mut fee = Amount::ZERO;
  for input in &tx.input {
    fee += core.get_utxo_amount(&input.previous_output).unwrap();
  }

  for output in &tx.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx.vsize() as f64;

  assert!(f64::abs(fee_rate - 13.3) < 0.1);

  assert_eq!(
    Address::from_script(&tx.output[0].script_pubkey, Network::Bitcoin).unwrap(),
    "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
      .parse::<Address<NetworkUnchecked>>()
      .unwrap()
      .assume_checked()
  );

  assert_eq!(tx.output[0].value.to_sat(), 2 * COIN_VALUE);
}

#[test]
fn send_btc_locks_inscriptions() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (_, reveal) = inscribe(&core, &ord);

  CommandBuilder::new("wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 1btc")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  assert!(core.get_locked().contains(&OutPoint {
    txid: reveal,
    vout: 0,
  }))
}

#[test]
fn send_btc_fails_if_lock_unspent_fails() {
  let core = mockcore::builder().fail_lock_unspent(true).build();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new("wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 1btc")
    .core(&core)
    .ord(&ord)
    .expected_stderr("error: failed to lock UTXOs\n")
    .expected_exit_code(1)
    .run_and_extract_stdout();
}

#[test]
fn wallet_send_with_fee_rate() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  CommandBuilder::new(format!(
    "wallet send bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription} --fee-rate 2.0"
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  let tx = &core.mempool()[0];
  let mut fee = Amount::ZERO;
  for input in &tx.input {
    fee += core.get_utxo_amount(&input.previous_output).unwrap();
  }
  for output in &tx.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx.vsize() as f64;

  pretty_assert_eq!(fee_rate, 2.0);
}

#[test]
fn user_must_provide_fee_rate_to_send() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  CommandBuilder::new(format!(
    "wallet send bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription}"
  ))
  .core(&core)
  .ord(&ord)
  .expected_exit_code(2)
  .stderr_regex(
    ".*error: the following required arguments were not provided:
.*--fee-rate <FEE_RATE>.*",
  )
  .run_and_extract_stdout();
}

#[test]
fn wallet_send_with_fee_rate_and_target_postage() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  CommandBuilder::new(format!(
    "wallet send bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription} --fee-rate 2.0 --postage 77000sat"
  ))
  .core(&core)
    .ord(&ord)
  .run_and_deserialize_output::<Send>();

  let tx = &core.mempool()[0];
  let mut fee = Amount::ZERO;
  for input in &tx.input {
    fee += core.get_utxo_amount(&input.previous_output).unwrap();
  }
  for output in &tx.output {
    fee -= output.value;
  }

  let fee_rate = fee.to_sat() as f64 / tx.vsize() as f64;

  pretty_assert_eq!(fee_rate, 2.0);
  pretty_assert_eq!(tx.output[0].value.to_sat(), 77_000);
}

#[test]
fn send_btc_does_not_send_locked_utxos() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  let coinbase_tx = &core.mine_blocks(1)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.compute_txid(), 0);

  core.lock(outpoint);

  CommandBuilder::new("wallet send --fee-rate 1 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 1btc")
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .stderr_regex("error:.*")
    .run_and_extract_stdout();
}

#[test]
fn send_dry_run() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {inscription} --dry-run",
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  assert!(core.mempool().is_empty());
  assert_eq!(
    Psbt::deserialize(&base64_decode(&output.psbt).unwrap())
      .unwrap()
      .fee()
      .unwrap()
      .to_sat(),
    output.fee
  );
  assert_eq!(output.asset, Outgoing::InscriptionId(inscription));
}

#[test]
fn sending_rune_that_has_not_been_etched_is_an_error() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  let coinbase_tx = &core.mine_blocks(1)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.compute_txid(), 0);

  core.lock(outpoint);

  CommandBuilder::new("--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1:FOO")
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .expected_stderr("error: rune `FOO` has not been etched\n")
    .run_and_extract_stdout();
}

#[test]
fn sending_rune_with_excessive_precision_is_an_error() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1.1:{}",
    Rune(RUNE)
  ))
  .core(&core)
    .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr("error: excessive precision\n")
  .run_and_extract_stdout();
}

#[test]
fn sending_rune_with_insufficient_balance_is_an_error() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1001:{}",
    Rune(RUNE)
  ))
  .core(&core)
    .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr("error: insufficient `AAAAAAAAAAAAA` balance, only 1000\u{A0}¢ in wallet\n")
  .run_and_extract_stdout();
}

#[test]
fn sending_rune_works() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  let output = CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1000:{}",
    Rune(RUNE)
  ))
  .core(&core)
    .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  pretty_assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: vec![(
        SpacedRune::new(Rune(RUNE), 0),
        vec![(
          OutPoint {
            txid: output.txid,
            vout: 0
          },
          Pile {
            amount: 1000,
            divisibility: 0,
            symbol: Some('¢')
          },
        )]
        .into_iter()
        .collect()
      ),]
      .into_iter()
      .collect(),
    }
  );
}

#[test]
fn sending_rune_with_change_works() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  let output = CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --postage 1234sat --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 777:{}",
    Rune(RUNE)
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let tx = core.tx_by_id(output.txid);

  assert_eq!(tx.output[1].value.to_sat(), 1234);
  assert_eq!(tx.output[2].value.to_sat(), 1234);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  pretty_assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: [(
        SpacedRune::new(Rune(RUNE), 0),
        [
          (
            OutPoint {
              txid: output.txid,
              vout: 1
            },
            Pile {
              amount: 223,
              divisibility: 0,
              symbol: Some('¢')
            },
          ),
          (
            OutPoint {
              txid: output.txid,
              vout: 2
            },
            Pile {
              amount: 777,
              divisibility: 0,
              symbol: Some('¢')
            },
          )
        ]
        .into()
      )]
      .into()
    }
  );
}

#[test]
fn sending_rune_creates_change_output_for_non_outgoing_runes() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  let a = etch(&core, &ord, Rune(RUNE));
  let b = etch(&core, &ord, Rune(RUNE + 1));

  let (a_block, a_tx) = core.tx_index(a.output.reveal);
  let (b_block, b_tx) = core.tx_index(b.output.reveal);

  core.mine_blocks(1);

  let address = CommandBuilder::new("--regtest wallet receive")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::wallet::receive::Output>()
    .addresses
    .into_iter()
    .next()
    .unwrap();

  let merge = core.broadcast_template(TransactionTemplate {
    inputs: &[(a_block, a_tx, 1, default()), (b_block, b_tx, 1, default())],
    recipient: Some(address.require_network(Network::Regtest).unwrap()),
    ..default()
  });

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  pretty_assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: [
        (
          SpacedRune::new(Rune(RUNE), 0),
          [(
            OutPoint {
              txid: merge,
              vout: 0
            },
            Pile {
              amount: 1000,
              divisibility: 0,
              symbol: Some('¢')
            },
          )]
          .into()
        ),
        (
          SpacedRune::new(Rune(RUNE + 1), 0),
          [(
            OutPoint {
              txid: merge,
              vout: 0
            },
            Pile {
              amount: 1000,
              divisibility: 0,
              symbol: Some('¢')
            },
          )]
          .into()
        ),
      ]
      .into()
    }
  );

  let output = CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1000:{}",
    Rune(RUNE)
  ))
  .core(&core)
    .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  pretty_assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: [
        (
          SpacedRune::new(Rune(RUNE), 0),
          [(
            OutPoint {
              txid: output.txid,
              vout: 2
            },
            Pile {
              amount: 1000,
              divisibility: 0,
              symbol: Some('¢')
            },
          )]
          .into()
        ),
        (
          SpacedRune::new(Rune(RUNE + 1), 0),
          [(
            OutPoint {
              txid: output.txid,
              vout: 1
            },
            Pile {
              amount: 1000,
              divisibility: 0,
              symbol: Some('¢')
            },
          )]
          .into()
        )
      ]
      .into()
    }
  );

  pretty_assert_eq!(
    CommandBuilder::new("--regtest --index-runes wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: 84999960160,
      ordinal: 20000,
      runes: Some([(SpacedRune::new(Rune(RUNE + 1), 0), "1000".parse().unwrap())].into()),
      runic: Some(10000),
      total: 84999990160,
    }
  );
}

#[test]
fn sending_spaced_rune_works_with_no_change() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  let output = CommandBuilder::new(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1000:A•AAAAAAAAAAAA",
  )
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let tx = core.tx_by_id(output.txid);

  assert_eq!(tx.output.len(), 1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: vec![(
        SpacedRune::new(Rune(RUNE), 0),
        vec![(
          OutPoint {
            txid: output.txid,
            vout: 0
          },
          Pile {
            amount: 1000,
            divisibility: 0,
            symbol: Some('¢')
          },
        )]
        .into_iter()
        .collect()
      ),]
      .into_iter()
      .collect(),
    }
  );
}

#[test]
fn sending_rune_with_divisibility_works() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let rune = Rune(RUNE);

  batch(
    &core,
    &ord,
    batch::File {
      etching: Some(batch::Etching {
        divisibility: 1,
        rune: SpacedRune { rune, spacers: 0 },
        premine: "1000".parse().unwrap(),
        supply: "1000".parse().unwrap(),
        symbol: '¢',
        terms: None,
        turbo: false,
      }),
      inscriptions: vec![batch::Entry {
        file: Some("inscription.jpeg".into()),
        ..default()
      }],
      ..default()
    },
  );

  let output = CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 10.1:{}",
    rune
  ))
  .core(&core)
    .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  pretty_assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: vec![(
        SpacedRune::new(Rune(RUNE), 0),
        vec![
          (
            OutPoint {
              txid: output.txid,
              vout: 1
            },
            Pile {
              amount: 9899,
              divisibility: 1,
              symbol: Some('¢')
            },
          ),
          (
            OutPoint {
              txid: output.txid,
              vout: 2
            },
            Pile {
              amount: 101,
              divisibility: 1,
              symbol: Some('¢')
            },
          )
        ]
        .into_iter()
        .collect()
      ),]
      .into_iter()
      .collect(),
    }
  );
}

#[test]
fn sending_rune_leaves_unspent_runes_in_wallet() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  let output = CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 750:{}",
    Rune(RUNE)
  ))
  .core(&core)
    .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: vec![(
        SpacedRune::new(Rune(RUNE), 0),
        vec![
          (
            OutPoint {
              txid: output.txid,
              vout: 1
            },
            Pile {
              amount: 250,
              divisibility: 0,
              symbol: Some('¢')
            },
          ),
          (
            OutPoint {
              txid: output.txid,
              vout: 2
            },
            Pile {
              amount: 750,
              divisibility: 0,
              symbol: Some('¢')
            },
          )
        ]
        .into_iter()
        .collect()
      ),]
      .into_iter()
      .collect(),
    }
  );

  let tx = core.tx_by_id(output.txid);

  let address = Address::from_script(&tx.output[1].script_pubkey, Network::Regtest).unwrap();

  assert!(core.state().change_addresses.contains(&address));
}

#[test]
fn sending_rune_creates_transaction_with_expected_runestone() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  let etch = etch(&core, &ord, Rune(RUNE));

  let output = CommandBuilder::new(format!(
    "
      --chain regtest
      --index-runes
      wallet
      send
      --fee-rate 1
      bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 750:{}
    ",
    Rune(RUNE),
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  let balances = CommandBuilder::new("--regtest --index-runes balances")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::balances::Output>();

  assert_eq!(
    balances,
    ord::subcommand::balances::Output {
      runes: vec![(
        SpacedRune::new(Rune(RUNE), 0),
        vec![
          (
            OutPoint {
              txid: output.txid,
              vout: 1
            },
            Pile {
              amount: 250,
              divisibility: 0,
              symbol: Some('¢')
            },
          ),
          (
            OutPoint {
              txid: output.txid,
              vout: 2
            },
            Pile {
              amount: 750,
              divisibility: 0,
              symbol: Some('¢')
            },
          )
        ]
        .into_iter()
        .collect()
      ),]
      .into_iter()
      .collect(),
    }
  );

  let tx = core.tx_by_id(output.txid);

  pretty_assert_eq!(
    Runestone::decipher(&tx).unwrap(),
    Artifact::Runestone(Runestone {
      pointer: None,
      etching: None,
      edicts: vec![Edict {
        id: etch.id,
        amount: 750,
        output: 2
      }],
      mint: None,
    }),
  );
}

#[test]
fn error_messages_use_spaced_runes() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-runes", "--regtest"], &[]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  CommandBuilder::new(
    "--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1001:A•AAAAAAAAAAAA",
  )
  .core(&core)
    .ord(&ord)
  .expected_exit_code(1)
  .expected_stderr("error: insufficient `A•AAAAAAAAAAAA` balance, only 1000\u{A0}¢ in wallet\n")
  .run_and_extract_stdout();

  CommandBuilder::new("--chain regtest --index-runes wallet send --fee-rate 1 bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw 1:F•OO")
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .expected_stderr("error: rune `FOO` has not been etched\n")
    .run_and_extract_stdout();
}
