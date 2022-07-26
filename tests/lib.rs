#![allow(clippy::type_complexity)]

use {
  crate::rpc_server::RpcServer,
  bitcoin::{
    blockdata::constants::COIN_VALUE, blockdata::script, consensus::Encodable, Block, BlockHash,
    BlockHeader, OutPoint, Transaction, TxIn, TxOut, Witness,
  },
  bitcoind::{
    bitcoincore_rpc::{
      bitcoin::{Address, Amount},
      RpcApi,
    },
    BitcoinD as Bitcoind,
  },
  core::str::FromStr,
  executable_path::executable_path,
  nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
  },
  regex::Regex,
  std::{
    collections::BTreeMap,
    error::Error,
    ffi::OsString,
    fs,
    net::TcpListener,
    process::{Command, Stdio},
    str,
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::{Duration, Instant},
  },
  tempfile::TempDir,
  unindent::Unindent,
};

mod epochs;
mod find;
mod index;
mod info;
mod list;
mod name;
mod nft;
mod range;
mod rpc_server;
mod server;
mod supply;
mod traits;
mod version;
mod wallet;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

enum Expected {
  String(String),
  Regex(Regex),
  Ignore,
}

enum Event<'a> {
  Block,
  Request(String, u16, String),
  Transaction(TransactionOptions<'a>),
}

struct Output {
  calls: Vec<String>,
  stdout: String,
  tempdir: TempDir,
}

struct TransactionOptions<'a> {
  slots: &'a [(usize, usize, usize)],
  output_count: usize,
  fee: u64,
}

struct Test<'a> {
  address: Address,
  args: Vec<String>,
  bitcoind: Bitcoind,
  envs: Vec<(OsString, OsString)>,
  events: Vec<Event<'a>>,
  expected_status: i32,
  expected_stderr: Expected,
  expected_stdout: Expected,
  tempdir: TempDir,
}

impl<'a> Test<'a> {
  fn new() -> Result<Self> {
    Ok(Self::with_tempdir(TempDir::new()?)?)
  }

  fn with_tempdir(tempdir: TempDir) -> Result<Self> {
    let mut conf = bitcoind::Conf::default();

    conf.view_stdout = true;

    Ok(Self {
      address: Address::from_str("bcrt1qjcgxtte2ttzmvugn874y0n7j9jc82j0y6qvkvn")?,
      args: Vec::new(),
      bitcoind: Bitcoind::with_conf(bitcoind::downloaded_exe_path()?, &conf)?,
      envs: Vec::new(),
      events: Vec::new(),
      expected_status: 0,
      expected_stderr: Expected::Ignore,
      expected_stdout: Expected::String(String::new()),
      tempdir,
    })
  }

  fn command(self, args: &str) -> Self {
    Self {
      args: args.split_whitespace().map(str::to_owned).collect(),
      ..self
    }
  }

  fn args(self, args: &[&str]) -> Self {
    Self {
      args: self
        .args
        .into_iter()
        .chain(args.iter().cloned().map(str::to_owned))
        .collect(),
      ..self
    }
  }

  fn expected_stdout(self, expected_stdout: impl AsRef<str>) -> Self {
    Self {
      expected_stdout: Expected::String(expected_stdout.as_ref().to_owned()),
      ..self
    }
  }

  fn stdout_regex(self, expected_stdout: impl AsRef<str>) -> Self {
    Self {
      expected_stdout: Expected::Regex(
        Regex::new(&format!("(?s)^{}$", expected_stdout.as_ref())).unwrap(),
      ),
      ..self
    }
  }

  fn set_home_to_tempdir(mut self) -> Self {
    self
      .envs
      .push((OsString::from("HOME"), OsString::from(self.tempdir.path())));

    self
  }

  fn expected_stderr(self, expected_stderr: &str) -> Self {
    Self {
      expected_stderr: Expected::String(expected_stderr.to_owned()),
      ..self
    }
  }

  fn stderr_regex(self, expected_stderr: impl AsRef<str>) -> Self {
    Self {
      expected_stderr: Expected::Regex(
        Regex::new(&format!("(?s)^{}$", expected_stderr.as_ref())).unwrap(),
      ),
      ..self
    }
  }

  fn expected_status(self, expected_status: i32) -> Self {
    Self {
      expected_status,
      ..self
    }
  }

  fn ignore_stdout(self) -> Self {
    Self {
      expected_stdout: Expected::Ignore,
      ..self
    }
  }

  fn request(mut self, path: &str, status: u16, response: &str) -> Self {
    self.events.push(Event::Request(
      path.to_string(),
      status,
      response.to_string(),
    ));
    self
  }

  fn run(self) -> Result {
    self.test(None).map(|_| ())
  }

  fn output(self) -> Result<Output> {
    self.test(None)
  }

  fn run_server(self, port: u16) -> Result {
    self.test(Some(port)).map(|_| ())
  }

  fn get_block(&self, height: u64) -> Result<Block> {
    let block = self
      .bitcoind
      .client
      .get_block(&self.bitcoind.client.get_block_hash(height)?)?;

    Ok(block)
  }

