#![allow(
  clippy::large_enum_variant,
  clippy::result_large_err,
  clippy::too_many_arguments,
  clippy::type_complexity
)]
#![deny(
  clippy::cast_lossless,
  clippy::cast_possible_truncation,
  clippy::cast_possible_wrap,
  clippy::cast_sign_loss
)]

use {
  self::{
    arguments::Arguments,
    blocktime::Blocktime,
    decimal::Decimal,
    deserialize_from_str::DeserializeFromStr,
    index::BitcoinCoreRpcResultExt,
    inscriptions::{
      inscription_id,
      media::{self, ImageRendering, Media},
      teleburn, ParsedEnvelope,
    },
    into_usize::IntoUsize,
    representation::Representation,
    settings::Settings,
    subcommand::{Subcommand, SubcommandResult},
    tally::Tally,
  },
  anyhow::{anyhow, bail, ensure, Context, Error},
  bip39::Mnemonic,
  bitcoin::{
    address::{Address, NetworkUnchecked},
    blockdata::{
      constants::{DIFFCHANGE_INTERVAL, MAX_SCRIPT_ELEMENT_SIZE, SUBSIDY_HALVING_INTERVAL},
      locktime::absolute::LockTime,
    },
    consensus::{self, Decodable, Encodable},
    hash_types::{BlockHash, TxMerkleNode},
    hashes::Hash,
    script, Amount, Block, Network, OutPoint, Script, ScriptBuf, Sequence, Transaction, TxIn,
    TxOut, Txid, Witness,
  },
  bitcoincore_rpc::{Client, RpcApi},
  chrono::{DateTime, TimeZone, Utc},
  ciborium::Value,
  clap::{ArgGroup, Parser},
  html_escaper::{Escape, Trusted},
  http::HeaderMap,
  lazy_static::lazy_static,
  ordinals::{
    varint, Artifact, Charm, Edict, Epoch, Etching, Height, Pile, Rarity, Rune, RuneId, Runestone,
    Sat, SatPoint, SpacedRune, Terms,
  },
  regex::Regex,
  reqwest::Url,
  serde::{Deserialize, Deserializer, Serialize},
  serde_with::{DeserializeFromStr, SerializeDisplay},
  std::{
    cmp::{self, Reverse},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    env,
    fmt::{self, Display, Formatter},
    fs,
    io::{self, Cursor, Read},
    mem,
    net::ToSocketAddrs,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    str::FromStr,
    sync::{
      atomic::{self, AtomicBool},
      Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
  },
  sysinfo::System,
  tokio::{runtime::Runtime, task},
};

pub use self::{
  chain::Chain,
  fee_rate::FeeRate,
  index::{Index, RuneEntry},
  inscriptions::{Envelope, Inscription, InscriptionId},
  object::Object,
  options::Options,
  wallet::transaction_builder::{Target, TransactionBuilder},
};

#[cfg(test)]
#[macro_use]
mod test;

#[cfg(test)]
use self::test::*;

pub mod api;
pub mod arguments;
mod blocktime;
pub mod chain;
pub mod decimal;
mod deserialize_from_str;
mod fee_rate;
pub mod index;
mod inscriptions;
mod into_usize;
mod macros;
mod object;
pub mod options;
pub mod outgoing;
mod re;
mod representation;
pub mod runes;
mod settings;
pub mod subcommand;
mod tally;
pub mod templates;
pub mod wallet;
mod event_publisher;

type Result<T = (), E = Error> = std::result::Result<T, E>;

const TARGET_POSTAGE: Amount = Amount::from_sat(10_000);

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static LISTENERS: Mutex<Vec<axum_server::Handle>> = Mutex::new(Vec::new());
static INDEXER: Mutex<Option<thread::JoinHandle<()>>> = Mutex::new(None);

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fund_raw_transaction(
  client: &Client,
  fee_rate: FeeRate,
  unfunded_transaction: &Transaction,
) -> Result<Vec<u8>> {
  let mut buffer = Vec::new();

  {
    unfunded_transaction.version.consensus_encode(&mut buffer)?;
    unfunded_transaction.input.consensus_encode(&mut buffer)?;
    unfunded_transaction.output.consensus_encode(&mut buffer)?;
    unfunded_transaction
      .lock_time
      .consensus_encode(&mut buffer)?;
  }

  Ok(
    client
      .fund_raw_transaction(
        &buffer,
        Some(&bitcoincore_rpc::json::FundRawTransactionOptions {
          // NB. This is `fundrawtransaction`'s `feeRate`, which is fee per kvB
          // and *not* fee per vB. So, we multiply the fee rate given by the user
          // by 1000.
          fee_rate: Some(Amount::from_sat((fee_rate.n() * 1000.0).ceil() as u64)),
          change_position: Some(unfunded_transaction.output.len().try_into()?),
          ..default()
        }),
        Some(false),
      )
      .map_err(|err| {
        if matches!(
          err,
          bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(
            bitcoincore_rpc::jsonrpc::error::RpcError { code: -6, .. }
          ))
        ) {
          anyhow!("not enough cardinal utxos")
        } else {
          err.into()
        }
      })?
      .hex,
  )
}

