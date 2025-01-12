use {super::*, ciborium::value::Integer, ord::subcommand::wallet::send::Output};

#[test]
fn run() {
  let core = mockcore::spawn();

  let port = TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port();

  let builder =
    CommandBuilder::new(format!("server --address 127.0.0.1 --http-port {port}")).core(&core);

  let mut command = builder.command();

  let mut child = command.spawn().unwrap();

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}/status")) {
      if response.status() == 200 {
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond to status check",);
    }

    thread::sleep(Duration::from_millis(50));
  }

  child.kill().unwrap();
  child.wait().unwrap();
}

#[test]
fn address_page_shows_outputs_and_sat_balance() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_args(&core, &["--index-addresses"]);

  create_wallet(&core, &ord);
  core.mine_blocks(1);

  let address = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

  let send = CommandBuilder::new(format!("wallet send --fee-rate 13.3 {address} 2btc"))
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Send>();

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/address/{address}"),
    format!(
      ".*<h1>Address {address}</h1>.*<dd>200000000</dd>.*<a class=collapse href=/output/{}.*",
      OutPoint {
        txid: send.txid,
        vout: 0
      }
    ),
  );
}

#[test]
fn address_page_shows_single_rune() {
  let core = mockcore::builder().network(Network::Regtest).build();
  let ord =
    TestServer::spawn_with_args(&core, &["--index-runes", "--index-addresses", "--regtest"]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  let address = "bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw";

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} 1000:{}",
    Rune(RUNE)
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(6);

  ord.assert_response_regex(
    format!("/address/{address}"),
    format!(".*<dd>.*{}.*: 1000¢</dd>.*", Rune(RUNE)),
  );
}

#[test]
fn address_page_shows_multiple_runes() {
  let core = mockcore::builder().network(Network::Regtest).build();
  let ord =
    TestServer::spawn_with_args(&core, &["--index-runes", "--index-addresses", "--regtest"]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));
  etch(&core, &ord, Rune(RUNE + 1));

  let address = "bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw";

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} 1000:{}",
    Rune(RUNE)
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(6);

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} 1000:{}",
    Rune(RUNE + 1)
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(6);

  ord.assert_response_regex(
    format!("/address/{address}"),
    format!(
      ".*<dd>.*{}.*: 1000¢</dd>.*<dd>.*{}.*: 1000¢</dd>.*",
      Rune(RUNE),
      Rune(RUNE + 1)
    ),
  );
}

#[test]
fn address_page_shows_aggregated_runes_balance() {
  let core = mockcore::builder().network(Network::Regtest).build();
  let ord =
    TestServer::spawn_with_args(&core, &["--index-runes", "--index-addresses", "--regtest"]);

  create_wallet(&core, &ord);

  etch(&core, &ord, Rune(RUNE));

  let address = "bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw";

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} 250:{}",
    Rune(RUNE)
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(6);

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} 250:{}",
    Rune(RUNE)
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(6);

  ord.assert_response_regex(
    format!("/address/{address}"),
    format!(".*<dd>.*{}.*: 500¢</dd>.*", Rune(RUNE)),
  );
}

#[test]
fn address_page_shows_aggregated_inscriptions() {
  let core = mockcore::builder().network(Network::Regtest).build();
  let ord =
    TestServer::spawn_with_args(&core, &["--index-runes", "--index-addresses", "--regtest"]);

  create_wallet(&core, &ord);

  let (inscription_id_1, _reveal) = inscribe(&core, &ord);

  let address = "bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw";

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} {inscription_id_1}",
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(1);

  let (inscription_id_2, _reveal) = inscribe(&core, &ord);

  CommandBuilder::new(format!(
    "--chain regtest --index-runes wallet send --fee-rate 1 {address} {inscription_id_2}",
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>();

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/address/{address}"),
      r".*
<dl>.*
  <dt>inscriptions</dt>
  <dd class=thumbnails>
    <a href=/inscription/[[:xdigit:]]{64}i\d><iframe .* src=/preview/[[:xdigit:]]{64}i\d></iframe></a>
    <a href=/inscription/[[:xdigit:]]{64}i\d><iframe .* src=/preview/[[:xdigit:]]{64}i\d></iframe></a>
  </dd>.*"
    ,
  );
}

