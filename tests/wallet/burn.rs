use super::*;

#[test]
fn inscriptions_can_be_burned() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new(format!("wallet burn --fee-rate 1 {inscription}",))
    .core(&core)
    .ord(&ord)
    .stdout_regex(r".*")
    .run_and_deserialize_output::<Send>();

  let txid = core.mempool()[0].compute_txid();
  assert_eq!(txid, output.txid);

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    ".*<h1>Inscription 0</h1>.*<dl>.*
  <dt>charms</dt>
  <dd>
    <span title=burned>🔥</span>
  </dd>
  <dt>value</dt>
  <dd>1</dd>
  .*
  <dt>content length</dt>
  <dd>3 bytes</dd>
  <dt>content type</dt>
  <dd>text/plain;charset=utf-8</dd>
  .*
</dl>
.*",
  );
}

#[test]
fn runic_outputs_are_protected() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--regtest", "--index-runes"], &[""]);

  create_wallet(&core, &ord);

  let (inscription, _) = inscribe_with_options(&core, &ord, Some(1000), 1);
  let height = core.height();

  let rune = Rune(RUNE);
  etch(&core, &ord, rune);

  let address = CommandBuilder::new("--regtest wallet receive")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::wallet::receive::Output>()
    .addresses
    .into_iter()
    .next()
    .unwrap();

  CommandBuilder::new(format!(
    "--regtest --index-runes wallet send --fee-rate 1 {} 1000:{} --postage 1000sat",
    address.clone().require_network(Network::Regtest).unwrap(),
    Rune(RUNE)
  ))
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Send>();

  core.mine_blocks(2);

  let txid = core.broadcast_template(TransactionTemplate {
    inputs: &[
      // send rune and inscription to the same output
      (height as usize, 2, 0, Witness::new()),
      ((core.height() - 1) as usize, 1, 0, Witness::new()),
      // fees
      (core.height() as usize, 0, 0, Witness::new()),
    ],
    outputs: 2,
    output_values: &[2000, 50 * COIN_VALUE],
    recipient: Some(address.require_network(Network::Regtest).unwrap()),
    ..default()
  });

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/output/{}:0", txid),
    format!(r".*<a href=/inscription/{}>.*</a>.*", inscription),
  );

  ord.assert_response_regex(
    format!("/output/{}:0", txid),
    format!(r".*<a href=/rune/{rune}>{rune}</a>.*"),
  );

  CommandBuilder::new(format!(
    "--regtest --index-runes wallet burn --fee-rate 1 {inscription}",
  ))
  .core(&core)
  .ord(&ord)
  .expected_stderr("error: runic outpoints may not be burned\n")
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn burns_only_one_sat() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  assert_eq!(
    CommandBuilder::new("wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: 50 * COIN_VALUE,
      ordinal: 0,
      runic: None,
      runes: None,
      total: 50 * COIN_VALUE,
    }
  );

  let (inscription, _) = inscribe_with_options(&core, &ord, Some(100_000), 1);

  CommandBuilder::new(format!("wallet burn --fee-rate 1 {inscription}",))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  // 4 block rewards - 1 burned sat
  let expected_balance = 4 * 50 * COIN_VALUE - 1;

  assert_eq!(
    CommandBuilder::new("wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: expected_balance,
      ordinal: 0,
      runic: None,
      runes: None,
      total: expected_balance,
    }
  );
}

#[test]
fn cannot_burn_inscription_sharing_utxo_with_another_inscription() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--regtest"], &[]);

  create_wallet(&core, &ord);

  let address = CommandBuilder::new("--regtest wallet receive")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<ord::subcommand::wallet::receive::Output>()
    .addresses
    .into_iter()
    .next()
    .unwrap();

  let (inscription0, _) = inscribe_with_options(&core, &ord, Some(1000), 1);
  let height0 = core.height();
  let (inscription1, _) = inscribe_with_options(&core, &ord, Some(1000), 1);
  let height1 = core.height();
  let (inscription2, _) = inscribe_with_options(&core, &ord, Some(1000), 1);
  let height2 = core.height();

  let txid = core.broadcast_template(TransactionTemplate {
    inputs: &[
      // send all 3 inscriptions on a single output
      (height0 as usize, 2, 0, Witness::new()),
      (height1 as usize, 2, 0, Witness::new()),
      (height2 as usize, 2, 0, Witness::new()),
      // fees
      (core.height() as usize, 0, 0, Witness::new()),
    ],
    outputs: 2,
    output_values: &[3000, 50 * COIN_VALUE],
    recipient: Some(address.require_network(Network::Regtest).unwrap()),
    ..default()
  });

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/output/{}:0", txid),
    format!(r".*<a href=/inscription/{}>.*</a>.*<a href=/inscription/{}>.*</a>.*<a href=/inscription/{}>.*</a>.*", inscription0, inscription1, inscription2),
  );

  CommandBuilder::new(format!("--regtest wallet burn --fee-rate 1 {inscription0}",))
    .core(&core)
    .ord(&ord)
    .expected_stderr(format!(
      "error: cannot send {txid}:0:0 without also sending inscription {inscription2} at {txid}:0:2000\n"
    ))
    .expected_exit_code(1)
    .run_and_extract_stdout();
}

