#![allow(clippy::too_many_arguments)]

use {
  api::Api,
  base64::Engine,
  bitcoin::{
    address::{Address, NetworkUnchecked},
    amount::SignedAmount,
    bip32::{ChildNumber, DerivationPath, Xpriv},
    block::Header,
    blockdata::{script, transaction::Version},
    consensus::encode::{deserialize, serialize},
    consensus::Decodable,
    hash_types::{BlockHash, TxMerkleNode},
    hashes::Hash,
    key::{Keypair, Secp256k1, TapTweak, XOnlyPublicKey},
    locktime::absolute::LockTime,
    opcodes,
    pow::CompactTarget,
    psbt::Psbt,
    script::Instruction,
    secp256k1::{self, rand},
    sighash::{self, SighashCache, TapSighashType},
    Amount, Block, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
    WPubkeyHash, Witness, Wtxid,
  },
  bitcoincore_rpc::json::{
    Bip125Replaceable, CreateRawTransactionInput, EstimateMode, FeeRatePercentiles,
    FinalizePsbtResult, GetBalancesResult, GetBalancesResultEntry, GetBlockHeaderResult,
    GetBlockStatsResult, GetBlockchainInfoResult, GetDescriptorInfoResult, GetNetworkInfoResult,
    GetRawTransactionResult, GetRawTransactionResultVout, GetRawTransactionResultVoutScriptPubKey,
    GetTransactionResult, GetTransactionResultDetail, GetTransactionResultDetailCategory,
    GetTxOutResult, GetWalletInfoResult, ImportDescriptors, ImportMultiResult,
    ListTransactionResult, ListUnspentResultEntry, ListWalletDirItem, ListWalletDirResult,
    LoadWalletResult, SignRawTransactionInput, SignRawTransactionResult, StringOrStringArray,
    Timestamp, WalletProcessPsbtResult, WalletTxInfo,
  },
  jsonrpc_core::{IoHandler, Value},
  jsonrpc_http_server::{CloseHandle, ServerBuilder},
  ord::{SimulateRawTransactionOptions, SimulateRawTransactionResult},
  serde::{Deserialize, Serialize},
  server::Server,
  state::State,
  std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs, mem,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::Duration,
  },
  tempfile::TempDir,
  wallet::Wallet,
};

const COIN_VALUE: u64 = 100_000_000;

mod api;
mod server;
mod state;
mod wallet;