#[test]
fn inscription_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (inscription, reveal) = inscribe(&core, &ord);

  let response = ord.json_request(format!(
    "/output/{}",
    OutPoint {
      txid: reveal,
      vout: 0
    }
  ));

  assert_eq!(response.status(), StatusCode::OK);

  let output: api::Output = serde_json::from_str(&response.text().unwrap()).unwrap();

  TestServer::spawn_with_args(&core, &[]).assert_html(
    format!("/inscription/{inscription}"),
    Chain::Mainnet,
    InscriptionHtml {
      chain: Chain::Mainnet,
      charms: 0,
      child_count: 0,
      children: Vec::new(),
      fee: 138,
      height: 2,
      inscription: Inscription {
        content_type: Some("text/plain;charset=utf-8".as_bytes().into()),
        body: Some("foo".as_bytes().into()),
        ..default()
      },
      id: inscription,
      number: 0,
      next: None,
      output: Some(TxOut {
        value: Amount::from_sat(10000),
        script_pubkey: output.script_pubkey,
      }),
      parents: Vec::new(),
      previous: None,
      rune: None,
      sat: None,
      satpoint: SatPoint {
        outpoint: OutPoint {
          txid: reveal,
          vout: 0,
        },
        offset: 0,
      },
      timestamp: "1970-01-01 00:00:02+00:00"
        .parse::<DateTime<Utc>>()
        .unwrap(),
    },
  );
}

#[test]
fn inscription_appears_on_reveal_transaction_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (_, reveal) = inscribe(&core, &ord);

  core.mine_blocks(1);

  TestServer::spawn_with_args(&core, &[]).assert_response_regex(
    format!("/tx/{reveal}"),
    format!(".*<h1>Transaction .*</h1>.*<a href=/inscription/{reveal}.*"),
  );
}

#[test]
fn multiple_inscriptions_appear_on_reveal_transaction_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new("wallet batch --batch batch.yaml --fee-rate 55")
    .write("inscription.txt", "Hello World")
    .write("meow.wav", [0; 2048])
    .write(
      "batch.yaml",
      "mode: shared-output\ninscriptions:\n- file: inscription.txt\n- file: meow.wav\n",
    )
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  let id0 = output.inscriptions[0].id;
  let id1 = output.inscriptions[1].id;
  let reveal = output.reveal;

  ord.assert_response_regex(
    format!("/tx/{reveal}"),
    format!(".*<h1>Transaction .*</h1>.*<a href=/inscription/{id0}.*<a href=/inscription/{id1}.*"),
  );
}

#[test]
fn inscription_appears_on_output_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (inscription, reveal) = inscribe(&core, &ord);

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/output/{reveal}:0"),
    format!(".*<h1>Output <span class=monospace>{reveal}:0</span></h1>.*<a href=/inscription/{inscription}.*"),
  );
}

#[test]
fn inscription_page_after_send() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (inscription, reveal) = inscribe(&core, &ord);

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      r".*<h1>Inscription 0</h1>.*<dt>location</dt>\s*<dd><a class=collapse href=/satpoint/{reveal}:0:0>{reveal}:0:0</a></dd>.*",
    ),
  );

  let txid = CommandBuilder::new(format!(
    "wallet send --fee-rate 1 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {inscription}"
  ))
  .core(&core)
  .ord(&ord)
  .stdout_regex(".*")
  .run_and_deserialize_output::<Output>()
  .txid;

  core.mine_blocks(1);

  ord.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      r".*<h1>Inscription 0</h1>.*<dt>address</dt>\s*<dd><a class=collapse href=/address/bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv>bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv</a></dd>.*<dt>location</dt>\s*<dd><a class=collapse href=/satpoint/{txid}:0:0>{txid}:0:0</a></dd>.*",
    ),
  )
}