  fn test(self, port: Option<u16>) -> Result<Output> {
    for event in &self.events {
      match event {
        Event::Block => {
          eprintln!("mining block !");
          self.bitcoind.client.generate_to_address(1, &self.address)?;
        }
        Event::Request(request, status, expected_response) => {
          panic!()
        }
        Event::Transaction(options) => {
          let input_value = options
            .slots
            .iter()
            .map(|slot| {
              self.get_block(slot.0 as u64).unwrap().txdata[slot.1].output[slot.2].value
            })
            .sum::<u64>();

          let output_value = input_value - options.fee;

          let tx = Transaction {
            version: 0,
            lock_time: 0,
            input: options
              .slots
              .iter()
              .map(|slot| TxIn {
                previous_output: OutPoint {
                  txid: self.get_block(slot.0 as u64).unwrap().txdata[slot.1].txid(),
                  vout: slot.2 as u32,
                },
                script_sig: script::Builder::new().into_script(),
                sequence: 0,
                witness: Witness::new(),
              })
              .collect(),
            output: vec![
              TxOut {
                value: output_value / options.output_count as u64,
                script_pubkey: script::Builder::new().into_script(),
              };
              options.output_count
            ],
          };

          self.bitcoind.client.send_raw_transaction(&tx)?;
        }
      }
    }
    // for (b, block) in self.blocks().enumerate() {
    //   for (t, transaction) in block.txdata.iter().enumerate() {
    //     eprintln!("{b}.{t}: {}", transaction.txid());
    //   }
    // }

    let (blocks, close_handle, calls, rpc_server_port) = if port.is_some() {
      RpcServer::spawn(Vec::new())
    } else {
      RpcServer::spawn(Vec::new())
    };

    let child = Command::new(executable_path("ord"))
      .envs(self.envs.clone())
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(if !matches!(self.expected_stderr, Expected::Ignore) {
        Stdio::piped()
      } else {
        Stdio::inherit()
      })
      .current_dir(&self.tempdir)
      .arg(format!(
        "--rpc-url={}",
        self.bitcoind.params.rpc_socket.to_string()
      ))
      .arg(format!(
        "--cookie-file={}",
        self.bitcoind.params.cookie_file.display()
      ))
      .args(self.args.clone())
      .spawn()?;

    let mut successful_requests = 0;

    dbg!(port);

    if let Some(port) = port {
      let client = reqwest::blocking::Client::new();

      let start = Instant::now();
      let mut healthy = false;

      loop {
        if let Ok(response) = client
          .get(&format!("http://127.0.0.1:{port}/status"))
          .send()
        {
          if response.status().is_success() {
            healthy = true;
            break;
          }
        }

        if Instant::now() - start > Duration::from_secs(1) {
          break;
        }

        sleep(Duration::from_millis(100));
      }

      dbg!(healthy);

      if healthy {
        for event in &self.events {
          match event {
            Event::Block => {
              eprintln!("mining block !");
              self.bitcoind.client.generate_to_address(1, &self.address)?;
            }
            Event::Request(request, status, expected_response) => {
              let response = client
                .get(&format!("http://127.0.0.1:{port}/{request}"))
                .send()?;
              assert_eq!(response.status().as_u16(), *status);
              assert_eq!(response.text()?, *expected_response);
              successful_requests += 1;
            }
            Event::Transaction(options) => {
              let input_value = options
                .slots
                .iter()
                .map(|slot| {
                  self.get_block(slot.0 as u64).unwrap().txdata[slot.1].output[slot.2].value
                })
                .sum::<u64>();

              let output_value = input_value - options.fee;

              let tx = Transaction {
                version: 0,
                lock_time: 0,
                input: options
                  .slots
                  .iter()
                  .map(|slot| TxIn {
                    previous_output: OutPoint {
                      txid: self.get_block(slot.0 as u64).unwrap().txdata[slot.1].txid(),
                      vout: slot.2 as u32,
                    },
                    script_sig: script::Builder::new().into_script(),
                    sequence: 0,
                    witness: Witness::new(),
                  })
                  .collect(),
                output: vec![
                  TxOut {
                    value: output_value / options.output_count as u64,
                    script_pubkey: script::Builder::new().into_script(),
                  };
                  options.output_count
                ],
              };

              self.bitcoind.client.send_raw_transaction(&tx)?;
            }
          }
        }
      }

      signal::kill(Pid::from_raw(child.id() as i32), Signal::SIGINT)?;
    }

    let output = child.wait_with_output()?;

    close_handle.close();

    let stdout = str::from_utf8(&output.stdout)?;
    let stderr = str::from_utf8(&output.stderr)?;

    if output.status.code() != Some(self.expected_status) {
      panic!(
        "Test failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status, stdout, stderr
      );
    }

    let log_line_re = Regex::new(r"(?m)^\[.*\n")?;

    for log_line in log_line_re.find_iter(stderr) {
      print!("{}", log_line.as_str())
    }

    let stripped_stderr = log_line_re.replace_all(stderr, "");

    match self.expected_stderr {
      Expected::String(expected_stderr) => assert_eq!(stripped_stderr, expected_stderr),
      Expected::Regex(expected_stderr) => assert!(
        expected_stderr.is_match(&stripped_stderr),
        "stderr did not match regex: {:?}",
        stripped_stderr
      ),
      Expected::Ignore => {}
    }

    match self.expected_stdout {
      Expected::String(expected_stdout) => assert_eq!(stdout, expected_stdout),
      Expected::Regex(expected_stdout) => assert!(
        expected_stdout.is_match(stdout),
        "stdout did not match regex: {:?}",
        stdout
      ),
      Expected::Ignore => {}
    }

    assert_eq!(
      successful_requests,
      self
        .events
        .iter()
        .filter(|event| matches!(event, Event::Request(..)))
        .count(),
      "Unsuccessful requests"
    );

    let calls = calls.lock().unwrap().clone();

    Ok(Output {
      stdout: stdout.to_string(),
      tempdir: self.tempdir,
      calls,
    })
  }

  fn block(mut self) -> Self {
    self.events.push(Event::Block);
    self
  }

  fn transaction(mut self, options: TransactionOptions<'a>) -> Self {
    self.events.push(Event::Transaction(options));
    self
  }

  fn write(self, path: &str, contents: &str) -> Result<Self> {
    fs::write(self.tempdir.path().join(path), contents)?;
    Ok(self)
  }
}