fn parse_hex_tx(tx: String) -> Transaction {
  let mut cursor = bitcoin::io::Cursor::new(hex::decode(tx).unwrap());

  let version = Version(i32::consensus_decode_from_finite_reader(&mut cursor).unwrap());
  let input = Vec::<TxIn>::consensus_decode_from_finite_reader(&mut cursor).unwrap();
  let output = Decodable::consensus_decode_from_finite_reader(&mut cursor).unwrap();
  let lock_time = Decodable::consensus_decode_from_finite_reader(&mut cursor).unwrap();

  Transaction {
    version,
    input,
    output,
    lock_time,
  }
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct Descriptor {
  pub desc: String,
  pub timestamp: Timestamp,
  pub active: bool,
  pub internal: Option<bool>,
  pub range: Option<(u64, u64)>,
  pub next: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct ListDescriptorsResult {
  pub wallet_name: String,
  pub descriptors: Vec<Descriptor>,
}

pub fn builder() -> Builder {
  Builder {
    fail_lock_unspent: false,
    network: Network::Bitcoin,
    version: 250000,
  }
}

pub struct Builder {
  fail_lock_unspent: bool,
  network: Network,
  version: usize,
}

impl Builder {
  pub fn fail_lock_unspent(self, fail_lock_unspent: bool) -> Self {
    Self {
      fail_lock_unspent,
      ..self
    }
  }

  pub fn network(self, network: Network) -> Self {
    Self { network, ..self }
  }

  pub fn version(self, version: usize) -> Self {
    Self { version, ..self }
  }

  pub fn build(self) -> Handle {
    let state = Arc::new(Mutex::new(State::new(
      self.network,
      self.version,
      self.fail_lock_unspent,
    )));
    let server = Server::new(state.clone());
    let mut io = IoHandler::default();
    io.extend_with(server.to_delegate());

    let rpc_server = ServerBuilder::new(io)
      .threads(1)
      .start_http(&"127.0.0.1:0".parse().unwrap())
      .unwrap();

    let close_handle = rpc_server.close_handle();
    let port = rpc_server.address().port();

    thread::spawn(|| rpc_server.wait());

    for i in 0.. {
      match reqwest::blocking::get(format!("http://127.0.0.1:{port}/")) {
        Ok(_) => break,
        Err(err) => {
          if i == 400 {
            panic!("mock bitcoind server failed to start: {err}");
          }
        }
      }

      thread::sleep(Duration::from_millis(25));
    }

    let tempdir = TempDir::new().unwrap();

    fs::write(tempdir.path().join(".cookie"), "username:password").unwrap();

    Handle {
      close_handle: Some(close_handle),
      tempdir,
      port,
      state,
    }
  }
}

pub fn spawn() -> Handle {
  builder().build()
}

#[derive(Clone)]
pub struct TransactionTemplate<'a> {
  pub fee: u64,
  pub inputs: &'a [(usize, usize, usize, Witness)],
  pub op_return: Option<ScriptBuf>,
  pub op_return_index: Option<usize>,
  pub op_return_value: Option<u64>,
  pub output_values: &'a [u64],
  pub outputs: usize,
  pub p2tr: bool,
  pub recipient: Option<Address>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonOutPoint {
  txid: Txid,
  vout: u32,
}

impl From<OutPoint> for JsonOutPoint {
  fn from(outpoint: OutPoint) -> Self {
    Self {
      txid: outpoint.txid,
      vout: outpoint.vout,
    }
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundRawTransactionOptions {
  #[serde(with = "bitcoin::amount::serde::as_btc::opt")]
  fee_rate: Option<Amount>,
  #[serde(skip_serializing_if = "Option::is_none")]
  change_position: Option<u32>,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundRawTransactionResult {
  #[serde(with = "bitcoincore_rpc::json::serde_hex")]
  pub hex: Vec<u8>,
  #[serde(with = "bitcoin::amount::serde::as_btc")]
  pub fee: Amount,
  #[serde(rename = "changepos")]
  pub change_position: i32,
}

impl Default for TransactionTemplate<'_> {
  fn default() -> Self {
    Self {
      fee: 0,
      inputs: &[],
      op_return: None,
      op_return_index: None,
      op_return_value: None,
      output_values: &[],
      outputs: 1,
      p2tr: false,
      recipient: None,
    }
  }
}

pub struct Handle {
  close_handle: Option<CloseHandle>,
  port: u16,
  state: Arc<Mutex<State>>,
  tempdir: TempDir,
}

impl Handle {
  pub fn url(&self) -> String {
    format!("http://127.0.0.1:{}", self.port)
  }

  pub fn address(&self, output: OutPoint) -> Address {
    let state = self.state();

    Address::from_script(
      &state.transactions.get(&output.txid).unwrap().output[output.vout as usize].script_pubkey,
      state.network,
    )
    .unwrap()
  }

  pub fn state(&self) -> MutexGuard<State> {
    self.state.lock().unwrap()
  }

  pub fn clear_state(&self) {
    self.state.lock().unwrap().clear();
  }

  pub fn wallets(&self) -> BTreeSet<String> {
    self.state().wallets.clone()
  }

  #[track_caller]
  pub fn mine_blocks(&self, n: u64) -> Vec<Block> {
    self.mine_blocks_with_subsidy(n, 50 * COIN_VALUE)
  }

  #[track_caller]
  pub fn mine_blocks_with_subsidy(&self, n: u64, subsidy: u64) -> Vec<Block> {
    let mut bitcoin_rpc_data = self.state();
    let mut blocks = Vec::new();
    for _ in 0..n {
      blocks.push(bitcoin_rpc_data.mine_block(subsidy));
    }
    blocks
  }

  pub fn create_tx_from_template(&self, template: TransactionTemplate) -> Transaction {
    self.state().create_tx_from_template(template)
  }

  pub fn broadcast_tx(&self, transaction: Transaction) -> Txid {
    self.state().broadcast_tx(transaction)
  }

  pub fn broadcast_template(&self, template: TransactionTemplate) -> Txid {
    self.state().broadcast_template(template)
  }

  pub fn height(&self) -> u64 {
    u64::try_from(self.state().blocks.len()).unwrap() - 1
  }

  pub fn invalidate_tip(&self) -> BlockHash {
    self.state().pop_block()
  }

  pub fn get_utxo_amount(&self, outpoint: &OutPoint) -> Option<Amount> {
    self.state().utxos.get(outpoint).cloned()
  }

  #[track_caller]
  pub fn tx(&self, block: usize, transaction: usize) -> Transaction {
    let state = self.state();
    let blockhash = state.hashes.get(block).expect("block index out of bounds");
    state.blocks[blockhash]
      .txdata
      .get(transaction)
      .expect("transaction index out of bounds")
      .clone()
  }

  #[track_caller]
  pub fn tx_by_id(&self, txid: Txid) -> Transaction {
    self
      .state()
      .transactions
      .get(&txid)
      .expect("unknown transaction")
      .clone()
  }

  #[track_caller]
  pub fn tx_index(&self, txid: Txid) -> (usize, usize) {
    let state = self.state();

    for (block_hash, block) in &state.blocks {
      for (t, tx) in block.txdata.iter().enumerate() {
        if tx.compute_txid() == txid {
          let b = state
            .hashes
            .iter()
            .enumerate()
            .find(|(_b, hash)| *hash == block_hash)
            .unwrap()
            .0;
          return (b, t);
        }
      }
    }

    panic!("unknown transaction");
  }

  pub fn mempool(&self) -> Vec<Transaction> {
    self.state().mempool().to_vec()
  }

  pub fn descriptors(&self) -> Vec<String> {
    self
      .state()
      .descriptors
      .iter()
      .map(|(descriptor, _timestamp)| descriptor.clone())
      .collect()
  }

  pub fn import_descriptor(&self, desc: String) {
    self.state().descriptors.push((desc, Timestamp::Now));
  }

  pub fn lock(&self, output: OutPoint) {
    self.state().locked.insert(output);
  }

  pub fn network(&self) -> String {
    match self.state().network {
      Network::Bitcoin => "mainnet".to_string(),
      Network::Regtest => Network::Regtest.to_string(),
      Network::Signet => Network::Signet.to_string(),
      Network::Testnet4 => Network::Testnet4.to_string(),
      Network::Testnet => Network::Testnet.to_string(),
      _ => panic!(),
    }
  }

  pub fn loaded_wallets(&self) -> BTreeSet<String> {
    self.state().loaded_wallets.clone()
  }

  pub fn cookie_file(&self) -> PathBuf {
    self.tempdir.path().join(".cookie")
  }

  pub fn get_locked(&self) -> BTreeSet<OutPoint> {
    self.state().get_locked()
  }
}

impl Drop for Handle {
  fn drop(&mut self) {
    self.close_handle.take().unwrap().close();
  }
}
