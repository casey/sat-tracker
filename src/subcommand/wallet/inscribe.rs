use super::*;

#[derive(Debug, Parser)]
#[clap(
  group = ArgGroup::new("source")
      .required(true)
      .args(&["file", "batch", "delegate"]),
)]
pub(crate) struct Inscribe {
  #[arg(
    long,
    help = "Inscribe multiple inscriptions defined in a yaml <BATCH_FILE>.",
    conflicts_with_all = &[
      "cbor_metadata", "delegate", "destination", "file", "json_metadata", "metaprotocol",
      "parent", "postage", "reinscribe", "satpoint"
    ]
  )]
  pub(crate) batch: Option<PathBuf>,
  #[arg(
    long,
    help = "Include CBOR in file at <METADATA> as inscription metadata",
    conflicts_with = "json_metadata"
  )]
  pub(crate) cbor_metadata: Option<PathBuf>,
  #[arg(
    long,
    help = "Use <COMMIT_FEE_RATE> sats/vbyte for commit transaction.\nDefaults to <FEE_RATE> if unset."
  )]
  pub(crate) commit_fee_rate: Option<FeeRate>,
  #[arg(long, help = "Compress inscription content with brotli.")]
  pub(crate) compress: bool,
  #[arg(long, help = "Delegate inscription content to <DELEGATE>.")]
  pub(crate) delegate: Option<InscriptionId>,
  #[arg(long, help = "Send inscription to <DESTINATION>.")]
  pub(crate) destination: Option<Address<NetworkUnchecked>>,
  #[arg(long, help = "Don't sign or broadcast transactions.")]
  pub(crate) dry_run: bool,
  #[arg(long, help = "Use fee rate of <FEE_RATE> sats/vB.")]
  pub(crate) fee_rate: FeeRate,
  #[arg(long, help = "Inscribe sat with contents of <FILE>.")]
  pub(crate) file: Option<PathBuf>,
  #[arg(
    long,
    help = "Include JSON in file at <METADATA> converted to CBOR as inscription metadata",
    conflicts_with = "cbor_metadata"
  )]
  pub(crate) json_metadata: Option<PathBuf>,
  #[clap(long, help = "Set inscription metaprotocol to <METAPROTOCOL>.")]
  pub(crate) metaprotocol: Option<String>,
  #[arg(long, alias = "nobackup", help = "Do not back up recovery key.")]
  pub(crate) no_backup: bool,
  #[arg(
    long,
    alias = "nolimit",
    help = "Do not check that transactions are equal to or below the MAX_STANDARD_TX_WEIGHT of 400,000 weight units. Transactions over this limit are currently nonstandard and will not be relayed by bitcoind in its default configuration. Do not use this flag unless you understand the implications."
  )]
  pub(crate) no_limit: bool,
  #[clap(long, help = "Make inscription a child of <PARENT>.")]
  pub(crate) parent: Option<InscriptionId>,
  #[arg(
    long,
    help = "Amount of postage to include in the inscription. Default `10000sat`."
  )]
  pub(crate) postage: Option<Amount>,
  #[clap(long, help = "Allow reinscription.")]
  pub(crate) reinscribe: bool,
  #[arg(long, help = "Inscribe <SATPOINT>.")]
  pub(crate) satpoint: Option<SatPoint>,
  #[arg(long, help = "Inscribe <SAT>.", conflicts_with = "satpoint")]
  pub(crate) sat: Option<Sat>,
}

impl Inscribe {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    let metadata = Inscribe::parse_metadata(self.cbor_metadata, self.json_metadata)?;

    let utxos = wallet.get_unspent_outputs()?;

    let locked_utxos = wallet.get_locked_outputs()?;

    let runic_utxos = wallet.get_runic_outputs()?;

    let chain = wallet.chain();

    let postage;
    let destinations;
    let inscriptions;
    let mode;
    let parent_info;
    let sat;

