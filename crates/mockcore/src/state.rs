use super::*;

#[derive(Debug)]
pub struct State {
  pub blocks: BTreeMap<BlockHash, Block>,
  pub change_addresses: BTreeSet<Address>,
  pub descriptors: Vec<(String, bitcoincore_rpc::json::Timestamp)>,
  pub fail_lock_unspent: bool,
  pub hashes: Vec<BlockHash>,
  pub loaded_wallets: BTreeSet<String>,
  pub locked: BTreeSet<OutPoint>,
  pub mempool: Vec<Transaction>,
  pub network: Network,
  pub nonce: u32,
  pub receive_addresses: BTreeSet<Address>,
  pub transactions: BTreeMap<Txid, Transaction>,
  pub txid_to_block_height: BTreeMap<Txid, u32>,
  pub utxos: BTreeMap<OutPoint, Amount>,
  pub version: usize,
  pub wallet: Wallet,
  pub wallets: BTreeSet<String>,
}

impl State {
  pub(crate) fn new(network: Network, version: usize, fail_lock_unspent: bool) -> Self {
    let mut hashes = Vec::new();
    let mut blocks = BTreeMap::new();

    let genesis_block = bitcoin::blockdata::constants::genesis_block(network);
    let genesis_block_hash = genesis_block.block_hash();
    hashes.push(genesis_block_hash);
    blocks.insert(genesis_block_hash, genesis_block);

    Self {
      blocks,
      change_addresses: BTreeSet::new(),
      descriptors: Vec::new(),
      fail_lock_unspent,
      hashes,
      loaded_wallets: BTreeSet::new(),
      locked: BTreeSet::new(),
      mempool: Vec::new(),
      network,
      nonce: 0,
      receive_addresses: BTreeSet::new(),
      transactions: BTreeMap::new(),
      txid_to_block_height: BTreeMap::new(),
      utxos: BTreeMap::new(),
      version,
      wallet: Wallet::new(network),
      wallets: BTreeSet::new(),
    }
  }

  pub fn clear_wallet_addresses(&mut self) -> BTreeSet<Address> {
    mem::take(&mut self.receive_addresses)
      .into_iter()
      .chain(mem::take(&mut self.change_addresses))
      .collect()
  }

  pub fn remove_wallet_address(&mut self, address: Address) {
    assert!(self.receive_addresses.remove(&address) || self.change_addresses.remove(&address));
  }

  pub fn add_wallet_address(&mut self, address: Address) {
    self.receive_addresses.insert(address);
  }

  pub fn new_address(&mut self, change: bool) -> Address {
    let address = self.wallet.new_address();

    if change {
      &mut self.change_addresses
    } else {
      &mut self.receive_addresses
    }
    .insert(address.clone());

    address
  }

  pub fn is_wallet_address(&self, address: &Address) -> bool {
    self.receive_addresses.contains(address) || self.change_addresses.contains(address)
  }

  pub(crate) fn clear(&mut self) {
    *self = Self::new(self.network, self.version, self.fail_lock_unspent);
  }

  #[track_caller]
  pub fn balances(&self) -> BTreeMap<Address, Vec<(OutPoint, Amount)>> {
    let mut addresses: BTreeMap<Address, Vec<(OutPoint, Amount)>> = BTreeMap::new();

    for (&outpoint, &amount) in &self.utxos {
      let transaction = self.transactions.get(&outpoint.txid).unwrap();
      let tx_out = &transaction.output[usize::try_from(outpoint.vout).unwrap()];

      if tx_out.script_pubkey == ScriptBuf::new() {
        continue;
      }

      let address = Address::from_script(&tx_out.script_pubkey, self.network).unwrap();

      addresses
        .entry(address)
        .or_default()
        .push((outpoint, amount));
    }

    addresses
  }