#[test]
fn inscription_content() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (inscription, _) = inscribe(&core, &ord);

  core.mine_blocks(1);

  let response = ord.request(format!("/content/{inscription}"));

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response.headers().get("content-type").unwrap(),
    "text/plain;charset=utf-8"
  );
  assert_eq!(
    response
      .headers()
      .get_all("content-security-policy")
      .into_iter()
      .collect::<Vec<&reqwest::header::HeaderValue>>(),
    &[
      "default-src 'self' 'unsafe-eval' 'unsafe-inline' data: blob:",
      "default-src *:*/content/ *:*/blockheight *:*/blockhash *:*/blockhash/ *:*/blocktime *:*/r/ 'unsafe-eval' 'unsafe-inline' data: blob:",
    ]
  );
  assert_eq!(response.bytes().unwrap(), "FOO");
}

#[test]
fn inscription_metadata() {
  let metadata = r#"{"foo":"bar","baz":1}"#;
  let mut encoded_metadata = Vec::new();
  let cbor_map = ciborium::value::Value::Map(vec![
    (
      ciborium::value::Value::Text("foo".into()),
      ciborium::value::Value::Text("bar".into()),
    ),
    (
      ciborium::value::Value::Text("baz".into()),
      ciborium::value::Value::Integer(Integer::from(1)),
    ),
  ]);
  ciborium::ser::into_writer(&cbor_map, &mut encoded_metadata).unwrap();

  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let inscription_id = CommandBuilder::new(
    "wallet inscribe --fee-rate 1 --json-metadata metadata.json --file foo.txt",
  )
  .write("foo.txt", "FOO")
  .write("metadata.json", metadata)
  .core(&core)
  .ord(&ord)
  .run_and_deserialize_output::<Batch>()
  .inscriptions
  .first()
  .unwrap()
  .id;

  core.mine_blocks(1);

  let response = ord.request(format!("/r/metadata/{inscription_id}"));

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response.headers().get("content-type").unwrap(),
    "application/json"
  );
  assert_eq!(
    response.text().unwrap(),
    format!("\"{}\"", hex::encode(encoded_metadata))
  );
}

#[test]
fn recursive_inscription_endpoint() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn_with_server_args(&core, &["--index-sats"], &[]);

  create_wallet(&core, &ord);

  core.mine_blocks(1);

  let output = CommandBuilder::new("wallet inscribe --fee-rate 1 --file foo.txt")
    .write("foo.txt", "FOO")
    .core(&core)
    .ord(&ord)
    .run_and_deserialize_output::<Batch>();

  core.mine_blocks(1);

  let inscription = output.inscriptions.first().unwrap();
  let response = ord.request(format!("/r/inscription/{}", inscription.id));

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response.headers().get("content-type").unwrap(),
    "application/json"
  );

  let mut inscription_recursive_json: api::InscriptionRecursive =
    serde_json::from_str(&response.text().unwrap()).unwrap();

  assert_regex_match!(inscription_recursive_json.address.unwrap(), r"bc1p.*");
  inscription_recursive_json.address = None;

  pretty_assert_eq!(
    inscription_recursive_json,
    api::InscriptionRecursive {
      charms: vec![Charm::Coin, Charm::Uncommon],
      content_type: Some("text/plain;charset=utf-8".to_string()),
      content_length: Some(3),
      delegates: Vec::new(),
      fee: 138,
      height: 2,
      id: inscription.id,
      number: 0,
      output: inscription.location.outpoint,
      sat: Some(Sat(50 * COIN_VALUE)),
      satpoint: SatPoint {
        outpoint: inscription.location.outpoint,
        offset: 0,
      },
      timestamp: 2,
      value: Some(10000),
      address: None,
    }
  )
}

