use super::*;

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct File {
  pub mode: Mode,
  #[serde(default)]
  pub parents: Vec<InscriptionId>,
  pub postage: Option<u64>,
  #[serde(default)]
  pub reinscribe: bool,
  pub sat: Option<Sat>,
  pub satpoint: Option<SatPoint>,
  pub inscriptions: Vec<batch::entry::Entry>,
  pub etching: Option<batch::Etching>,
}

impl File {
  pub(crate) fn load(path: &Path) -> Result<Self> {
    let batchfile: Self = serde_yaml::from_reader(fs::File::open(path)?)?;

    ensure!(
      !batchfile.inscriptions.is_empty(),
      "batchfile must contain at least one inscription",
    );

    let sat_and_satpoint = batchfile.sat.is_some() && batchfile.satpoint.is_some();

    ensure!(
      !sat_and_satpoint,
      "batchfile cannot set both `sat` and `satpoint`",
    );

    let sat_or_satpoint = batchfile.sat.is_some() || batchfile.satpoint.is_some();

    if sat_or_satpoint {
      ensure!(
        batchfile.mode == Mode::SameSat,
        "`sat` or `satpoint` can only be set in `same-sat` mode",
      );
    }

    if batchfile
      .inscriptions
      .iter()
      .any(|entry| entry.destination.is_some())
      && (batchfile.mode == Mode::SharedOutput || batchfile.mode == Mode::SameSat)
    {
      bail!(
        "individual inscription destinations cannot be set in `shared-output` or `same-sat` mode"
      );
    }

    let any_entry_has_satpoint = batchfile
      .inscriptions
      .iter()
      .any(|entry| entry.satpoint.is_some());

    if any_entry_has_satpoint {
      ensure!(
        batchfile.mode == Mode::SatPoints,
        "specifying `satpoint` in an inscription only works in `satpoints` mode"
      );

      ensure!(
        batchfile.inscriptions.iter().all(|entry| entry.satpoint.is_some()),
        "if `satpoint` is set for any inscription, then all inscriptions need to specify a satpoint"
      );

      ensure!(
        batchfile
          .inscriptions
          .iter()
          .all(|entry| entry.satpoint.unwrap().offset == 0),
        "`satpoint` can only be specified for first sat of an output"
      );
    }

    if batchfile.mode == Mode::SatPoints {
      ensure!(
        batchfile.postage.is_none(),
        "`postage` cannot be set if in `satpoints` mode"
      );

      ensure!(
        batchfile.sat.is_none(),
        "`sat` cannot be set if in `satpoints` mode"
      );

      ensure!(
        batchfile.satpoint.is_none(),
        "`satpoint cannot be set if in `satpoints` mode"
      );

      let mut seen = HashSet::new();
      for entry in batchfile.inscriptions.iter() {
        let satpoint = entry.satpoint.unwrap_or_default();
        if !seen.insert(satpoint) {
          bail!("duplicate satpoint {}", satpoint);
        }
      }
    }

    Ok(batchfile)
  }