pub fn timestamp(seconds: u64) -> DateTime<Utc> {
  Utc
    .timestamp_opt(seconds.try_into().unwrap_or(i64::MAX), 0)
    .unwrap()
}

fn target_as_block_hash(target: bitcoin::Target) -> BlockHash {
  BlockHash::from_raw_hash(Hash::from_byte_array(target.to_le_bytes()))
}

fn unbound_outpoint() -> OutPoint {
  OutPoint {
    txid: Hash::all_zeros(),
    vout: 0,
  }
}

fn uncheck(address: &Address) -> Address<NetworkUnchecked> {
  address.to_string().parse().unwrap()
}

fn default<T: Default>() -> T {
  Default::default()
}

pub fn parse_ord_server_args(args: &str) -> (Settings, subcommand::server::Server) {
  match Arguments::try_parse_from(args.split_whitespace()) {
    Ok(arguments) => match arguments.subcommand {
      Subcommand::Server(server) => (
        Settings::merge(
          arguments.options,
          vec![("INTEGRATION_TEST".into(), "1".into())]
            .into_iter()
            .collect(),
        )
        .unwrap(),
        server,
      ),
      subcommand => panic!("unexpected subcommand: {subcommand:?}"),
    },
    Err(err) => panic!("error parsing arguments: {err}"),
  }
}

fn gracefully_shutdown_indexer() {
  if let Some(indexer) = INDEXER.lock().unwrap().take() {
    // We explicitly set this to true to notify the thread to not take on new work
    SHUTTING_DOWN.store(true, atomic::Ordering::Relaxed);
    log::info!("Waiting for index thread to finish...");
    if indexer.join().is_err() {
      log::warn!("Index thread panicked; join failed");
    }
  }
}

pub fn main() {
  env_logger::init();
  ctrlc::set_handler(move || {
    if SHUTTING_DOWN.fetch_or(true, atomic::Ordering::Relaxed) {
      process::exit(1);
    }

    eprintln!("Shutting down gracefully. Press <CTRL-C> again to shutdown immediately.");

    LISTENERS
      .lock()
      .unwrap()
      .iter()
      .for_each(|handle| handle.graceful_shutdown(Some(Duration::from_millis(100))));

    gracefully_shutdown_indexer();
  })
  .expect("Error setting <CTRL-C> handler");

  let args = Arguments::parse();

  let minify = args.options.minify;

  match args.run() {
    Err(err) => {
      eprintln!("error: {err}");
      err
        .chain()
        .skip(1)
        .for_each(|cause| eprintln!("because: {cause}"));
      if env::var_os("RUST_BACKTRACE")
        .map(|val| val == "1")
        .unwrap_or_default()
      {
        eprintln!("{}", err.backtrace());
      }

      gracefully_shutdown_indexer();

      process::exit(1);
    }
    Ok(output) => {
      if let Some(output) = output {
        output.print_json(minify);
      }
      gracefully_shutdown_indexer();
    }
  }
}