#[test]
fn inscriptions_page() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (inscription, _) = inscribe(&core, &ord);

  ord.assert_response_regex(
    "/inscriptions",
    format!(
      ".*<h1>All Inscriptions</h1>
<div class=thumbnails>
  <a href=/inscription/{inscription}>.*</a>
</div>
.*",
    ),
  );
}

#[test]
fn inscriptions_page_is_sorted() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let mut regex = String::new();

  for _ in 0..8 {
    let (inscription, _) = inscribe(&core, &ord);
    regex.insert_str(0, &format!(".*<a href=/inscription/{inscription}>.*"));
  }

  ord.assert_response_regex("/inscriptions", &regex);
}

#[test]
fn inscriptions_page_has_next_and_previous() {
  let core = mockcore::spawn();
  let ord = TestServer::spawn(&core);

  create_wallet(&core, &ord);

  let (a, _) = inscribe(&core, &ord);
  let (b, _) = inscribe(&core, &ord);
  let (c, _) = inscribe(&core, &ord);

  ord.assert_response_regex(
    format!("/inscription/{b}"),
    format!(
      ".*<h1>Inscription 1</h1>.*
<div class=inscription>
<a class=prev href=/inscription/{a}>❮</a>
<iframe .* src=/preview/{b}></iframe>
<a class=next href=/inscription/{c}>❯</a>
</div>.*",
    ),
  );
}

#[test]
fn expected_sat_time_is_rounded() {
  let core = mockcore::spawn();

  TestServer::spawn_with_args(&core, &[]).assert_response_regex(
    "/sat/2099999997689999",
    r".*<dt>timestamp</dt><dd><time>.* \d+:\d+:\d+ UTC</time> \(expected\)</dd>.*",
  );
}

#[test]
fn missing_credentials() {
  let core = mockcore::spawn();

  CommandBuilder::new("--bitcoin-rpc-username foo server")
    .core(&core)
    .expected_exit_code(1)
    .expected_stderr("error: no bitcoin RPC password specified\n")
    .run_and_extract_stdout();

  CommandBuilder::new("--bitcoin-rpc-password bar server")
    .core(&core)
    .expected_exit_code(1)
    .expected_stderr("error: no bitcoin RPC username specified\n")
    .run_and_extract_stdout();
}

#[test]
fn all_endpoints_in_recursive_directory_return_json() {
  let core = mockcore::spawn();

  core.mine_blocks(2);

  let ord_server = TestServer::spawn_with_args(&core, &[]);

  assert_eq!(
    ord_server.request("/r/blockheight").json::<u64>().unwrap(),
    2
  );

  assert_eq!(ord_server.request("/r/blocktime").json::<u64>().unwrap(), 2);

  assert_eq!(
    ord_server.request("/r/blockhash").json::<String>().unwrap(),
    "70a93647a8d559c7e7ff2df9bd875f5b726a2ff8ca3562003d257df5a4c47ae2"
  );

  assert_eq!(
    ord_server
      .request("/r/blockhash/0")
      .json::<String>()
      .unwrap(),
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
  );

  assert!(ord_server.request("/blockhash").json::<String>().is_err());

  assert!(ord_server.request("/blockhash/2").json::<String>().is_err());
}

#[test]
fn sat_recursive_endpoints_without_sat_index_return_404() {
  let core = mockcore::spawn();

  core.mine_blocks(1);

  let server = TestServer::spawn_with_args(&core, &[""]);

  assert_eq!(
    server.request("/r/sat/5000000000").status(),
    StatusCode::NOT_FOUND,
  );

  assert_eq!(
    server.request("/r/sat/5000000000/at/1").status(),
    StatusCode::NOT_FOUND,
  );
}