#[test]
fn json_metadata_can_be_included_when_burning() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new(format!(
    "wallet burn --fee-rate 1 {inscription} --json-metadata metadata.json"
  ))
  .core(&core)
  .ord(&ord)
  .write("metadata.json", r#"{"foo": "bar", "baz": 1}"#)
  .stdout_regex(r".*")
  .run_and_deserialize_output::<Send>();

  let txid = core.mempool()[0].compute_txid();
  assert_eq!(txid, output.txid);

  core.mine_blocks(1);

  let script_pubkey = script::Builder::new()
    .push_opcode(opcodes::all::OP_RETURN)
    .push_slice([
      0xA2, 0x63, b'f', b'o', b'o', 0x63, b'b', b'a', b'r', 0x63, b'b', b'a', b'z', 0x01,
    ])
    .into_script();

  ord.assert_html(
    format!("/inscription/{inscription}"),
    Chain::Mainnet,
    InscriptionHtml {
      charms: Charm::Burned.flag(),
      fee: 138,
      id: inscription,
      output: Some(TxOut {
        value: Amount::from_sat(1),
        script_pubkey,
      }),
      height: 3,
      inscription: Inscription {
        content_type: Some("text/plain;charset=utf-8".as_bytes().into()),
        body: Some("foo".as_bytes().into()),
        ..default()
      },
      satpoint: SatPoint {
        outpoint: OutPoint {
          txid: output.txid,
          vout: 0,
        },
        offset: 0,
      },
      timestamp: "1970-01-01 00:00:03+00:00"
        .parse::<DateTime<Utc>>()
        .unwrap(),
      ..default()
    },
  );
}

#[test]
fn cbor_metadata_can_be_included_when_burning() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let metadata = [
    0xA2, 0x63, b'f', b'o', b'o', 0x63, b'b', b'a', b'r', 0x63, b'b', b'a', b'z', 0x01,
  ];

  let output = CommandBuilder::new(format!(
    "wallet burn --fee-rate 1 {inscription} --cbor-metadata metadata.cbor"
  ))
  .core(&core)
  .ord(&ord)
  .write("metadata.cbor", metadata)
  .stdout_regex(r".*")
  .run_and_deserialize_output::<Send>();

  let txid = core.mempool()[0].compute_txid();
  assert_eq!(txid, output.txid);

  core.mine_blocks(1);

  let script_pubkey = script::Builder::new()
    .push_opcode(opcodes::all::OP_RETURN)
    .push_slice(metadata)
    .into_script();

  ord.assert_html(
    format!("/inscription/{inscription}"),
    Chain::Mainnet,
    InscriptionHtml {
      charms: Charm::Burned.flag(),
      fee: 138,
      id: inscription,
      output: Some(TxOut {
        value: Amount::from_sat(1),
        script_pubkey,
      }),
      height: 3,
      inscription: Inscription {
        content_type: Some("text/plain;charset=utf-8".as_bytes().into()),
        body: Some("foo".as_bytes().into()),
        ..default()
      },
      satpoint: SatPoint {
        outpoint: OutPoint {
          txid: output.txid,
          vout: 0,
        },
        offset: 0,
      },
      timestamp: "1970-01-01 00:00:03+00:00"
        .parse::<DateTime<Utc>>()
        .unwrap(),
      ..default()
    },
  );
}

#[test]
fn cbor_and_json_metadata_flags_conflict() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new(format!(
    "wallet burn --fee-rate 1 {inscription} --cbor-metadata foo --json-metadata bar"
  ))
  .core(&core)
  .ord(&ord)
  .stderr_regex(
    "error: the argument '--cbor-metadata <PATH>' cannot be used with '--json-metadata <PATH>'.*",
  )
  .expected_exit_code(2)
  .run_and_extract_stdout();
}

#[test]
fn oversize_metadata_requires_no_limit_flag() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &[], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  CommandBuilder::new(format!(
    "wallet burn --fee-rate 1 {inscription} --json-metadata metadata.json"
  ))
  .core(&core)
  .ord(&ord)
  .write("metadata.json", format!("\"{}\"", "0".repeat(79)))
  .stderr_regex("error: OP_RETURN with metadata larger than maximum: 84 > 83\n")
  .expected_exit_code(1)
  .run_and_extract_stdout();
}