    match (self.file, self.batch, self.delegate) {
      (Some(file), None, _) => {
        parent_info = wallet.get_parent_info(self.parent, &utxos)?;

        postage = self.postage.unwrap_or(TARGET_POSTAGE);

        if let Some(delegate) = self.delegate {
          ensure! {
            wallet.inscription_exists(delegate)?,
            "delegate {delegate} does not exist"
          }
        }

        inscriptions = vec![Inscription::from_file(
          chain,
          self.compress,
          self.delegate,
          metadata,
          self.metaprotocol,
          self.parent,
          file,
          None,
        )?];

        mode = Mode::SeparateOutputs;

        sat = self.sat;

        destinations = vec![match self.destination.clone() {
          Some(destination) => destination.require_network(chain.network())?,
          None => wallet.get_change_address()?,
        }];
      }
      (None, Some(batch), _) => {
        let batchfile = Batchfile::load(&batch)?;

        parent_info = wallet.get_parent_info(batchfile.parent, &utxos)?;

        postage = batchfile
          .postage
          .map(Amount::from_sat)
          .unwrap_or(TARGET_POSTAGE);

        (inscriptions, destinations) = batchfile.inscriptions(
          &wallet,
          parent_info.as_ref().map(|info| info.tx_out.value),
          metadata,
          postage,
          self.compress,
        )?;

        mode = batchfile.mode;

        if batchfile.sat.is_some() && mode != Mode::SameSat {
          return Err(anyhow!("`sat` can only be set in `same-sat` mode"));
        }

        sat = batchfile.sat;
      }
      (None, None, Some(delegate)) => {
        parent_info = wallet.get_parent_info(self.parent, &utxos)?;

        postage = self.postage.unwrap_or(TARGET_POSTAGE);

        ensure! {
          wallet.inscription_exists(delegate)?,
          "delegate {delegate} does not exist"
        }

        inscriptions = vec![Inscription::without_file(
          self.delegate,
          metadata,
          self.metaprotocol,
          self.parent,
          None,
        )?];

        mode = Mode::SeparateOutputs;

        sat = self.sat;

        destinations = vec![match self.destination.clone() {
          Some(destination) => destination.require_network(chain.network())?,
          None => wallet.get_change_address()?,
        }];
      }
      _ => unreachable!(),
    }

    let satpoint = if let Some(sat) = sat {
      Some(wallet.find_sat_in_outputs(sat, &utxos)?)
    } else {
      self.satpoint
    };

    Batch {
      commit_fee_rate: self.commit_fee_rate.unwrap_or(self.fee_rate),
      destinations,
      dry_run: self.dry_run,
      inscriptions,
      mode,
      no_backup: self.no_backup,
      no_limit: self.no_limit,
      parent_info,
      postage,
      reinscribe: self.reinscribe,
      reveal_fee_rate: self.fee_rate,
      satpoint,
    }
    .inscribe(&locked_utxos, runic_utxos, &utxos, &wallet)
  }

  fn parse_metadata(cbor: Option<PathBuf>, json: Option<PathBuf>) -> Result<Option<Vec<u8>>> {
    if let Some(path) = cbor {
      let cbor = fs::read(path)?;
      let _value: Value = ciborium::from_reader(Cursor::new(cbor.clone()))
        .context("failed to parse CBOR metadata")?;

      Ok(Some(cbor))
    } else if let Some(path) = json {
      let value: serde_json::Value =
        serde_json::from_reader(File::open(path)?).context("failed to parse JSON metadata")?;
      let mut cbor = Vec::new();
      ciborium::into_writer(&value, &mut cbor)?;

      Ok(Some(cbor))
    } else {
      Ok(None)
    }
  }
}

#[cfg(test)]
mod tests {
  use {
    super::*,
    crate::wallet::inscribe::{BatchEntry, ParentInfo},
    bitcoin::policy::MAX_STANDARD_TX_WEIGHT,
    serde_yaml::{Mapping, Value},
    tempfile::TempDir,
  };