  #[track_caller]
  pub(crate) fn mine_block(&mut self, subsidy: u64) -> Block {
    let coinbase = Transaction {
      version: Version(2),
      lock_time: LockTime::ZERO,
      input: vec![TxIn {
        previous_output: OutPoint::null(),
        script_sig: script::Builder::new()
          .push_int(self.blocks.len().try_into().unwrap())
          .into_script(),
        sequence: Sequence::MAX,
        witness: Witness::new(),
      }],
      output: vec![TxOut {
        value: Amount::from_sat(subsidy)
          + self
            .mempool
            .iter()
            .map(|tx| {
              let fee = tx
                .input
                .iter()
                .map(|txin| {
                  self.transactions[&txin.previous_output.txid].output
                    [txin.previous_output.vout as usize]
                    .value
                })
                .sum::<Amount>()
                - tx.output.iter().map(|txout| txout.value).sum::<Amount>();
              self.transactions.insert(tx.compute_txid(), tx.clone());

              fee
            })
            .sum::<Amount>(),
        script_pubkey: self.new_address(false).into(),
      }],
    };

    self
      .transactions
      .insert(coinbase.compute_txid(), coinbase.clone());

    let block = Block {
      header: Header {
        version: bitcoin::block::Version::ONE,
        prev_blockhash: *self.hashes.last().unwrap(),
        merkle_root: TxMerkleNode::all_zeros(),
        time: self.blocks.len().try_into().unwrap(),
        bits: CompactTarget::from_consensus(0),
        nonce: self.nonce,
      },
      txdata: std::iter::once(coinbase)
        .chain(self.mempool.drain(0..))
        .collect(),
    };

    for tx in block.txdata.iter() {
      self
        .txid_to_block_height
        .insert(tx.compute_txid(), self.hashes.len().try_into().unwrap());

      for input in tx.input.iter() {
        if !input.previous_output.is_null() {
          assert!(self.utxos.remove(&input.previous_output).is_some());
        }
      }

      for (vout, txout) in tx.output.iter().enumerate() {
        if !txout.script_pubkey.is_op_return() {
          self.utxos.insert(
            OutPoint {
              txid: tx.compute_txid(),
              vout: vout.try_into().unwrap(),
            },
            txout.value,
          );
        }
      }
    }

    self.blocks.insert(block.block_hash(), block.clone());
    self.hashes.push(block.block_hash());
    self.nonce += 1;

    block
  }

  pub(crate) fn pop_block(&mut self) -> BlockHash {
    let blockhash = self.hashes.pop().unwrap();
    self.blocks.remove(&blockhash);

    blockhash
  }

  pub(crate) fn create_tx_from_template(&mut self, template: TransactionTemplate) -> Transaction {
    let mut total_value = 0;
    let mut input = Vec::new();
    for (height, tx, vout, witness) in template.inputs.iter() {
      let block_hash = self
        .hashes
        .get(*height)
        .ok_or_else(|| format!("invalid block height {height}"))
        .unwrap();

      let block = self.blocks.get(block_hash).unwrap();

      let tx = block
        .txdata
        .get(*tx)
        .ok_or_else(|| format!("invalid transaction index {tx}"))
        .unwrap();

      let tx_out = tx
        .output
        .get(*vout)
        .ok_or_else(|| format!("invalid output index {vout}"))
        .unwrap();

      total_value += tx_out.value.to_sat();

      input.push(TxIn {
        previous_output: OutPoint::new(tx.compute_txid(), *vout as u32),
        script_sig: ScriptBuf::new(),
        sequence: Sequence::MAX,
        witness: witness.clone(),
      });
    }

    let value_per_output = if template.outputs > 0 {
      (total_value - template.fee) / template.outputs as u64
    } else {
      0
    };

    if template.outputs > 0 {
      assert_eq!(
        value_per_output * template.outputs as u64 + template.fee,
        total_value
      );
    }

    let mut tx = Transaction {
      version: Version(2),
      lock_time: LockTime::ZERO,
      input,
      output: (0..template.outputs)
        .map(|i| TxOut {
          value: Amount::from_sat(
            template
              .output_values
              .get(i)
              .cloned()
              .unwrap_or(value_per_output),
          ),
          script_pubkey: if let Some(recipient) = &template.recipient {
            recipient.script_pubkey()
          } else if template.p2tr {
            let secp = Secp256k1::new();
            let keypair = Keypair::new(&secp, &mut rand::thread_rng());
            let internal_key = XOnlyPublicKey::from_keypair(&keypair);
            ScriptBuf::new_p2tr(&secp, internal_key.0, None)
          } else {
            ScriptBuf::new_p2wpkh(&WPubkeyHash::all_zeros())
          },
        })
        .collect(),
    };

    if let Some(script_pubkey) = template.op_return {
      tx.output.insert(
        template.op_return_index.unwrap_or(tx.output.len()),
        TxOut {
          value: Amount::from_sat(template.op_return_value.unwrap_or_default()),
          script_pubkey,
        },
      );
    }

    tx
  }

  pub(crate) fn broadcast_tx(&mut self, transaction: Transaction) -> Txid {
    let txid = transaction.compute_txid();

    self.mempool.push(transaction);

    txid
  }

  pub(crate) fn broadcast_template(&mut self, template: TransactionTemplate) -> Txid {
    let tx = self.create_tx_from_template(template);
    let txid = tx.compute_txid();

    self.mempool.push(tx);

    txid
  }

  pub(crate) fn mempool(&self) -> &[Transaction] {
    &self.mempool
  }

  pub(crate) fn get_confirmations(&self, tx: &Transaction) -> i32 {
    for (confirmations, hash) in self.hashes.iter().rev().enumerate() {
      if self.blocks.get(hash).unwrap().txdata.contains(tx) {
        return (confirmations + 1).try_into().unwrap();
      }
    }

    0
  }

  pub(crate) fn get_locked(&self) -> BTreeSet<OutPoint> {
    self.locked.clone()
  }
}