  pub(crate) fn inscriptions(
    &self,
    wallet: &Wallet,
    utxos: &BTreeMap<OutPoint, TxOut>,
    parent_values: Vec<u64>,
    compress: bool,
  ) -> Result<(
    Vec<Inscription>,
    Vec<(SatPoint, TxOut)>,
    Vec<Amount>,
    Vec<Address>,
  )> {
    let mut inscriptions = Vec::new();
    let mut reveal_satpoints = Vec::new();
    let mut postages = Vec::new();

    let mut pointer = parent_values.iter().sum();

    for (i, entry) in self.inscriptions.iter().enumerate() {
      if let Some(delegate) = entry.delegate {
        ensure! {
          wallet.inscription_exists(delegate)?,
          "delegate {delegate} does not exist"
        }
      }

      inscriptions.push(Inscription::new(
        wallet.chain(),
        compress,
        entry.delegate.into_iter().collect(),
        entry.metadata()?,
        entry.metaprotocol.clone(),
        self.parents.clone(),
        entry.file.clone(),
        Some(pointer),
        self
          .etching
          .and_then(|etch| (i == 0).then_some(etch.rune.rune)),
      )?);

      let postage = if self.mode == Mode::SatPoints {
        let satpoint = entry
          .satpoint
          .ok_or_else(|| anyhow!("no satpoint specified for entry {i}"))?;

        let txout = utxos
          .get(&satpoint.outpoint)
          .ok_or_else(|| anyhow!("{} not in wallet", satpoint))?;

        reveal_satpoints.push((satpoint, txout.clone()));

        txout.value
      } else {
        self.postage.map(Amount::from_sat).unwrap_or(TARGET_POSTAGE)
      };

      pointer += postage.to_sat();

      if self.mode == Mode::SameSat && i > 0 {
        continue;
      } else {
        postages.push(postage);
      }
    }

    let destinations = match self.mode {
      Mode::SharedOutput | Mode::SameSat => vec![wallet.get_change_address()?],
      Mode::SeparateOutputs | Mode::SatPoints => self
        .inscriptions
        .iter()
        .map(|entry| {
          entry.destination.as_ref().map_or_else(
            || wallet.get_change_address(),
            |address| {
              address
                .clone()
                .require_network(wallet.chain().network())
                .map_err(|e| e.into())
            },
          )
        })
        .collect::<Result<Vec<_>, _>>()?,
    };

    Ok((inscriptions, reveal_satpoints, postages, destinations))
  }
}

#[cfg(test)]
mod tests {
  use {super::*, pretty_assertions::assert_eq};

  #[test]
  fn batchfile_not_sat_and_satpoint() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: same-sat
sat: 55555
satpoint: 4651dc5e964879b1eb9183d467d1187dcd252504698002b01853446c460db2c5:0:0
inscriptions:
- file: inscription.txt
- file: tulip.png
- file: meow.wav
"#,
    )
    .unwrap();

    assert_eq!(
      File::load(batch_file.as_path()).unwrap_err().to_string(),
      "batchfile cannot set both `sat` and `satpoint`"
    );
  }

  #[test]
  fn batchfile_wrong_mode_for_satpoints() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: separate-outputs
inscriptions:
- file: inscription.txt
  satpoint: bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0
- file: tulip.png
  satpoint: 5fddcbdc3eb21a93e8dd1dd3f9087c3677f422b82d5ba39a6b1ec37338154af6:0:0
- file: meow.wav
  satpoint: 4651dc5e964879b1eb9183d467d1187dcd252504698002b01853446c460db2c5:0:0
"#,
    )
    .unwrap();

    assert_eq!(
      batch::File::load(batch_file.as_path())
        .unwrap_err()
        .to_string(),
      "specifying `satpoint` in an inscription only works in `satpoints` mode"
    );
  }

  #[test]
  fn batchfile_missing_satpoint() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: satpoints
inscriptions:
- file: inscription.txt
  satpoint: bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0
- file: tulip.png
- file: meow.wav
  satpoint: 4651dc5e964879b1eb9183d467d1187dcd252504698002b01853446c460db2c5:0:0
"#,
    )
    .unwrap();

    assert_eq!(
      batch::File::load(batch_file.as_path())
        .unwrap_err()
        .to_string(),
      "if `satpoint` is set for any inscription, then all inscriptions need to specify a satpoint"
    );
  }

  #[test]
  fn batchfile_only_first_sat_of_outpoint() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: satpoints
inscriptions:
- file: inscription.txt
  satpoint: bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0
- file: tulip.png
  satpoint: 5fddcbdc3eb21a93e8dd1dd3f9087c3677f422b82d5ba39a6b1ec37338154af6:0:21
- file: meow.wav
  satpoint: 4651dc5e964879b1eb9183d467d1187dcd252504698002b01853446c460db2c5:0:0
"#,
    )
    .unwrap();

    assert_eq!(
      batch::File::load(batch_file.as_path())
        .unwrap_err()
        .to_string(),
      "`satpoint` can only be specified for first sat of an output"
    );
  }

  #[test]
  fn batchfile_no_postage_if_mode_satpoints() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: satpoints
postage: 1111
inscriptions:
- file: inscription.txt
  satpoint: bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0
- file: tulip.png
  satpoint: 5fddcbdc3eb21a93e8dd1dd3f9087c3677f422b82d5ba39a6b1ec37338154af6:0:0