#[test]
fn burn_rune() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--regtest", "--index-runes"], &[]);

  create_wallet(&core, &ord);

  let rune = Rune(RUNE);
  etch(&core, &ord, rune);

  core.mine_blocks(1);

  CommandBuilder::new(format!("--regtest wallet burn --fee-rate 1 500:{rune}",))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  pretty_assert_eq!(
    CommandBuilder::new("--regtest wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: 450 * COIN_VALUE - 2 * 10000 + 129,
      ordinal: 10000,
      runic: Some(9871),
      runes: Some(
        [(
          SpacedRune { rune, spacers: 0 },
          Decimal {
            value: 500,
            scale: 0
          }
        )]
        .into_iter()
        .collect()
      ),
      total: 450 * COIN_VALUE,
    }
  );

  CommandBuilder::new(format!("--regtest wallet burn --fee-rate 1 500:{rune}",))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  pretty_assert_eq!(
    CommandBuilder::new("--regtest wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: 500 * COIN_VALUE - 10000,
      ordinal: 10000,
      runic: Some(0),
      runes: Some(BTreeMap::new()),
      total: 500 * COIN_VALUE,
    }
  );
}

#[test]
fn burn_rune_with_many_assets_in_wallet() {
  let core = mockcore::builder().network(Network::Regtest).build();

  let ord = TestServer::spawn_with_server_args(&core, &["--regtest", "--index-runes"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  inscribe(&core, &ord);

  let rune_0 = Rune(RUNE);
  etch(&core, &ord, rune_0);

  let rune_1 = Rune(RUNE - 1);
  etch(&core, &ord, rune_1);

  let rune_2 = Rune(RUNE - 2);
  etch(&core, &ord, rune_2);

  pretty_assert_eq!(
    CommandBuilder::new("--regtest wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: 119999930000,
      ordinal: 40000,
      runic: Some(30000),
      runes: Some(
        [
          (
            SpacedRune {
              rune: rune_0,
              spacers: 0
            },
            Decimal {
              value: 1000,
              scale: 0
            }
          ),
          (
            SpacedRune {
              rune: rune_1,
              spacers: 0
            },
            Decimal {
              value: 1000,
              scale: 0
            }
          ),
          (
            SpacedRune {
              rune: rune_2,
              spacers: 0
            },
            Decimal {
              value: 1000,
              scale: 0
            }
          )
        ]
        .into_iter()
        .collect()
      ),
      total: 24 * 50 * COIN_VALUE,
    }
  );

  CommandBuilder::new(format!("--regtest wallet burn --fee-rate 1 1111:{rune_0}",))
    .core(&core)
    .ord(&ord)
    .expected_exit_code(1)
    .stderr_regex("error: insufficient `AAAAAAAAAAAAA` balance.*")
    .run_and_extract_stdout();

  CommandBuilder::new(format!("--regtest wallet burn --fee-rate 1 1000:{rune_2}",))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  pretty_assert_eq!(
    CommandBuilder::new("--regtest wallet balance")
      .core(&core)
      .ord(&ord)
      .run_and_deserialize_output::<Balance>(),
    Balance {
      cardinal: 124999940000,
      ordinal: 40000,
      runic: Some(20000),
      runes: Some(
        [
          (
            SpacedRune {
              rune: rune_0,
              spacers: 0
            },
            Decimal {
              value: 1000,
              scale: 0
            }
          ),
          (
            SpacedRune {
              rune: rune_1,
              spacers: 0
            },
            Decimal {
              value: 1000,
              scale: 0
            }
          ),
        ]
        .into_iter()
        .collect()
      ),
      total: 25 * 50 * COIN_VALUE,
    }
  );
}

#[test]
fn burning_rune_creates_change_output_for_non_burnt_runes() {
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

  let merge = core.broadcast_tx(TransactionTemplate {
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
    "--chain regtest --index-runes wallet burn --fee-rate 1 500:{}",
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
              vout: 1
            },
            Pile {
              amount: 500,
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
      cardinal: 84999970000,
      ordinal: 20000,
      runes: Some(
        [
          (SpacedRune::new(Rune(RUNE), 0), "500".parse().unwrap()),
          (SpacedRune::new(Rune(RUNE + 1), 0), "1000".parse().unwrap())
        ]
        .into()
      ),
      runic: Some(10000),
      total: 17 * 50 * COIN_VALUE,
    }
  );
}