  #[test]
  fn reveal_transaction_pays_fee() {
    let utxos = vec![(outpoint(1), Amount::from_sat(20000))];
    let inscription = inscription("text/plain", "ord");
    let commit_address = change(0);
    let reveal_address = recipient();
    let change = [commit_address, change(1)];

    let (commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint: Some(satpoint(1, 0)),
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(1.0).unwrap(),
      reveal_fee_rate: FeeRate::try_from(1.0).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      BTreeMap::new(),
      Chain::Mainnet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      change,
    )
    .unwrap();

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let fee = Amount::from_sat((1.0 * (reveal_tx.vsize() as f64)).ceil() as u64);

    assert_eq!(
      reveal_tx.output[0].value,
      20000 - fee.to_sat() - (20000 - commit_tx.output[0].value),
    );
  }

  #[test]
  fn inscribe_transactions_opt_in_to_rbf() {
    let utxos = vec![(outpoint(1), Amount::from_sat(20000))];
    let inscription = inscription("text/plain", "ord");
    let commit_address = change(0);
    let reveal_address = recipient();
    let change = [commit_address, change(1)];

    let (commit_tx, reveal_tx, _, _) = Batch {
      satpoint: Some(satpoint(1, 0)),
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(1.0).unwrap(),
      reveal_fee_rate: FeeRate::try_from(1.0).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      BTreeMap::new(),
      Chain::Mainnet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      change,
    )
    .unwrap();

    assert!(commit_tx.is_explicitly_rbf());
    assert!(reveal_tx.is_explicitly_rbf());
  }

  #[test]
  fn inscribe_with_no_satpoint_and_no_cardinal_utxos() {
    let utxos = vec![(outpoint(1), Amount::from_sat(1000))];
    let mut inscriptions = BTreeMap::new();
    inscriptions.insert(
      SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      inscription_id(1),
    );

    let inscription = inscription("text/plain", "ord");
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();

    let error = Batch {
      satpoint,
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(1.0).unwrap(),
      reveal_fee_rate: FeeRate::try_from(1.0).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      inscriptions,
      Chain::Mainnet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(1)],
    )
    .unwrap_err()
    .to_string();