- file: meow.wav
  satpoint: 4651dc5e964879b1eb9183d467d1187dcd252504698002b01853446c460db2c5:0:0
"#,
    )
    .unwrap();

    assert_eq!(
      batch::File::load(batch_file.as_path())
        .unwrap_err()
        .to_string(),
      "`postage` cannot be set if in `satpoints` mode"
    );
  }

  #[test]
  fn batchfile_no_duplicate_satpoints() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: satpoints
inscriptions:
- file: inscription.txt
  satpoint: bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0
- file: tulip.png
  satpoint: 5fddcbdc3eb21a93e8dd1dd3f9087c3677f422b82d5ba39a6b1ec37338154af6:0:0
- file: meow.wav
  satpoint: 4651dc5e964879b1eb9183d467d1187dcd252504698002b01853446c460db2c5:0:0
- file: inscription_1.txt
  satpoint: bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0
"#,
    )
    .unwrap();

    assert_eq!(
      batch::File::load(batch_file.as_path())
        .unwrap_err()
        .to_string(),
      "duplicate satpoint bc4c30829a9564c0d58e6287195622b53ced54a25711d1b86be7cd3a70ef61ed:0:0"
    );
  }

  #[test]
  fn example_batchfile_deserializes_successfully() {
    assert_eq!(
      batch::File::load(Path::new("batch.yaml")).unwrap(),
      batch::File {
        mode: batch::Mode::SeparateOutputs,
        parents: vec![
          "6ac5cacb768794f4fd7a78bf00f2074891fce68bd65c4ff36e77177237aacacai0"
            .parse()
            .unwrap()
        ],
        postage: Some(12345),
        reinscribe: true,
        sat: None,
        satpoint: None,
        etching: Some(Etching {
          rune: "THE•BEST•RUNE".parse().unwrap(),
          divisibility: 2,
          premine: "1000.00".parse().unwrap(),
          supply: "10000.00".parse().unwrap(),
          symbol: '$',
          terms: Some(batch::Terms {
            amount: "100.00".parse().unwrap(),
            cap: 90,
            height: Some(batch::Range {
              start: Some(840000),
              end: Some(850000),
            }),
            offset: Some(batch::Range {
              start: Some(1000),
              end: Some(9000),
            }),
          }),
          turbo: true,
        }),
        inscriptions: vec![
          batch::Entry {
            file: Some("mango.avif".into()),
            delegate: Some(
              "6ac5cacb768794f4fd7a78bf00f2074891fce68bd65c4ff36e77177237aacacai0"
                .parse()
                .unwrap()
            ),
            destination: Some(
              "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
                .parse()
                .unwrap()
            ),
            metadata: Some(serde_yaml::Value::Mapping({
              let mut mapping = serde_yaml::Mapping::new();
              mapping.insert("title".into(), "Delicious Mangos".into());
              mapping.insert(
                "description".into(),
                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Aliquam semper, \
                ligula ornare laoreet tincidunt, odio nisi euismod tortor, vel blandit \
                metus est et odio. Nullam venenatis, urna et molestie vestibulum, orci \
                mi efficitur risus, eu malesuada diam lorem sed velit. Nam fermentum \
                dolor et luctus euismod.\n"
                  .into(),
              );
              mapping
            })),
            ..default()
          },
          batch::Entry {
            file: Some("token.json".into()),
            metaprotocol: Some("DOPEPROTOCOL-42069".into()),
            ..default()
          },
          batch::Entry {
            file: Some("tulip.png".into()),
            destination: Some(
              "bc1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdyspfxcv6"
                .parse()
                .unwrap()
            ),
            metadata: Some(serde_yaml::Value::Mapping({
              let mut mapping = serde_yaml::Mapping::new();
              mapping.insert("author".into(), "Satoshi Nakamoto".into());
              mapping
            })),
            ..default()
          },
        ],
      }
    );
  }

  #[test]
  fn batchfile_no_delegate_no_file_allowed() {
    let tempdir = TempDir::new().unwrap();
    let batch_file = tempdir.path().join("batch.yaml");
    fs::write(
      batch_file.clone(),
      r#"
mode: shared-output
inscriptions:
  -
"#,
    )
    .unwrap();

    assert!(batch::File::load(batch_file.as_path()).is_ok());
  }
}