#[test]
fn inscription_transactions_are_stored_with_transaction_index() {
  let core = mockcore::spawn();

  let ord = TestServer::spawn_with_server_args(&core, &["--index-transactions"], &[]);

  create_wallet(&core, &ord);

  let (_inscription, reveal) = inscribe(&core, &ord);

  let coinbase = core.tx(1, 0).compute_txid();

  assert_eq!(
    ord.request(format!("/tx/{reveal}")).status(),
    StatusCode::OK,
  );

  assert_eq!(
    ord.request(format!("/tx/{coinbase}")).status(),
    StatusCode::OK,
  );

  core.clear_state();

  assert_eq!(
    ord.request(format!("/tx/{reveal}")).status(),
    StatusCode::OK,
  );

  assert_eq!(
    ord.request(format!("/tx/{coinbase}")).status(),
    StatusCode::NOT_FOUND,
  );
}

#[test]
fn run_no_sync() {
  let core = mockcore::spawn();

  let port = TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port();

  let tempdir = Arc::new(TempDir::new().unwrap());

  let builder = CommandBuilder::new(format!("server --address 127.0.0.1 --http-port {port}",))
    .core(&core)
    .temp_dir(tempdir.clone());

  let mut command = builder.command();

  let mut child = command.spawn().unwrap();

  core.mine_blocks(1);

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}/blockheight")) {
      if response.status() == 200 {
        assert_eq!(response.text().unwrap(), "1");
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond to status check",);
    }

    thread::sleep(Duration::from_millis(50));
  }

  child.kill().unwrap();
  child.wait().unwrap();

  let builder = CommandBuilder::new(format!(
    "server --no-sync --address 127.0.0.1 --http-port {port}",
  ))
  .core(&core)
  .temp_dir(tempdir);

  let mut command = builder.command();

  let mut child = command.spawn().unwrap();

  core.mine_blocks(2);

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}/blockheight")) {
      if response.status() == 200 {
        assert_eq!(response.text().unwrap(), "1");
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond to status check",);
    }

    thread::sleep(Duration::from_millis(50));
  }

  child.kill().unwrap();
  child.wait().unwrap();
}

#[test]
fn authentication() {
  let core = mockcore::spawn();

  let port = TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port();

  let builder = CommandBuilder::new(format!(
    " --server-username foo --server-password bar server --address 127.0.0.1 --http-port {port}"
  ))
  .core(&core);

  let mut command = builder.command();

  let mut child = command.spawn().unwrap();

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}")) {
      if response.status() == 401 {
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond");
    }

    thread::sleep(Duration::from_millis(50));
  }

  let response = reqwest::blocking::Client::new()
    .get(format!("http://localhost:{port}"))
    .basic_auth("foo", Some("bar"))
    .send()
    .unwrap();

  assert_eq!(response.status(), 200);

  child.kill().unwrap();
  child.wait().unwrap();
}

#[cfg(unix)]
#[test]
fn ctrl_c() {
  use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
  };

  let core = mockcore::spawn();

  let port = TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port();

  let tempdir = Arc::new(TempDir::new().unwrap());

  core.mine_blocks(3);

  let mut spawn = CommandBuilder::new(format!("server --address 127.0.0.1 --http-port {port}"))
    .temp_dir(tempdir.clone())
    .core(&core)
    .spawn();

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}/blockcount")) {
      if response.status() == 200 || response.text().unwrap() == *"3" {
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond to status check",);
    }

    thread::sleep(Duration::from_millis(50));
  }

  signal::kill(
    Pid::from_raw(spawn.child.id().try_into().unwrap()),
    Signal::SIGINT,
  )
  .unwrap();

  let mut buffer = String::new();
  BufReader::new(spawn.child.stderr.as_mut().unwrap())
    .read_line(&mut buffer)
    .unwrap();

  assert_eq!(
    buffer,
    "Shutting down gracefully. Press <CTRL-C> again to shutdown immediately.\n"
  );

  spawn.child.wait().unwrap();

  CommandBuilder::new(format!(
    "server --no-sync --address 127.0.0.1 --http-port {port}"
  ))
  .temp_dir(tempdir)
  .core(&core)
  .spawn();

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}/blockcount")) {
      if response.status() == 200 || response.text().unwrap() == *"3" {
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond to status check",);
    }

    thread::sleep(Duration::from_millis(50));
  }
}