    assert!(
      error.contains("wallet contains no cardinal utxos"),
      "{}",
      error
    );
  }

  #[test]
  fn inscribe_with_no_satpoint_and_enough_cardinal_utxos() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(20_000)),
      (outpoint(2), Amount::from_sat(20_000)),
    ];
    let mut inscriptions = BTreeMap::new();
    inscriptions.insert(
      SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      inscription_id(1),
    );

    let inscription = inscription("text/plain", "ord");
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();

    assert!(Batch {
      satpoint,
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(1.0).unwrap(),
      reveal_fee_rate: FeeRate::try_from(1.0).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      inscriptions,
      Chain::Mainnet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(1)],
    )
    .is_ok())
  }

  #[test]
  fn inscribe_with_custom_fee_rate() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(20_000)),
    ];
    let mut inscriptions = BTreeMap::new();
    inscriptions.insert(
      SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      inscription_id(1),
    );

    let inscription = inscription("text/plain", "ord");
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();
    let fee_rate = 3.3;

    let (commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint,
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(fee_rate).unwrap(),
      reveal_fee_rate: FeeRate::try_from(fee_rate).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(1)],
    )
    .unwrap();

    let sig_vbytes = 17;
    let fee = FeeRate::try_from(fee_rate)
      .unwrap()
      .fee(commit_tx.vsize() + sig_vbytes)
      .to_sat();

    let reveal_value = commit_tx
      .output
      .iter()
      .map(|o| o.value)
      .reduce(|acc, i| acc + i)
      .unwrap();

    assert_eq!(reveal_value, 20_000 - fee);

    let fee = FeeRate::try_from(fee_rate)
      .unwrap()
      .fee(reveal_tx.vsize())
      .to_sat();

    assert_eq!(
      reveal_tx.output[0].value,
      20_000 - fee - (20_000 - commit_tx.output[0].value),
    );
  }

  #[test]
  fn inscribe_with_parent() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(20_000)),
    ];

    let mut inscriptions = BTreeMap::new();
    let parent_inscription = inscription_id(1);
    let parent_info = ParentInfo {
      destination: change(3),
      id: parent_inscription,
      location: SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      tx_out: TxOut {
        script_pubkey: change(0).script_pubkey(),
        value: 10000,
      },
    };

    inscriptions.insert(parent_info.location, parent_inscription);

    let child_inscription = InscriptionTemplate {
      parent: Some(parent_inscription),
      ..Default::default()
    }
    .into();

    let commit_address = change(1);
    let reveal_address = recipient();
    let fee_rate = 4.0;

    let (commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint: None,
      parent_info: Some(parent_info.clone()),
      inscriptions: vec![child_inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(fee_rate).unwrap(),
      reveal_fee_rate: FeeRate::try_from(fee_rate).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    )
    .unwrap();

    let sig_vbytes = 17;
    let fee = FeeRate::try_from(fee_rate)
      .unwrap()
      .fee(commit_tx.vsize() + sig_vbytes)
      .to_sat();

    let reveal_value = commit_tx
      .output
      .iter()
      .map(|o| o.value)
      .reduce(|acc, i| acc + i)
      .unwrap();

    assert_eq!(reveal_value, 20_000 - fee);

    let sig_vbytes = 16;
    let fee = FeeRate::try_from(fee_rate)
      .unwrap()
      .fee(reveal_tx.vsize() + sig_vbytes)
      .to_sat();

    assert_eq!(fee, commit_tx.output[0].value - reveal_tx.output[1].value,);
    assert_eq!(
      reveal_tx.output[0].script_pubkey,
      parent_info.destination.script_pubkey()
    );
    assert_eq!(reveal_tx.output[0].value, parent_info.tx_out.value);
    pretty_assert_eq!(
      reveal_tx.input[0],
      TxIn {
        previous_output: parent_info.location.outpoint,
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        ..Default::default()
      }
    );
  }

  #[test]
  fn inscribe_with_commit_fee_rate() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(20_000)),
    ];
    let mut inscriptions = BTreeMap::new();
    inscriptions.insert(
      SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      inscription_id(1),
    );

    let inscription = inscription("text/plain", "ord");
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();
    let commit_fee_rate = 3.3;
    let fee_rate = 1.0;

    let (commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint,
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(commit_fee_rate).unwrap(),
      reveal_fee_rate: FeeRate::try_from(fee_rate).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(1)],
    )
    .unwrap();

    let sig_vbytes = 17;
    let fee = FeeRate::try_from(commit_fee_rate)
      .unwrap()
      .fee(commit_tx.vsize() + sig_vbytes)
      .to_sat();

    let reveal_value = commit_tx
      .output
      .iter()
      .map(|o| o.value)
      .reduce(|acc, i| acc + i)
      .unwrap();

    assert_eq!(reveal_value, 20_000 - fee);

    let fee = FeeRate::try_from(fee_rate)
      .unwrap()
      .fee(reveal_tx.vsize())
      .to_sat();

    assert_eq!(
      reveal_tx.output[0].value,
      20_000 - fee - (20_000 - commit_tx.output[0].value),
    );
  }

  #[test]
  fn inscribe_over_max_standard_tx_weight() {
    let utxos = vec![(outpoint(1), Amount::from_sat(50 * COIN_VALUE))];

    let inscription = inscription("text/plain", [0; MAX_STANDARD_TX_WEIGHT as usize]);
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();

    let error = Batch {
      satpoint,
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(1.0).unwrap(),
      reveal_fee_rate: FeeRate::try_from(1.0).unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      BTreeMap::new(),
      Chain::Mainnet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(1)],
    )
    .unwrap_err()
    .to_string();

    assert!(
      error.contains(&format!("reveal transaction weight greater than {MAX_STANDARD_TX_WEIGHT} (MAX_STANDARD_TX_WEIGHT): 402799")),
      "{}",
      error
    );
  }

  #[test]
  fn inscribe_with_no_max_standard_tx_weight() {
    let utxos = vec![(outpoint(1), Amount::from_sat(50 * COIN_VALUE))];

    let inscription = inscription("text/plain", [0; MAX_STANDARD_TX_WEIGHT as usize]);
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();

    let (_commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint,
      parent_info: None,
      inscriptions: vec![inscription],
      destinations: vec![reveal_address],
      commit_fee_rate: FeeRate::try_from(1.0).unwrap(),
      reveal_fee_rate: FeeRate::try_from(1.0).unwrap(),
      no_limit: true,
      reinscribe: false,
      postage: TARGET_POSTAGE,
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      BTreeMap::new(),
      Chain::Mainnet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(1)],
    )
    .unwrap();

    assert!(reveal_tx.size() >= MAX_STANDARD_TX_WEIGHT as usize);
  }

  #[test]
  fn cbor_and_json_metadata_flags_conflict() {
    assert_regex_match!(
      Arguments::try_parse_from([
        "ord",
        "wallet",
        "inscribe",
        "--cbor-metadata",
        "foo",
        "--json-metadata",
        "bar",
        "--file",
        "baz",
      ])
      .unwrap_err()
      .to_string(),
      ".*--cbor-metadata.*cannot be used with.*--json-metadata.*"
    );
  }

  #[test]
  fn batch_is_loaded_from_yaml_file() {
    let parent = "8d363b28528b0cb86b5fd48615493fb175bdf132d2a3d20b4251bba3f130a5abi0"
      .parse::<InscriptionId>()
      .unwrap();

    let tempdir = TempDir::new().unwrap();

    let inscription_path = tempdir.path().join("tulip.txt");
    fs::write(&inscription_path, "tulips are pretty").unwrap();

    let brc20_path = tempdir.path().join("token.json");

    let batch_path = tempdir.path().join("batch.yaml");
    fs::write(
      &batch_path,
      format!(
        "mode: separate-outputs
parent: {parent}
inscriptions:
- file: {}
  metadata:
    title: Lorem Ipsum
    description: Lorem ipsum dolor sit amet, consectetur adipiscing elit. In tristique, massa nec condimentum venenatis, ante massa tempor velit, et accumsan ipsum ligula a massa. Nunc quis orci ante.
- file: {}
  metaprotocol: brc-20
",
        inscription_path.display(),
        brc20_path.display()
      ),
    )
    .unwrap();

    let mut metadata = Mapping::new();
    metadata.insert(
      Value::String("title".to_string()),
      Value::String("Lorem Ipsum".to_string()),
    );
    metadata.insert(Value::String("description".to_string()), Value::String("Lorem ipsum dolor sit amet, consectetur adipiscing elit. In tristique, massa nec condimentum venenatis, ante massa tempor velit, et accumsan ipsum ligula a massa. Nunc quis orci ante.".to_string()));

    assert_eq!(
      Batchfile::load(&batch_path).unwrap(),
      Batchfile {
        inscriptions: vec![
          BatchEntry {
            file: inscription_path,
            metadata: Some(Value::Mapping(metadata)),
            ..Default::default()
          },
          BatchEntry {
            file: brc20_path,
            metaprotocol: Some("brc-20".to_string()),
            ..Default::default()
          }
        ],
        parent: Some(parent),
        ..Default::default()
      }
    );
  }

  #[test]
  fn batch_with_unknown_field_throws_error() {
    let tempdir = TempDir::new().unwrap();
    let batch_path = tempdir.path().join("batch.yaml");
    fs::write(
      &batch_path,
      "mode: shared-output\ninscriptions:\n- file: meow.wav\nunknown: 1.)what",
    )
    .unwrap();

    assert!(Batchfile::load(&batch_path)
      .unwrap_err()
      .to_string()
      .contains("unknown field `unknown`"));
  }

  #[test]
  fn batch_inscribe_with_parent() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(50_000)),
    ];

    let parent = inscription_id(1);

    let parent_info = ParentInfo {
      destination: change(3),
      id: parent,
      location: SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      tx_out: TxOut {
        script_pubkey: change(0).script_pubkey(),
        value: 10000,
      },
    };

    let mut wallet_inscriptions = BTreeMap::new();
    wallet_inscriptions.insert(parent_info.location, parent);

    let commit_address = change(1);
    let reveal_addresses = vec![recipient()];

    let inscriptions = vec![
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
    ];

    let mode = Mode::SharedOutput;

    let fee_rate = 4.0.try_into().unwrap();

    let (commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint: None,
      parent_info: Some(parent_info.clone()),
      inscriptions,
      destinations: reveal_addresses,
      commit_fee_rate: fee_rate,
      reveal_fee_rate: fee_rate,
      no_limit: false,
      reinscribe: false,
      postage: Amount::from_sat(10_000),
      mode,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      wallet_inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    )
    .unwrap();

    let sig_vbytes = 17;
    let fee = fee_rate.fee(commit_tx.vsize() + sig_vbytes).to_sat();

    let reveal_value = commit_tx
      .output
      .iter()
      .map(|o| o.value)
      .reduce(|acc, i| acc + i)
      .unwrap();

    assert_eq!(reveal_value, 50_000 - fee);

    let sig_vbytes = 16;
    let fee = fee_rate.fee(reveal_tx.vsize() + sig_vbytes).to_sat();

    assert_eq!(fee, commit_tx.output[0].value - reveal_tx.output[1].value,);
    assert_eq!(
      reveal_tx.output[0].script_pubkey,
      parent_info.destination.script_pubkey()
    );
    assert_eq!(reveal_tx.output[0].value, parent_info.tx_out.value);
    pretty_assert_eq!(
      reveal_tx.input[0],
      TxIn {
        previous_output: parent_info.location.outpoint,
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        ..Default::default()
      }
    );
  }

  #[test]
  fn batch_inscribe_with_parent_not_enough_cardinals_utxos_fails() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(20_000)),
    ];

    let parent = inscription_id(1);

    let parent_info = ParentInfo {
      destination: change(3),
      id: parent,
      location: SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      tx_out: TxOut {
        script_pubkey: change(0).script_pubkey(),
        value: 10000,
      },
    };

    let mut wallet_inscriptions = BTreeMap::new();
    wallet_inscriptions.insert(parent_info.location, parent);

    let inscriptions = vec![
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
    ];

    let commit_address = change(1);
    let reveal_addresses = vec![recipient()];

    let error = Batch {
      satpoint: None,
      parent_info: Some(parent_info.clone()),
      inscriptions,
      destinations: reveal_addresses,
      commit_fee_rate: 4.0.try_into().unwrap(),
      reveal_fee_rate: 4.0.try_into().unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: Amount::from_sat(10_000),
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      wallet_inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains(
      "wallet does not contain enough cardinal UTXOs, please add additional funds to wallet."
    ));
  }

  #[test]
  #[should_panic(expected = "invariant: shared-output has only one destination")]
  fn batch_inscribe_with_inconsistent_reveal_addresses_panics() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(80_000)),
    ];

    let parent = inscription_id(1);

    let parent_info = ParentInfo {
      destination: change(3),
      id: parent,
      location: SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      tx_out: TxOut {
        script_pubkey: change(0).script_pubkey(),
        value: 10000,
      },
    };

    let mut wallet_inscriptions = BTreeMap::new();
    wallet_inscriptions.insert(parent_info.location, parent);

    let inscriptions = vec![
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
    ];

    let commit_address = change(1);
    let reveal_addresses = vec![recipient(), recipient()];

    let _ = Batch {
      satpoint: None,
      parent_info: Some(parent_info.clone()),
      inscriptions,
      destinations: reveal_addresses,
      commit_fee_rate: 4.0.try_into().unwrap(),
      reveal_fee_rate: 4.0.try_into().unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: Amount::from_sat(10_000),
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      wallet_inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    );
  }

  #[test]
  fn batch_inscribe_over_max_standard_tx_weight() {
    let utxos = vec![(outpoint(1), Amount::from_sat(50 * COIN_VALUE))];

    let wallet_inscriptions = BTreeMap::new();

    let inscriptions = vec![
      inscription("text/plain", [0; MAX_STANDARD_TX_WEIGHT as usize / 3]),
      inscription("text/plain", [0; MAX_STANDARD_TX_WEIGHT as usize / 3]),
      inscription("text/plain", [0; MAX_STANDARD_TX_WEIGHT as usize / 3]),
    ];

    let commit_address = change(1);
    let reveal_addresses = vec![recipient()];

    let error = Batch {
      satpoint: None,
      parent_info: None,
      inscriptions,
      destinations: reveal_addresses,
      commit_fee_rate: 1.0.try_into().unwrap(),
      reveal_fee_rate: 1.0.try_into().unwrap(),
      no_limit: false,
      reinscribe: false,
      postage: Amount::from_sat(30_000),
      mode: Mode::SharedOutput,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      wallet_inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    )
    .unwrap_err()
    .to_string();

    assert!(
      error.contains(&format!("reveal transaction weight greater than {MAX_STANDARD_TX_WEIGHT} (MAX_STANDARD_TX_WEIGHT): 402841")),
      "{}",
      error
    );
  }

  #[test]
  fn batch_inscribe_into_separate_outputs() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(80_000)),
    ];

    let wallet_inscriptions = BTreeMap::new();

    let commit_address = change(1);
    let reveal_addresses = vec![recipient(), recipient(), recipient()];

    let inscriptions = vec![
      inscription("text/plain", [b'O'; 100]),
      inscription("text/plain", [b'O'; 111]),
      inscription("text/plain", [b'O'; 222]),
    ];

    let mode = Mode::SeparateOutputs;

    let fee_rate = 4.0.try_into().unwrap();

    let (_commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint: None,
      parent_info: None,
      inscriptions,
      destinations: reveal_addresses,
      commit_fee_rate: fee_rate,
      reveal_fee_rate: fee_rate,
      no_limit: false,
      reinscribe: false,
      postage: Amount::from_sat(10_000),
      mode,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      wallet_inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    )
    .unwrap();

    assert_eq!(reveal_tx.output.len(), 3);
    assert!(reveal_tx
      .output
      .iter()
      .all(|output| output.value == TARGET_POSTAGE.to_sat()));
  }

  #[test]
  fn batch_inscribe_into_separate_outputs_with_parent() {
    let utxos = vec![
      (outpoint(1), Amount::from_sat(10_000)),
      (outpoint(2), Amount::from_sat(50_000)),
    ];

    let parent = inscription_id(1);

    let parent_info = ParentInfo {
      destination: change(3),
      id: parent,
      location: SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      tx_out: TxOut {
        script_pubkey: change(0).script_pubkey(),
        value: 10000,
      },
    };

    let mut wallet_inscriptions = BTreeMap::new();
    wallet_inscriptions.insert(parent_info.location, parent);

    let commit_address = change(1);
    let reveal_addresses = vec![recipient(), recipient(), recipient()];

    let inscriptions = vec![
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
      InscriptionTemplate {
        parent: Some(parent),
        ..Default::default()
      }
      .into(),
    ];

    let mode = Mode::SeparateOutputs;

    let fee_rate = 4.0.try_into().unwrap();

    let (commit_tx, reveal_tx, _private_key, _) = Batch {
      satpoint: None,
      parent_info: Some(parent_info.clone()),
      inscriptions,
      destinations: reveal_addresses,
      commit_fee_rate: fee_rate,
      reveal_fee_rate: fee_rate,
      no_limit: false,
      reinscribe: false,
      postage: Amount::from_sat(10_000),
      mode,
      ..Default::default()
    }
    .create_batch_inscription_transactions(
      wallet_inscriptions,
      Chain::Signet,
      BTreeSet::new(),
      BTreeSet::new(),
      utxos.into_iter().collect(),
      [commit_address, change(2)],
    )
    .unwrap();

    assert_eq!(
      parent,
      ParsedEnvelope::from_transaction(&reveal_tx)[0]
        .payload
        .parent()
        .unwrap()
    );
    assert_eq!(
      parent,
      ParsedEnvelope::from_transaction(&reveal_tx)[1]
        .payload
        .parent()
        .unwrap()
    );

    let sig_vbytes = 17;
    let fee = fee_rate.fee(commit_tx.vsize() + sig_vbytes).to_sat();

    let reveal_value = commit_tx
      .output
      .iter()
      .map(|o| o.value)
      .reduce(|acc, i| acc + i)
      .unwrap();

    assert_eq!(reveal_value, 50_000 - fee);

    assert_eq!(
      reveal_tx.output[0].script_pubkey,
      parent_info.destination.script_pubkey()
    );
    assert_eq!(reveal_tx.output[0].value, parent_info.tx_out.value);
    pretty_assert_eq!(
      reveal_tx.input[0],
      TxIn {
        previous_output: parent_info.location.outpoint,
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        ..Default::default()
      }
    );
  }

  #[test]
  fn example_batchfile_deserializes_successfully() {
    Batchfile::load(Path::new("batch.yaml")).unwrap();
  }

  #[test]
  fn flags_conflict_with_batch() {
    for (flag, value) in [
      ("--file", Some("foo")),
      (
        "--delegate",
        Some("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33bi0"),
      ),
      (
        "--destination",
        Some("tb1qsgx55dp6gn53tsmyjjv4c2ye403hgxynxs0dnm"),
      ),
      ("--cbor-metadata", Some("foo")),
      ("--json-metadata", Some("foo")),
      (
        "--satpoint",
        Some("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b:0:0"),
      ),
      ("--reinscribe", None),
      ("--metaprotocol", Some("foo")),
      (
        "--parent",
        Some("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33bi0"),
      ),
    ] {
      let mut args = vec![
        "ord",
        "wallet",
        "inscribe",
        "--fee-rate",
        "1",
        "--batch",
        "foo.yaml",
        flag,
      ];

      if let Some(value) = value {
        args.push(value);
      }

      assert!(Arguments::try_parse_from(args)
        .unwrap_err()
        .to_string()
        .contains("the argument '--batch <BATCH>' cannot be used with"));
    }
  }

  #[test]
  fn batch_or_file_is_required() {
    assert!(
      Arguments::try_parse_from(["ord", "wallet", "inscribe", "--fee-rate", "1",])
        .unwrap_err()
        .to_string()
        .contains("error: the following required arguments were not provided:\n  <--file <FILE>|--batch <BATCH>>")
    );
  }

  #[test]
  fn satpoint_and_sat_flags_conflict() {
    assert_regex_match!(
      Arguments::try_parse_from([
        "ord",
        "--index-sats",
        "wallet",
        "inscribe",
        "--sat",
        "50000000000",
        "--satpoint",
        "038112028c55f3f77cc0b8b413df51f70675f66be443212da0642b7636f68a00:1:0",
        "--file",
        "baz",
      ])
      .unwrap_err()
      .to_string(),
      ".*--sat.*cannot be used with.*--satpoint.*"
    );
  }
}
