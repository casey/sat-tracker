use {
  self::{
    entry::{
      Entry, HeaderValue, InscriptionEntry, InscriptionEntryValue, InscriptionIdValue,
      OutPointValue, RuneEntryValue, RuneIdValue, SatPointValue, SatRange, TxidValue,
    },
    event::Event,
    lot::Lot,
    reorg::Reorg,
    updater::Updater,
    utxo_entry::{ParsedUtxoEntry, UtxoEntry, UtxoEntryBuf},
  },
  super::*,
  crate::{
    runes::MintError,
    subcommand::{find::FindRangeOutput, server::query},
    templates::StatusHtml,
  },
  bitcoin::block::Header,
  bitcoincore_rpc::{
    json::{GetBlockHeaderResult, GetBlockStatsResult},
    Client,
  },
  chrono::SubsecRound,
  indicatif::{ProgressBar, ProgressStyle},
  log::log_enabled,
  redb::{
    Database, DatabaseError, MultimapTable, MultimapTableDefinition, MultimapTableHandle,
    ReadOnlyTable, ReadableMultimapTable, ReadableTable, ReadableTableMetadata, RepairSession,
    StorageError, Table, TableDefinition, TableHandle, TableStats, WriteTransaction,
  },
  std::{
    collections::HashMap,
    io::{BufWriter, Write},
    sync::Once,
  },
};

pub use self::entry::RuneEntry;

pub(crate) mod entry;
pub mod event;
mod fetcher;
mod lot;
mod reorg;
mod rtx;
mod updater;
mod utxo_entry;

#[cfg(test)]
pub(crate) mod testing;

const SCHEMA_VERSION: u64 = 30;

define_multimap_table! { SAT_TO_SEQUENCE_NUMBER, u64, u32 }
define_multimap_table! { SEQUENCE_NUMBER_TO_CHILDREN, u32, u32 }
define_multimap_table! { SCRIPT_PUBKEY_TO_OUTPOINT, &[u8], OutPointValue }
define_table! { HEIGHT_TO_BLOCK_HEADER, u32, &HeaderValue }
define_table! { HEIGHT_TO_LAST_SEQUENCE_NUMBER, u32, u32 }
define_table! { HOME_INSCRIPTIONS, u32, InscriptionIdValue }
define_table! { INSCRIPTION_ID_TO_SEQUENCE_NUMBER, InscriptionIdValue, u32 }
define_table! { INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER, i32, u32 }
define_table! { OUTPOINT_TO_RUNE_BALANCES, &OutPointValue, &[u8] }
define_table! { OUTPOINT_TO_UTXO_ENTRY, &OutPointValue, &UtxoEntry }
define_table! { RUNE_ID_TO_RUNE_ENTRY, RuneIdValue, RuneEntryValue }
define_table! { RUNE_TO_RUNE_ID, u128, RuneIdValue }
define_table! { SAT_TO_SATPOINT, u64, &SatPointValue }
define_table! { SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY, u32, InscriptionEntryValue }
define_table! { SEQUENCE_NUMBER_TO_RUNE_ID, u32, RuneIdValue }
define_table! { SEQUENCE_NUMBER_TO_SATPOINT, u32, &SatPointValue }
define_table! { STATISTIC_TO_COUNT, u64, u64 }
define_table! { TRANSACTION_ID_TO_RUNE, &TxidValue, u128 }
define_table! { TRANSACTION_ID_TO_TRANSACTION, &TxidValue, &[u8] }
define_table! { WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP, u32, u128 }

#[derive(Copy, Clone)]
pub(crate) enum Statistic {
  Schema = 0,
  BlessedInscriptions = 1,
  Commits = 2,
  CursedInscriptions = 3,
  IndexAddresses = 4,
  IndexInscriptions = 5,
  IndexRunes = 6,
  IndexSats = 7,
  IndexTransactions = 8,
  InitialSyncTime = 9,
  LostSats = 10,
  OutputsTraversed = 11,
  ReservedRunes = 12,
  Runes = 13,
  SatRanges = 14,
  UnboundInscriptions = 16,
  LastSavepointHeight = 17,
}

impl Statistic {
  fn key(self) -> u64 {
    self.into()
  }
}

impl From<Statistic> for u64 {
  fn from(statistic: Statistic) -> Self {
    statistic as u64
  }
}

#[derive(Serialize)]
pub struct Info {
  blocks_indexed: u32,
  branch_pages: u64,
  fragmented_bytes: u64,
  index_file_size: u64,
  index_path: PathBuf,
  leaf_pages: u64,
  metadata_bytes: u64,
  outputs_traversed: u64,
  page_size: usize,
  sat_ranges: u64,
  stored_bytes: u64,
  tables: BTreeMap<String, TableInfo>,
  total_bytes: u64,
  pub transactions: Vec<TransactionInfo>,
  tree_height: u32,
  utxos_indexed: u64,
}

#[derive(Serialize)]
pub(crate) struct TableInfo {
  branch_pages: u64,
  fragmented_bytes: u64,
  leaf_pages: u64,
  metadata_bytes: u64,
  proportion: f64,
  stored_bytes: u64,
  total_bytes: u64,
  tree_height: u32,
}

impl From<TableStats> for TableInfo {
  fn from(stats: TableStats) -> Self {
    Self {
      branch_pages: stats.branch_pages(),
      fragmented_bytes: stats.fragmented_bytes(),
      leaf_pages: stats.leaf_pages(),
      metadata_bytes: stats.metadata_bytes(),
      proportion: 0.0,
      stored_bytes: stats.stored_bytes(),
      total_bytes: stats.stored_bytes() + stats.metadata_bytes() + stats.fragmented_bytes(),
      tree_height: stats.tree_height(),
    }
  }
}

#[derive(Serialize)]
pub struct TransactionInfo {
  pub starting_block_count: u32,
  pub starting_timestamp: u128,
}

pub(crate) trait BitcoinCoreRpcResultExt<T> {
  fn into_option(self) -> Result<Option<T>>;
}

impl<T> BitcoinCoreRpcResultExt<T> for Result<T, bitcoincore_rpc::Error> {
  fn into_option(self) -> Result<Option<T>> {
    match self {
      Ok(ok) => Ok(Some(ok)),
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { code: -8, .. },
      ))) => Ok(None),
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError {
          code: -5, message, ..
        },
      )))
        if message.starts_with("No such mempool or blockchain transaction") =>
      {
        Ok(None)
      }
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { message, .. },
      )))
        if message.ends_with("not found") =>
      {
        Ok(None)
      }
      Err(err) => Err(err.into()),
    }
  }
}

pub struct Index {
  pub(crate) client: Client,
  database: Database,
  durability: redb::Durability,
  event_sender: Option<tokio::sync::mpsc::Sender<Event>>,
  genesis_block_coinbase_transaction: Transaction,
  genesis_block_coinbase_txid: Txid,
  height_limit: Option<u32>,
  index_addresses: bool,
  index_inscriptions: bool,
  index_runes: bool,
  index_sats: bool,
  index_transactions: bool,
  path: PathBuf,
  settings: Settings,
  started: DateTime<Utc>,
  first_index_height: u32,
  unrecoverably_reorged: AtomicBool,
}

impl Index {
  pub fn open(settings: &Settings) -> Result<Self> {
    Index::open_with_event_sender(settings, None)
  }

  pub fn open_with_event_sender(
    settings: &Settings,
    event_sender: Option<tokio::sync::mpsc::Sender<Event>>,
  ) -> Result<Self> {
    let client = settings.bitcoin_rpc_client(None)?;

    let path = settings.index().to_owned();

    let data_dir = path.parent().unwrap();

    fs::create_dir_all(data_dir).snafu_context(error::Io { path: data_dir })?;

    let index_cache_size = settings.index_cache_size();

    log::info!("Setting index cache size to {} bytes", index_cache_size);

    let durability = if cfg!(test) {
      redb::Durability::None
    } else {
      redb::Durability::Immediate
    };

    let index_path = path.clone();
    let once = Once::new();
    let progress_bar = Mutex::new(None);
    let integration_test = settings.integration_test();

    let repair_callback = move |progress: &mut RepairSession| {
      once.call_once(|| println!("Index file `{}` needs recovery. This can take a long time, especially for the --index-sats index.", index_path.display()));

      if !(cfg!(test) || log_enabled!(log::Level::Info) || integration_test) {
        let mut guard = progress_bar.lock().unwrap();

        let progress_bar = guard.get_or_insert_with(|| {
          let progress_bar = ProgressBar::new(100);
          progress_bar.set_style(
            ProgressStyle::with_template("[repairing database] {wide_bar} {pos}/{len}").unwrap(),
          );
          progress_bar
        });

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        progress_bar.set_position((progress.progress() * 100.0) as u64);
      }
    };

    let database = match Database::builder()
      .set_cache_size(index_cache_size)
      .set_repair_callback(repair_callback)
      .open(&path)
    {
      Ok(database) => {
        {
          let schema_version = database
            .begin_read()?
            .open_table(STATISTIC_TO_COUNT)?
            .get(&Statistic::Schema.key())?
            .map(|x| x.value())
            .unwrap_or(0);

          match schema_version.cmp(&SCHEMA_VERSION) {
            cmp::Ordering::Less =>
              bail!(
                "index at `{}` appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
                path.display()
              ),
            cmp::Ordering::Greater =>
              bail!(
                "index at `{}` appears to have been built with a newer, incompatible version of ord, consider updating ord: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
                path.display()
              ),
            cmp::Ordering::Equal => {
            }
          }
        }

        database
      }
      Err(DatabaseError::Storage(StorageError::Io(error)))
        if error.kind() == io::ErrorKind::NotFound =>
      {
        let database = Database::builder()
          .set_cache_size(index_cache_size)
          .create(&path)?;

        let mut tx = database.begin_write()?;

        tx.set_durability(durability);
        tx.set_quick_repair(true);

        tx.open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?;
        tx.open_multimap_table(SCRIPT_PUBKEY_TO_OUTPOINT)?;
        tx.open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?;
        tx.open_table(HEIGHT_TO_BLOCK_HEADER)?;
        tx.open_table(HEIGHT_TO_LAST_SEQUENCE_NUMBER)?;
        tx.open_table(HOME_INSCRIPTIONS)?;
        tx.open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?;
        tx.open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?;
        tx.open_table(OUTPOINT_TO_RUNE_BALANCES)?;
        tx.open_table(OUTPOINT_TO_UTXO_ENTRY)?;
        tx.open_table(RUNE_ID_TO_RUNE_ENTRY)?;
        tx.open_table(RUNE_TO_RUNE_ID)?;
        tx.open_table(SAT_TO_SATPOINT)?;
        tx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;
        tx.open_table(SEQUENCE_NUMBER_TO_RUNE_ID)?;
        tx.open_table(SEQUENCE_NUMBER_TO_SATPOINT)?;
        tx.open_table(TRANSACTION_ID_TO_RUNE)?;
        tx.open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?;

        {
          let mut statistics = tx.open_table(STATISTIC_TO_COUNT)?;

          Self::set_statistic(
            &mut statistics,
            Statistic::IndexAddresses,
            u64::from(settings.index_addresses_raw()),
          )?;

          Self::set_statistic(
            &mut statistics,
            Statistic::IndexInscriptions,
            u64::from(settings.index_inscriptions_raw()),
          )?;

          Self::set_statistic(
            &mut statistics,
            Statistic::IndexRunes,
            u64::from(settings.index_runes_raw()),
          )?;

          Self::set_statistic(
            &mut statistics,
            Statistic::IndexSats,
            u64::from(settings.index_sats_raw()),
          )?;

          Self::set_statistic(
            &mut statistics,
            Statistic::IndexTransactions,
            u64::from(settings.index_transactions_raw()),
          )?;

          Self::set_statistic(&mut statistics, Statistic::Schema, SCHEMA_VERSION)?;
        }

        if settings.index_runes_raw() && settings.chain() == Chain::Mainnet {
          let rune = Rune(2055900680524219742);

          let id = RuneId { block: 1, tx: 0 };
          let etching = Txid::all_zeros();

          tx.open_table(RUNE_TO_RUNE_ID)?
            .insert(rune.store(), id.store())?;

          let mut statistics = tx.open_table(STATISTIC_TO_COUNT)?;

          Self::set_statistic(&mut statistics, Statistic::Runes, 1)?;

          tx.open_table(RUNE_ID_TO_RUNE_ENTRY)?.insert(
            id.store(),
            RuneEntry {
              block: id.block,
              burned: 0,
              divisibility: 0,
              etching,
              terms: Some(Terms {
                amount: Some(1),
                cap: Some(u128::MAX),
                height: (
                  Some((SUBSIDY_HALVING_INTERVAL * 4).into()),
                  Some((SUBSIDY_HALVING_INTERVAL * 5).into()),
                ),
                offset: (None, None),
              }),
              mints: 0,
              number: 0,
              premine: 0,
              spaced_rune: SpacedRune { rune, spacers: 128 },
              symbol: Some('\u{29C9}'),
              timestamp: 0,
              turbo: true,
            }
            .store(),
          )?;

          tx.open_table(TRANSACTION_ID_TO_RUNE)?
            .insert(&etching.store(), rune.store())?;
        }

        tx.commit()?;

        database
      }
      Err(error) => bail!("failed to open index: {error}"),
    };

    let index_addresses;
    let index_runes;
    let index_sats;
    let index_transactions;
    let index_inscriptions;

    {
      let tx = database.begin_read()?;
      let statistics = tx.open_table(STATISTIC_TO_COUNT)?;
      index_addresses = Self::is_statistic_set(&statistics, Statistic::IndexAddresses)?;
      index_inscriptions = Self::is_statistic_set(&statistics, Statistic::IndexInscriptions)?;
      index_runes = Self::is_statistic_set(&statistics, Statistic::IndexRunes)?;
      index_sats = Self::is_statistic_set(&statistics, Statistic::IndexSats)?;
      index_transactions = Self::is_statistic_set(&statistics, Statistic::IndexTransactions)?;
    }

    let genesis_block_coinbase_transaction =
      settings.chain().genesis_block().coinbase().unwrap().clone();

    let first_index_height = if index_sats || index_addresses {
      0
    } else if index_inscriptions {
      settings.first_inscription_height()
    } else if index_runes {
      settings.first_rune_height()
    } else {
      u32::MAX
    };

    Ok(Self {
      genesis_block_coinbase_txid: genesis_block_coinbase_transaction.compute_txid(),
      client,
      database,
      durability,
      event_sender,
      first_index_height,
      genesis_block_coinbase_transaction,
      height_limit: settings.height_limit(),
      index_addresses,
      index_runes,
      index_sats,
      index_transactions,
      index_inscriptions,
      settings: settings.clone(),
      path,
      started: Utc::now(),
      unrecoverably_reorged: AtomicBool::new(false),
    })
  }

  pub fn have_full_utxo_index(&self) -> bool {
    self.first_index_height == 0
  }

  /// Unlike normal outpoints, which are added to index on creation and removed
  /// when spent, the UTXO entry for special outpoints may be updated.
  ///
  /// The special outpoints are the null outpoint, which receives lost sats,
  /// and the unbound outpoint, which receives unbound inscriptions.
  pub fn is_special_outpoint(outpoint: OutPoint) -> bool {
    outpoint == OutPoint::null() || outpoint == unbound_outpoint()
  }

  #[cfg(test)]
  fn set_durability(&mut self, durability: redb::Durability) {
    self.durability = durability;
  }

  pub fn contains_output(&self, output: &OutPoint) -> Result<bool> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(OUTPOINT_TO_UTXO_ENTRY)?
        .get(&output.store())?
        .is_some(),
    )
  }

  pub fn has_address_index(&self) -> bool {
    self.index_addresses
  }

  pub fn has_inscription_index(&self) -> bool {
    self.index_inscriptions
  }

  pub fn has_rune_index(&self) -> bool {
    self.index_runes
  }

  pub fn has_sat_index(&self) -> bool {
    self.index_sats
  }

  pub fn status(&self, json_api: bool) -> Result<StatusHtml> {
    let rtx = self.database.begin_read()?;

    let statistic_to_count = rtx.open_table(STATISTIC_TO_COUNT)?;

    let statistic = |statistic: Statistic| -> Result<u64> {
      Ok(
        statistic_to_count
          .get(statistic.key())?
          .map(|guard| guard.value())
          .unwrap_or_default(),
      )
    };

    let height = rtx
      .open_table(HEIGHT_TO_BLOCK_HEADER)?
      .range(0..)?
      .next_back()
      .transpose()?
      .map(|(height, _header)| height.value());

    let next_height = height.map(|height| height + 1).unwrap_or(0);

    let blessed_inscriptions = statistic(Statistic::BlessedInscriptions)?;
    let cursed_inscriptions = statistic(Statistic::CursedInscriptions)?;
    let initial_sync_time = statistic(Statistic::InitialSyncTime)?;

    Ok(StatusHtml {
      address_index: self.has_address_index(),
      blessed_inscriptions,
      chain: self.settings.chain(),
      cursed_inscriptions,
      height,
      initial_sync_time: Duration::from_micros(initial_sync_time),
      inscription_index: self.has_inscription_index(),
      inscriptions: blessed_inscriptions + cursed_inscriptions,
      json_api,
      lost_sats: statistic(Statistic::LostSats)?,
      minimum_rune_for_next_block: Rune::minimum_at_height(
        self.settings.chain().network(),
        Height(next_height),
      ),
      rune_index: self.has_rune_index(),
      runes: statistic(Statistic::Runes)?,
      sat_index: self.has_sat_index(),
      started: self.started,
      transaction_index: statistic(Statistic::IndexTransactions)? != 0,
      unrecoverably_reorged: self.unrecoverably_reorged.load(atomic::Ordering::Relaxed),
      uptime: (Utc::now() - self.started).to_std()?,
    })
  }

  pub fn info(&self) -> Result<Info> {
    let stats = self.database.begin_write()?.stats()?;

    let rtx = self.database.begin_read()?;

    let mut tables: BTreeMap<String, TableInfo> = BTreeMap::new();

    for handle in rtx.list_tables()? {
      let name = handle.name().into();
      let stats = rtx.open_untyped_table(handle)?.stats()?;
      tables.insert(name, stats.into());
    }

    for handle in rtx.list_multimap_tables()? {
      let name = handle.name().into();
      let stats = rtx.open_untyped_multimap_table(handle)?.stats()?;
      tables.insert(name, stats.into());
    }

    for table in rtx.list_tables()? {
      assert!(tables.contains_key(table.name()));
    }

    for table in rtx.list_multimap_tables()? {
      assert!(tables.contains_key(table.name()));
    }

    let total_bytes = tables
      .values()
      .map(|table_info| table_info.total_bytes)
      .sum();

    tables.values_mut().for_each(|table_info| {
      table_info.proportion = table_info.total_bytes as f64 / total_bytes as f64
    });

    let info = {
      let statistic_to_count = rtx.open_table(STATISTIC_TO_COUNT)?;
      let sat_ranges = statistic_to_count
        .get(&Statistic::SatRanges.key())?
        .map(|x| x.value())
        .unwrap_or(0);
      let outputs_traversed = statistic_to_count
        .get(&Statistic::OutputsTraversed.key())?
        .map(|x| x.value())
        .unwrap_or(0);
      Info {
        index_path: self.path.clone(),
        blocks_indexed: rtx
          .open_table(HEIGHT_TO_BLOCK_HEADER)?
          .range(0..)?
          .next_back()
          .transpose()?
          .map(|(height, _header)| height.value() + 1)
          .unwrap_or(0),
        branch_pages: stats.branch_pages(),
        fragmented_bytes: stats.fragmented_bytes(),
        index_file_size: fs::metadata(&self.path)?.len(),
        leaf_pages: stats.leaf_pages(),
        metadata_bytes: stats.metadata_bytes(),
        sat_ranges,
        outputs_traversed,
        page_size: stats.page_size(),
        stored_bytes: stats.stored_bytes(),
        total_bytes,
        tables,
        transactions: rtx
          .open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?
          .range(0..)?
          .flat_map(|result| {
            result.map(
              |(starting_block_count, starting_timestamp)| TransactionInfo {
                starting_block_count: starting_block_count.value(),
                starting_timestamp: starting_timestamp.value(),
              },
            )
          })
          .collect(),
        tree_height: stats.tree_height(),
        utxos_indexed: rtx.open_table(OUTPOINT_TO_UTXO_ENTRY)?.len()?,
      }
    };

    Ok(info)
  }

  pub fn update(&self) -> Result {
    loop {
      let wtx = self.begin_write()?;

      let mut updater = Updater {
        height: wtx
          .open_table(HEIGHT_TO_BLOCK_HEADER)?
          .range(0..)?
          .next_back()
          .transpose()?
          .map(|(height, _header)| height.value() + 1)
          .unwrap_or(0),
        index: self,
        outputs_cached: 0,
        outputs_traversed: 0,
        sat_ranges_since_flush: 0,
      };

      match updater.update_index(wtx) {
        Ok(ok) => return Ok(ok),
        Err(err) => {
          log::info!("{}", err.to_string());

          match err.downcast_ref() {
            Some(&reorg::Error::Recoverable { height, depth }) => {
              Reorg::handle_reorg(self, height, depth)?;
            }
            Some(&reorg::Error::Unrecoverable) => {
              self
                .unrecoverably_reorged
                .store(true, atomic::Ordering::Relaxed);
              return Err(anyhow!(reorg::Error::Unrecoverable));
            }
            _ => return Err(err),
          };
        }
      }
    }
  }

  pub fn export(&self, filename: &String, include_addresses: bool) -> Result {
    let mut writer = BufWriter::new(File::create(filename)?);
    let rtx = self.database.begin_read()?;

    let blocks_indexed = rtx
      .open_table(HEIGHT_TO_BLOCK_HEADER)?
      .range(0..)?
      .next_back()
      .transpose()?
      .map(|(height, _header)| height.value() + 1)
      .unwrap_or(0);

    writeln!(writer, "# export at block height {}", blocks_indexed)?;

    log::info!("exporting database tables to {filename}");

    let sequence_number_to_satpoint = rtx.open_table(SEQUENCE_NUMBER_TO_SATPOINT)?;
    let outpoint_to_utxo_entry = rtx.open_table(OUTPOINT_TO_UTXO_ENTRY)?;

    for result in rtx
      .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
      .iter()?
    {
      let entry = result?;
      let sequence_number = entry.0.value();
      let entry = InscriptionEntry::load(entry.1.value());
      let satpoint = SatPoint::load(
        *sequence_number_to_satpoint
          .get(sequence_number)?
          .unwrap()
          .value(),
      );

      write!(
        writer,
        "{}\t{}\t{}",
        entry.inscription_number, entry.id, satpoint
      )?;

      if include_addresses {
        let address = if satpoint.outpoint == unbound_outpoint() {
          "unbound".to_string()
        } else {
          let script_pubkey = if self.index_addresses {
            ScriptBuf::from_bytes(
              outpoint_to_utxo_entry
                .get(&satpoint.outpoint.store())?
                .unwrap()
                .value()
                .parse(self)
                .script_pubkey()
                .to_vec(),
            )
          } else {
            self
              .get_transaction(satpoint.outpoint.txid)?
              .unwrap()
              .output
              .into_iter()
              .nth(satpoint.outpoint.vout.try_into().unwrap())
              .unwrap()
              .script_pubkey
          };

          self
            .settings
            .chain()
            .address_from_script(&script_pubkey)
            .map(|address| address.to_string())
            .unwrap_or_else(|e| e.to_string())
        };
        write!(writer, "\t{}", address)?;
      }
      writeln!(writer)?;

      if SHUTTING_DOWN.load(atomic::Ordering::Relaxed) {
        break;
      }
    }
    writer.flush()?;
    Ok(())
  }

  fn begin_read(&self) -> Result<rtx::Rtx> {
    Ok(rtx::Rtx(self.database.begin_read()?))
  }

  fn begin_write(&self) -> Result<WriteTransaction> {
    let mut tx = self.database.begin_write()?;
    tx.set_durability(self.durability);
    tx.set_quick_repair(true);
    Ok(tx)
  }

  fn increment_statistic(wtx: &WriteTransaction, statistic: Statistic, n: u64) -> Result {
    let mut statistic_to_count = wtx.open_table(STATISTIC_TO_COUNT)?;
    let value = statistic_to_count
      .get(&(statistic.key()))?
      .map(|x| x.value())
      .unwrap_or_default()
      + n;
    statistic_to_count.insert(&statistic.key(), &value)?;
    Ok(())
  }

  pub(crate) fn set_statistic(
    statistics: &mut Table<u64, u64>,
    statistic: Statistic,
    value: u64,
  ) -> Result<()> {
    statistics.insert(&statistic.key(), &value)?;
    Ok(())
  }

  pub(crate) fn is_statistic_set(
    statistics: &ReadOnlyTable<u64, u64>,
    statistic: Statistic,
  ) -> Result<bool> {
    Ok(
      statistics
        .get(&statistic.key())?
        .map(|guard| guard.value())
        .unwrap_or_default()
        != 0,
    )
  }

  #[cfg(test)]
  pub(crate) fn statistic(&self, statistic: Statistic) -> u64 {
    self
      .database
      .begin_read()
      .unwrap()
      .open_table(STATISTIC_TO_COUNT)
      .unwrap()
      .get(&statistic.key())
      .unwrap()
      .map(|x| x.value())
      .unwrap_or_default()
  }

  #[cfg(test)]
  pub(crate) fn inscription_number(&self, inscription_id: InscriptionId) -> i32 {
    self
      .get_inscription_entry(inscription_id)
      .unwrap()
      .unwrap()
      .inscription_number
  }

  pub fn block_count(&self) -> Result<u32> {
    self.begin_read()?.block_count()
  }

  pub fn block_height(&self) -> Result<Option<Height>> {
    self.begin_read()?.block_height()
  }

  pub fn block_hash(&self, height: Option<u32>) -> Result<Option<BlockHash>> {
    self.begin_read()?.block_hash(height)
  }

  pub fn blocks(&self, take: usize) -> Result<Vec<(u32, BlockHash)>> {
    let rtx = self.begin_read()?;

    let block_count = rtx.block_count()?;

    let height_to_block_header = rtx.0.open_table(HEIGHT_TO_BLOCK_HEADER)?;

    let mut blocks = Vec::with_capacity(block_count.try_into().unwrap());

    for next in height_to_block_header
      .range(0..block_count)?
      .rev()
      .take(take)
    {
      let next = next?;
      blocks.push((next.0.value(), Header::load(*next.1.value()).block_hash()));
    }

    Ok(blocks)
  }

  pub fn rare_sat_satpoints(&self) -> Result<Vec<(Sat, SatPoint)>> {
    let rtx = self.database.begin_read()?;

    let sat_to_satpoint = rtx.open_table(SAT_TO_SATPOINT)?;

    let mut result = Vec::with_capacity(sat_to_satpoint.len()?.try_into().unwrap());

    for range in sat_to_satpoint.range(0..)? {
      let (sat, satpoint) = range?;
      result.push((Sat(sat.value()), Entry::load(*satpoint.value())));
    }

    Ok(result)
  }

  pub fn rare_sat_satpoint(&self, sat: Sat) -> Result<Option<SatPoint>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(SAT_TO_SATPOINT)?
        .get(&sat.n())?
        .map(|satpoint| Entry::load(*satpoint.value())),
    )
  }

  pub fn get_rune_by_id(&self, id: RuneId) -> Result<Option<Rune>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(RUNE_ID_TO_RUNE_ENTRY)?
        .get(&id.store())?
        .map(|entry| RuneEntry::load(entry.value()).spaced_rune.rune),
    )
  }

  pub fn get_rune_by_number(&self, number: usize) -> Result<Option<Rune>> {
    match self
      .database
      .begin_read()?
      .open_table(RUNE_ID_TO_RUNE_ENTRY)?
      .iter()?
      .nth(number)
    {
      Some(result) => {
        let rune_result =
          result.map(|(_id, entry)| RuneEntry::load(entry.value()).spaced_rune.rune);
        Ok(rune_result.ok())
      }
      None => Ok(None),
    }
  }

  pub fn rune(&self, rune: Rune) -> Result<Option<(RuneId, RuneEntry, Option<InscriptionId>)>> {
    let rtx = self.database.begin_read()?;

    let Some(id) = rtx
      .open_table(RUNE_TO_RUNE_ID)?
      .get(rune.0)?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let entry = RuneEntry::load(
      rtx
        .open_table(RUNE_ID_TO_RUNE_ENTRY)?
        .get(id)?
        .unwrap()
        .value(),
    );

    let parent = InscriptionId {
      txid: entry.etching,
      index: 0,
    };

    let parent = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
      .get(&parent.store())?
      .is_some()
      .then_some(parent);

    Ok(Some((RuneId::load(id), entry, parent)))
  }

  pub fn runes(&self) -> Result<Vec<(RuneId, RuneEntry)>> {
    let mut entries = Vec::new();

    for result in self
      .database
      .begin_read()?
      .open_table(RUNE_ID_TO_RUNE_ENTRY)?
      .iter()?
    {
      let (id, entry) = result?;
      entries.push((RuneId::load(id.value()), RuneEntry::load(entry.value())));
    }

    Ok(entries)
  }

  pub fn runes_paginated(
    &self,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<(RuneId, RuneEntry)>, bool)> {
    let mut entries = Vec::new();

    for result in self
      .database
      .begin_read()?
      .open_table(RUNE_ID_TO_RUNE_ENTRY)?
      .iter()?
      .rev()
      .skip(page_index.saturating_mul(page_size))
      .take(page_size.saturating_add(1))
    {
      let (id, entry) = result?;
      entries.push((RuneId::load(id.value()), RuneEntry::load(entry.value())));
    }

    let more = entries.len() > page_size;

    Ok((entries, more))
  }

  pub fn encode_rune_balance(id: RuneId, balance: u128, buffer: &mut Vec<u8>) {
    varint::encode_to_vec(id.block.into(), buffer);
    varint::encode_to_vec(id.tx.into(), buffer);
    varint::encode_to_vec(balance, buffer);
  }

  pub fn decode_rune_balance(buffer: &[u8]) -> Result<((RuneId, u128), usize)> {
    let mut len = 0;
    let (block, block_len) = varint::decode(&buffer[len..])?;
    len += block_len;
    let (tx, tx_len) = varint::decode(&buffer[len..])?;
    len += tx_len;
    let id = RuneId {
      block: block.try_into()?,
      tx: tx.try_into()?,
    };
    let (balance, balance_len) = varint::decode(&buffer[len..])?;
    len += balance_len;
    Ok(((id, balance), len))
  }

  pub fn get_rune_balances_for_output(
    &self,
    outpoint: OutPoint,
  ) -> Result<Option<BTreeMap<SpacedRune, Pile>>> {
    if !self.index_runes {
      return Ok(None);
    }

    let rtx = self.database.begin_read()?;

    let outpoint_to_balances = rtx.open_table(OUTPOINT_TO_RUNE_BALANCES)?;

    let id_to_rune_entries = rtx.open_table(RUNE_ID_TO_RUNE_ENTRY)?;

    let Some(balances) = outpoint_to_balances.get(&outpoint.store())? else {
      return Ok(Some(BTreeMap::new()));
    };

    let balances_buffer = balances.value();

    let mut balances = BTreeMap::new();
    let mut i = 0;
    while i < balances_buffer.len() {
      let ((id, amount), length) = Index::decode_rune_balance(&balances_buffer[i..]).unwrap();
      i += length;

      let entry = RuneEntry::load(id_to_rune_entries.get(id.store())?.unwrap().value());

      balances.insert(
        entry.spaced_rune,
        Pile {
          amount,
          divisibility: entry.divisibility,
          symbol: entry.symbol,
        },
      );
    }

    Ok(Some(balances))
  }

  pub fn get_rune_balance_map(&self) -> Result<BTreeMap<SpacedRune, BTreeMap<OutPoint, Pile>>> {
    let outpoint_balances = self.get_rune_balances()?;

    let rtx = self.database.begin_read()?;

    let rune_id_to_rune_entry = rtx.open_table(RUNE_ID_TO_RUNE_ENTRY)?;

    let mut rune_balances_by_id: BTreeMap<RuneId, BTreeMap<OutPoint, u128>> = BTreeMap::new();

    for (outpoint, balances) in outpoint_balances {
      for (rune_id, amount) in balances {
        *rune_balances_by_id
          .entry(rune_id)
          .or_default()
          .entry(outpoint)
          .or_default() += amount;
      }
    }

    let mut rune_balances = BTreeMap::new();

    for (rune_id, balances) in rune_balances_by_id {
      let RuneEntry {
        divisibility,
        spaced_rune,
        symbol,
        ..
      } = RuneEntry::load(
        rune_id_to_rune_entry
          .get(&rune_id.store())?
          .unwrap()
          .value(),
      );

      rune_balances.insert(
        spaced_rune,
        balances
          .into_iter()
          .map(|(outpoint, amount)| {
            (
              outpoint,
              Pile {
                amount,
                divisibility,
                symbol,
              },
            )
          })
          .collect(),
      );
    }

    Ok(rune_balances)
  }

  pub fn get_rune_balances(&self) -> Result<Vec<(OutPoint, Vec<(RuneId, u128)>)>> {
    let mut result = Vec::new();

    for entry in self
      .database
      .begin_read()?
      .open_table(OUTPOINT_TO_RUNE_BALANCES)?
      .iter()?
    {
      let (outpoint, balances_buffer) = entry?;
      let outpoint = OutPoint::load(*outpoint.value());
      let balances_buffer = balances_buffer.value();

      let mut balances = Vec::new();
      let mut i = 0;
      while i < balances_buffer.len() {
        let ((id, balance), length) = Index::decode_rune_balance(&balances_buffer[i..]).unwrap();
        i += length;
        balances.push((id, balance));
      }

      result.push((outpoint, balances));
    }

    Ok(result)
  }

  pub fn block_header(&self, hash: BlockHash) -> Result<Option<Header>> {
    self.client.get_block_header(&hash).into_option()
  }

  pub fn block_header_info(&self, hash: BlockHash) -> Result<Option<GetBlockHeaderResult>> {
    self.client.get_block_header_info(&hash).into_option()
  }

  pub fn block_stats(&self, height: u64) -> Result<Option<GetBlockStatsResult>> {
    self.client.get_block_stats(height).into_option()
  }

  pub fn get_block_by_height(&self, height: u32) -> Result<Option<Block>> {
    Ok(
      self
        .client
        .get_block_hash(height.into())
        .into_option()?
        .map(|hash| self.client.get_block(&hash))
        .transpose()?,
    )
  }

  pub fn get_block_by_hash(&self, hash: BlockHash) -> Result<Option<Block>> {
    self.client.get_block(&hash).into_option()
  }

  pub fn get_collections_paginated(
    &self,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<InscriptionId>, bool)> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let mut collections = rtx
      .open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?
      .iter()?
      .skip(page_index.saturating_mul(page_size))
      .take(page_size.saturating_add(1))
      .map(|result| {
        result
          .and_then(|(parent, _children)| {
            sequence_number_to_inscription_entry
              .get(parent.value())
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let more = collections.len() > page_size;

    if more {
      collections.pop();
    }

    Ok((collections, more))
  }

  #[cfg(test)]
  pub(crate) fn get_children_by_inscription_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Vec<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let Some(sequence_number) = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
      .get(&inscription_id.store())?
      .map(|sequence_number| sequence_number.value())
    else {
      return Ok(Vec::new());
    };

    self
      .get_children_by_sequence_number_paginated(sequence_number, usize::MAX, 0)
      .map(|(children, _more)| children)
  }

  #[cfg(test)]
  pub(crate) fn get_parents_by_inscription_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Vec<InscriptionId> {
    let rtx = self.database.begin_read().unwrap();

    let sequence_number = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)
      .unwrap()
      .get(&inscription_id.store())
      .unwrap()
      .unwrap()
      .value();

    let sequence_number_to_inscription_entry = rtx
      .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)
      .unwrap();

    let parent_sequences = InscriptionEntry::load(
      sequence_number_to_inscription_entry
        .get(sequence_number)
        .unwrap()
        .unwrap()
        .value(),
    )
    .parents;

    parent_sequences
      .into_iter()
      .map(|parent_sequence_number| {
        InscriptionEntry::load(
          sequence_number_to_inscription_entry
            .get(parent_sequence_number)
            .unwrap()
            .unwrap()
            .value(),
        )
        .id
      })
      .collect()
  }

  pub fn get_children_by_sequence_number_paginated(
    &self,
    sequence_number: u32,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<InscriptionId>, bool)> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_entry = rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let mut children = rtx
      .open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?
      .get(sequence_number)?
      .skip(page_index * page_size)
      .take(page_size.saturating_add(1))
      .map(|result| {
        result
          .and_then(|sequence_number| {
            sequence_number_to_entry
              .get(sequence_number.value())
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let more = children.len() > page_size;

    if more {
      children.pop();
    }

    Ok((children, more))
  }

  pub fn get_parents_by_sequence_number_paginated(
    &self,
    parent_sequence_numbers: Vec<u32>,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<InscriptionId>, bool)> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_entry = rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let mut parents = parent_sequence_numbers
      .iter()
      .skip(page_index * page_size)
      .take(page_size.saturating_add(1))
      .map(|sequence_number| {
        sequence_number_to_entry
          .get(sequence_number)
          .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let more_parents = parents.len() > page_size;

    if more_parents {
      parents.pop();
    }

    Ok((parents, more_parents))
  }

  pub fn get_etching(&self, txid: Txid) -> Result<Option<SpacedRune>> {
    let rtx = self.database.begin_read()?;

    let transaction_id_to_rune = rtx.open_table(TRANSACTION_ID_TO_RUNE)?;
    let Some(rune) = transaction_id_to_rune.get(&txid.store())? else {
      return Ok(None);
    };

    let rune_to_rune_id = rtx.open_table(RUNE_TO_RUNE_ID)?;
    let id = rune_to_rune_id.get(rune.value())?.unwrap();

    let rune_id_to_rune_entry = rtx.open_table(RUNE_ID_TO_RUNE_ENTRY)?;
    let entry = rune_id_to_rune_entry.get(&id.value())?.unwrap();

    Ok(Some(RuneEntry::load(entry.value()).spaced_rune))
  }

  pub fn get_inscription_ids_by_sat(&self, sat: Sat) -> Result<Vec<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let ids = rtx
      .open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?
      .get(&sat.n())?
      .map(|result| {
        result
          .and_then(|sequence_number| {
            let sequence_number = sequence_number.value();
            sequence_number_to_inscription_entry
              .get(sequence_number)
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    Ok(ids)
  }

  pub fn get_inscription_ids_by_sat_paginated(
    &self,
    sat: Sat,
    page_size: u64,
    page_index: u64,
  ) -> Result<(Vec<InscriptionId>, bool)> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let mut ids = rtx
      .open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?
      .get(&sat.n())?
      .skip(page_index.saturating_mul(page_size).try_into().unwrap())
      .take(page_size.saturating_add(1).try_into().unwrap())
      .map(|result| {
        result
          .and_then(|sequence_number| {
            let sequence_number = sequence_number.value();
            sequence_number_to_inscription_entry
              .get(sequence_number)
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let more = ids.len() > page_size.try_into().unwrap();

    if more {
      ids.pop();
    }

    Ok((ids, more))
  }

  pub fn get_inscription_id_by_sat_indexed(
    &self,
    sat: Sat,
    inscription_index: isize,
  ) -> Result<Option<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let sat_to_sequence_number = rtx.open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?;

    if inscription_index < 0 {
      sat_to_sequence_number
        .get(&sat.n())?
        .nth_back((inscription_index + 1).abs_diff(0))
    } else {
      sat_to_sequence_number
        .get(&sat.n())?
        .nth(inscription_index.abs_diff(0))
    }
    .map(|result| {
      result
        .and_then(|sequence_number| {
          let sequence_number = sequence_number.value();
          sequence_number_to_inscription_entry
            .get(sequence_number)
            .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
        })
        .map_err(|err| anyhow!(err.to_string()))
    })
    .transpose()
  }

  #[cfg(test)]
  pub(crate) fn get_inscription_id_by_inscription_number(
    &self,
    inscription_number: i32,
  ) -> Result<Option<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let Some(sequence_number) = rtx
      .open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?
      .get(inscription_number)?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let inscription_id = rtx
      .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
      .get(&sequence_number)?
      .map(|entry| InscriptionEntry::load(entry.value()).id);

    Ok(inscription_id)
  }

  pub fn get_inscription_satpoint_by_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<SatPoint>> {
    let rtx = self.database.begin_read()?;

    let Some(sequence_number) = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
      .get(&inscription_id.store())?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let satpoint = rtx
      .open_table(SEQUENCE_NUMBER_TO_SATPOINT)?
      .get(sequence_number)?
      .map(|satpoint| Entry::load(*satpoint.value()));

    Ok(satpoint)
  }

  pub fn get_inscription_by_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<Inscription>> {
    if !self.inscription_exists(inscription_id)? {
      return Ok(None);
    }

    Ok(self.get_transaction(inscription_id.txid)?.and_then(|tx| {
      ParsedEnvelope::from_transaction(&tx)
        .into_iter()
        .nth(inscription_id.index as usize)
        .map(|envelope| envelope.payload)
    }))
  }

  pub fn inscription_count(&self, txid: Txid) -> Result<u32> {
    let start = InscriptionId { index: 0, txid };

    let end = InscriptionId {
      index: u32::MAX,
      txid,
    };

    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
        .range::<&InscriptionIdValue>(&start.store()..&end.store())?
        .count()
        .try_into()
        .unwrap(),
    )
  }

  pub fn inscription_exists(&self, inscription_id: InscriptionId) -> Result<bool> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
        .get(&inscription_id.store())?
        .is_some(),
    )
  }

  pub fn get_inscriptions_on_output_with_satpoints(
    &self,
    outpoint: OutPoint,
  ) -> Result<Option<Vec<(SatPoint, InscriptionId)>>> {
    if !self.index_inscriptions {
      return Ok(None);
    }

    let rtx = self.database.begin_read()?;
    let outpoint_to_utxo_entry = rtx.open_table(OUTPOINT_TO_UTXO_ENTRY)?;
    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    self.inscriptions_on_output(
      &outpoint_to_utxo_entry,
      &sequence_number_to_inscription_entry,
      outpoint,
    )
  }

  pub fn get_inscriptions_for_output(
    &self,
    outpoint: OutPoint,
  ) -> Result<Option<Vec<InscriptionId>>> {
    let Some(inscriptions) = self.get_inscriptions_on_output_with_satpoints(outpoint)? else {
      return Ok(None);
    };

    Ok(Some(
      inscriptions
        .iter()
        .map(|(_satpoint, inscription_id)| *inscription_id)
        .collect(),
    ))
  }

  pub fn get_inscriptions_for_outputs(
    &self,
    outpoints: &Vec<OutPoint>,
  ) -> Result<Option<Vec<InscriptionId>>> {
    let mut result = Vec::new();
    for outpoint in outpoints {
      let Some(inscriptions) = self.get_inscriptions_on_output_with_satpoints(*outpoint)? else {
        return Ok(None);
      };

      result.extend(
        inscriptions
          .iter()
          .map(|(_satpoint, inscription_id)| *inscription_id),
      );
    }

    Ok(Some(result))
  }

  pub fn get_transaction(&self, txid: Txid) -> Result<Option<Transaction>> {
    if txid == self.genesis_block_coinbase_txid {
      return Ok(Some(self.genesis_block_coinbase_transaction.clone()));
    }

    if self.index_transactions {
      if let Some(transaction) = self
        .database
        .begin_read()?
        .open_table(TRANSACTION_ID_TO_TRANSACTION)?
        .get(&txid.store())?
      {
        return Ok(Some(consensus::encode::deserialize(transaction.value())?));
      }
    }

    self.client.get_raw_transaction(&txid, None).into_option()
  }

  pub fn find(&self, sat: Sat) -> Result<Option<SatPoint>> {
    let sat = sat.0;
    let rtx = self.begin_read()?;

    if rtx.block_count()? <= Sat(sat).height().n() {
      return Ok(None);
    }

    let outpoint_to_utxo_entry = rtx.0.open_table(OUTPOINT_TO_UTXO_ENTRY)?;

    for entry in outpoint_to_utxo_entry.iter()? {
      let (outpoint, utxo_entry) = entry?;
      let sat_ranges = utxo_entry.value().parse(self).sat_ranges();

      let mut offset = 0;
      for chunk in sat_ranges.chunks_exact(11) {
        let (start, end) = SatRange::load(chunk.try_into().unwrap());
        if start <= sat && sat < end {
          return Ok(Some(SatPoint {
            outpoint: Entry::load(*outpoint.value()),
            offset: offset + sat - start,
          }));
        }
        offset += end - start;
      }
    }

    Ok(None)
  }

  pub fn find_range(
    &self,
    range_start: Sat,
    range_end: Sat,
  ) -> Result<Option<Vec<FindRangeOutput>>> {
    let range_start = range_start.0;
    let range_end = range_end.0;
    let rtx = self.begin_read()?;

    if rtx.block_count()? < Sat(range_end - 1).height().n() + 1 {
      return Ok(None);
    }

    let Some(mut remaining_sats) = range_end.checked_sub(range_start) else {
      return Err(anyhow!("range end is before range start"));
    };

    let outpoint_to_utxo_entry = rtx.0.open_table(OUTPOINT_TO_UTXO_ENTRY)?;

    let mut result = Vec::new();
    for entry in outpoint_to_utxo_entry.iter()? {
      let (outpoint, utxo_entry) = entry?;
      let sat_ranges = utxo_entry.value().parse(self).sat_ranges();

      let mut offset = 0;
      for sat_range in sat_ranges.chunks_exact(11) {
        let (start, end) = SatRange::load(sat_range.try_into().unwrap());

        if end > range_start && start < range_end {
          let overlap_start = start.max(range_start);
          let overlap_end = end.min(range_end);

          result.push(FindRangeOutput {
            start: overlap_start,
            size: overlap_end - overlap_start,
            satpoint: SatPoint {
              outpoint: Entry::load(*outpoint.value()),
              offset: offset + overlap_start - start,
            },
          });

          remaining_sats -= overlap_end - overlap_start;

          if remaining_sats == 0 {
            break;
          }
        }
        offset += end - start;
      }
    }

    Ok(Some(result))
  }

  pub fn list(&self, outpoint: OutPoint) -> Result<Option<Vec<(u64, u64)>>> {
    if !self.index_sats {
      return Ok(None);
    }

    Ok(
      self
        .database
        .begin_read()?
        .open_table(OUTPOINT_TO_UTXO_ENTRY)?
        .get(&outpoint.store())?
        .map(|utxo_entry| {
          utxo_entry
            .value()
            .parse(self)
            .sat_ranges()
            .chunks_exact(11)
            .map(|chunk| SatRange::load(chunk.try_into().unwrap()))
            .collect::<Vec<(u64, u64)>>()
        }),
    )
  }

  pub fn is_output_spent(&self, outpoint: OutPoint) -> Result<bool> {
    Ok(
      outpoint != OutPoint::null()
        && outpoint != self.settings.chain().genesis_coinbase_outpoint()
        && if self.have_full_utxo_index() {
          self
            .database
            .begin_read()?
            .open_table(OUTPOINT_TO_UTXO_ENTRY)?
            .get(&outpoint.store())?
            .is_none()
        } else {
          self
            .client
            .get_tx_out(&outpoint.txid, outpoint.vout, Some(true))?
            .is_none()
        },
    )
  }

  pub fn is_output_in_active_chain(&self, outpoint: OutPoint) -> Result<bool> {
    if outpoint == OutPoint::null() {
      return Ok(true);
    }

    if outpoint == self.settings.chain().genesis_coinbase_outpoint() {
      return Ok(true);
    }

    let Some(info) = self
      .client
      .get_raw_transaction_info(&outpoint.txid, None)
      .into_option()?
    else {
      return Ok(false);
    };

    if info.blockhash.is_none() {
      return Ok(false);
    }

    if outpoint.vout.into_usize() >= info.vout.len() {
      return Ok(false);
    }

    Ok(true)
  }

  pub fn block_time(&self, height: Height) -> Result<Blocktime> {
    let height = height.n();

    let rtx = self.database.begin_read()?;

    let height_to_block_header = rtx.open_table(HEIGHT_TO_BLOCK_HEADER)?;

    if let Some(guard) = height_to_block_header.get(height)? {
      return Ok(Blocktime::confirmed(Header::load(*guard.value()).time));
    }

    let current = height_to_block_header
      .range(0..)?
      .next_back()
      .transpose()?
      .map(|(height, _header)| height)
      .map(|x| x.value())
      .unwrap_or(0);

    let expected_blocks = height
      .checked_sub(current)
      .with_context(|| format!("current {current} height is greater than sat height {height}"))?;

    Ok(Blocktime::Expected(
      Utc::now()
        .round_subsecs(0)
        .checked_add_signed(
          chrono::Duration::try_seconds(10 * 60 * i64::from(expected_blocks))
            .context("timestamp out of range")?,
        )
        .context("timestamp out of range")?,
    ))
  }

  pub fn get_inscriptions_paginated(
    &self,
    page_size: u32,
    page_index: u32,
  ) -> Result<(Vec<InscriptionId>, bool)> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let last = sequence_number_to_inscription_entry
      .iter()?
      .next_back()
      .map(|result| result.map(|(number, _entry)| number.value()))
      .transpose()?
      .unwrap_or_default();

    let start = last.saturating_sub(page_size.saturating_mul(page_index));

    let end = start.saturating_sub(page_size);

    let mut inscriptions = sequence_number_to_inscription_entry
      .range(end..=start)?
      .rev()
      .map(|result| result.map(|(_number, entry)| InscriptionEntry::load(entry.value()).id))
      .collect::<Result<Vec<InscriptionId>, StorageError>>()?;

    let more = u32::try_from(inscriptions.len()).unwrap_or(u32::MAX) > page_size;

    if more {
      inscriptions.pop();
    }

    Ok((inscriptions, more))
  }

  pub fn get_inscriptions_in_block(&self, block_height: u32) -> Result<Vec<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let height_to_last_sequence_number = rtx.open_table(HEIGHT_TO_LAST_SEQUENCE_NUMBER)?;
    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let Some(newest_sequence_number) = height_to_last_sequence_number
      .get(&block_height)?
      .map(|ag| ag.value())
    else {
      return Ok(Vec::new());
    };

    let oldest_sequence_number = height_to_last_sequence_number
      .get(block_height.saturating_sub(1))?
      .map(|ag| ag.value())
      .unwrap_or(0);

    (oldest_sequence_number..newest_sequence_number)
      .map(|num| match sequence_number_to_inscription_entry.get(&num) {
        Ok(Some(inscription_id)) => Ok(InscriptionEntry::load(inscription_id.value()).id),
        Ok(None) => Err(anyhow!(
          "could not find inscription for inscription number {num}"
        )),
        Err(err) => Err(anyhow!(err)),
      })
      .collect::<Result<Vec<InscriptionId>>>()
  }

  pub fn get_runes_in_block(&self, block_height: u64) -> Result<Vec<SpacedRune>> {
    let rtx = self.database.begin_read()?;

    let rune_id_to_rune_entry = rtx.open_table(RUNE_ID_TO_RUNE_ENTRY)?;

    let min_id = RuneId {
      block: block_height,
      tx: 0,
    };

    let max_id = RuneId {
      block: block_height,
      tx: u32::MAX,
    };

    let runes = rune_id_to_rune_entry
      .range(min_id.store()..=max_id.store())?
      .map(|result| result.map(|(_, entry)| RuneEntry::load(entry.value()).spaced_rune))
      .collect::<Result<Vec<SpacedRune>, StorageError>>()?;

    Ok(runes)
  }

  pub fn get_highest_paying_inscriptions_in_block(
    &self,
    block_height: u32,
    n: usize,
  ) -> Result<(Vec<InscriptionId>, usize)> {
    let inscription_ids = self.get_inscriptions_in_block(block_height)?;

    let mut inscription_to_fee: Vec<(InscriptionId, u64)> = Vec::new();
    for id in &inscription_ids {
      inscription_to_fee.push((
        *id,
        self
          .get_inscription_entry(*id)?
          .ok_or_else(|| anyhow!("could not get entry for inscription {id}"))?
          .fee,
      ));
    }

    inscription_to_fee.sort_by_key(|(_, fee)| *fee);

    Ok((
      inscription_to_fee
        .iter()
        .map(|(id, _)| *id)
        .rev()
        .take(n)
        .collect(),
      inscription_ids.len(),
    ))
  }

  pub fn get_home_inscriptions(&self) -> Result<Vec<InscriptionId>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(HOME_INSCRIPTIONS)?
        .iter()?
        .rev()
        .flat_map(|result| result.map(|(_number, id)| InscriptionId::load(id.value())))
        .collect(),
    )
  }

  pub fn get_feed_inscriptions(&self, n: usize) -> Result<Vec<(u32, InscriptionId)>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
        .iter()?
        .rev()
        .take(n)
        .flat_map(|result| {
          result.map(|(number, entry)| (number.value(), InscriptionEntry::load(entry.value()).id))
        })
        .collect(),
    )
  }

  pub(crate) fn inscription_info(
    &self,
    query: query::Inscription,
    child: Option<usize>,
  ) -> Result<Option<(api::Inscription, Option<TxOut>, Inscription)>> {
    let rtx = self.database.begin_read()?;

    let sequence_number = match query {
      query::Inscription::Id(id) => rtx
        .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
        .get(&id.store())?
        .map(|guard| guard.value()),
      query::Inscription::Number(inscription_number) => rtx
        .open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?
        .get(inscription_number)?
        .map(|guard| guard.value()),
      query::Inscription::Sat(sat) => rtx
        .open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?
        .get(sat.n())?
        .next()
        .transpose()?
        .map(|guard| guard.value()),
    };

    let Some(sequence_number) = sequence_number else {
      return Ok(None);
    };

    let sequence_number = if let Some(child) = child {
      let Some(child) = rtx
        .open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?
        .get(sequence_number)?
        .nth(child)
        .transpose()?
        .map(|child| child.value())
      else {
        return Ok(None);
      };

      child
    } else {
      sequence_number
    };

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let entry = InscriptionEntry::load(
      sequence_number_to_inscription_entry
        .get(&sequence_number)?
        .unwrap()
        .value(),
    );

    let Some(transaction) = self.get_transaction(entry.id.txid)? else {
      return Ok(None);
    };

    let Some(inscription) = ParsedEnvelope::from_transaction(&transaction)
      .into_iter()
      .nth(entry.id.index as usize)
      .map(|envelope| envelope.payload)
    else {
      return Ok(None);
    };

    let satpoint = SatPoint::load(
      *rtx
        .open_table(SEQUENCE_NUMBER_TO_SATPOINT)?
        .get(sequence_number)?
        .unwrap()
        .value(),
    );

    let output = if satpoint.outpoint == unbound_outpoint() || satpoint.outpoint == OutPoint::null()
    {
      None
    } else {
      let Some(transaction) = self.get_transaction(satpoint.outpoint.txid)? else {
        return Ok(None);
      };

      transaction
        .output
        .into_iter()
        .nth(satpoint.outpoint.vout.try_into().unwrap())
    };

    let previous = if let Some(n) = sequence_number.checked_sub(1) {
      Some(
        InscriptionEntry::load(
          sequence_number_to_inscription_entry
            .get(n)?
            .unwrap()
            .value(),
        )
        .id,
      )
    } else {
      None
    };

    let next = sequence_number_to_inscription_entry
      .get(sequence_number + 1)?
      .map(|guard| InscriptionEntry::load(guard.value()).id);

    let all_children = rtx
      .open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?
      .get(sequence_number)?;

    let child_count = all_children.len();

    let children = all_children
      .take(4)
      .map(|result| {
        result
          .and_then(|sequence_number| {
            sequence_number_to_inscription_entry
              .get(sequence_number.value())
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let rune = if let Some(rune_id) = rtx
      .open_table(SEQUENCE_NUMBER_TO_RUNE_ID)?
      .get(sequence_number)?
    {
      let rune_id_to_rune_entry = rtx.open_table(RUNE_ID_TO_RUNE_ENTRY)?;
      let entry = rune_id_to_rune_entry.get(&rune_id.value())?.unwrap();
      Some(RuneEntry::load(entry.value()).spaced_rune)
    } else {
      None
    };

    let parents = entry
      .parents
      .iter()
      .take(4)
      .map(|parent| {
        Ok(
          InscriptionEntry::load(
            sequence_number_to_inscription_entry
              .get(parent)?
              .unwrap()
              .value(),
          )
          .id,
        )
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let mut charms = entry.charms;

    if satpoint.outpoint == OutPoint::null() {
      Charm::Lost.set(&mut charms);
    }

    let effective_mime_type = if let Some(delegate_id) = inscription.delegate() {
      let delegate_result = self.get_inscription_by_id(delegate_id);
      if let Ok(Some(delegate)) = delegate_result {
        delegate.content_type().map(str::to_string)
      } else {
        inscription.content_type().map(str::to_string)
      }
    } else {
      inscription.content_type().map(str::to_string)
    };

    Ok(Some((
      api::Inscription {
        address: output
          .as_ref()
          .and_then(|o| {
            self
              .settings
              .chain()
              .address_from_script(&o.script_pubkey)
              .ok()
          })
          .map(|address| address.to_string()),
        charms: Charm::charms(charms),
        child_count,
        children,
        content_length: inscription.content_length(),
        content_type: inscription.content_type().map(|s| s.to_string()),
        effective_content_type: effective_mime_type,
        fee: entry.fee,
        height: entry.height,
        id: entry.id,
        next,
        number: entry.inscription_number,
        parents,
        previous,
        rune,
        sat: entry.sat,
        satpoint,
        timestamp: timestamp(entry.timestamp.into()).timestamp(),
        value: output.as_ref().map(|o| o.value.to_sat()),
        metaprotocol: inscription.metaprotocol().map(|s| s.to_string()),
      },
      output,
      inscription,
    )))
  }

  pub fn get_inscription_entry(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<InscriptionEntry>> {
    let rtx = self.database.begin_read()?;

    let Some(sequence_number) = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
      .get(&inscription_id.store())?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let entry = rtx
      .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
      .get(sequence_number)?
      .map(|value| InscriptionEntry::load(value.value()));

    Ok(entry)
  }

  #[cfg(test)]
  fn assert_inscription_location(
    &self,
    inscription_id: InscriptionId,
    satpoint: SatPoint,
    sat: Option<u64>,
  ) {
    let rtx = self.database.begin_read().unwrap();

    let outpoint_to_utxo_entry = rtx.open_table(OUTPOINT_TO_UTXO_ENTRY).unwrap();

    let sequence_number_to_satpoint = rtx.open_table(SEQUENCE_NUMBER_TO_SATPOINT).unwrap();

    let sequence_number = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)
      .unwrap()
      .get(&inscription_id.store())
      .unwrap()
      .unwrap()
      .value();

    assert_eq!(
      SatPoint::load(
        *sequence_number_to_satpoint
          .get(sequence_number)
          .unwrap()
          .unwrap()
          .value()
      ),
      satpoint,
    );

    let utxo_entry = outpoint_to_utxo_entry
      .get(&satpoint.outpoint.store())
      .unwrap()
      .unwrap();
    let parsed_inscriptions = utxo_entry.value().parse(self).parse_inscriptions();
    let satpoint_offsets: Vec<u64> = parsed_inscriptions
      .iter()
      .copied()
      .filter_map(|(seq, offset)| (seq == sequence_number).then_some(offset))
      .collect();
    assert!(satpoint_offsets == [satpoint.offset]);

    match sat {
      Some(sat) => {
        if self.index_sats {
          // unbound inscriptions should not be assigned to a sat
          assert_ne!(satpoint.outpoint, unbound_outpoint());

          assert!(rtx
            .open_multimap_table(SAT_TO_SEQUENCE_NUMBER)
            .unwrap()
            .get(&sat)
            .unwrap()
            .any(|entry| entry.unwrap().value() == sequence_number));

          // we do not track common sats (only the sat ranges)
          if !Sat(sat).common() {
            assert_eq!(
              SatPoint::load(
                *rtx
                  .open_table(SAT_TO_SATPOINT)
                  .unwrap()
                  .get(&sat)
                  .unwrap()
                  .unwrap()
                  .value()
              ),
              satpoint,
            );
          }
        }
      }
      None => {
        if self.index_sats {
          assert_eq!(satpoint.outpoint, unbound_outpoint())
        }
      }
    }
  }

  fn inscriptions_on_output<'a: 'tx, 'tx>(
    &self,
    outpoint_to_utxo_entry: &'a impl ReadableTable<&'static OutPointValue, &'static UtxoEntry>,
    sequence_number_to_inscription_entry: &'a impl ReadableTable<u32, InscriptionEntryValue>,
    outpoint: OutPoint,
  ) -> Result<Option<Vec<(SatPoint, InscriptionId)>>> {
    if !self.index_inscriptions {
      return Ok(None);
    }

    let Some(utxo_entry) = outpoint_to_utxo_entry.get(&outpoint.store())? else {
      return Ok(Some(Vec::new()));
    };

    let mut inscriptions = utxo_entry.value().parse(self).parse_inscriptions();

    inscriptions.sort_by_key(|(sequence_number, _)| *sequence_number);

    inscriptions
      .into_iter()
      .map(|(sequence_number, offset)| {
        let entry = sequence_number_to_inscription_entry
          .get(sequence_number)?
          .unwrap();

        let satpoint = SatPoint { outpoint, offset };

        Ok((satpoint, InscriptionEntry::load(entry.value()).id))
      })
      .collect::<Result<_>>()
      .map(Some)
  }

  pub fn get_address_info(&self, address: &Address) -> Result<Vec<OutPoint>> {
    self
      .database
      .begin_read()?
      .open_multimap_table(SCRIPT_PUBKEY_TO_OUTPOINT)?
      .get(address.script_pubkey().as_bytes())?
      .map(|result| {
        result
          .map_err(|err| anyhow!(err))
          .map(|value| OutPoint::load(value.value()))
      })
      .collect()
  }

  pub(crate) fn get_aggregated_rune_balances_for_outputs(
    &self,
    outputs: &Vec<OutPoint>,
  ) -> Result<Option<Vec<(SpacedRune, Decimal, Option<char>)>>> {
    let mut runes = BTreeMap::new();

    for output in outputs {
      let Some(rune_balances) = self.get_rune_balances_for_output(*output)? else {
        return Ok(None);
      };

      for (spaced_rune, pile) in rune_balances {
        runes
          .entry(spaced_rune)
          .and_modify(|(decimal, _symbol): &mut (Decimal, Option<char>)| {
            assert_eq!(decimal.scale, pile.divisibility);
            decimal.value += pile.amount;
          })
          .or_insert((
            Decimal {
              value: pile.amount,
              scale: pile.divisibility,
            },
            pile.symbol,
          ));
      }
    }

    Ok(Some(
      runes
        .into_iter()
        .map(|(spaced_rune, (decimal, symbol))| (spaced_rune, decimal, symbol))
        .collect(),
    ))
  }

  pub(crate) fn get_sat_balances_for_outputs(&self, outputs: &Vec<OutPoint>) -> Result<u64> {
    let outpoint_to_utxo_entry = self
      .database
      .begin_read()?
      .open_table(OUTPOINT_TO_UTXO_ENTRY)?;

    let mut acc = 0;
    for output in outputs {
      if let Some(utxo_entry) = outpoint_to_utxo_entry.get(&output.store())? {
        acc += utxo_entry.value().parse(self).total_value();
      };
    }

    Ok(acc)
  }

  pub(crate) fn get_utxo_recursive(
    &self,
    outpoint: OutPoint,
  ) -> Result<Option<api::UtxoRecursive>> {
    let Some(utxo_entry) = self
      .database
      .begin_read()?
      .open_table(OUTPOINT_TO_UTXO_ENTRY)?
      .get(&outpoint.store())?
    else {
      return Ok(None);
    };

    Ok(Some(api::UtxoRecursive {
      inscriptions: self.get_inscriptions_for_output(outpoint)?,
      runes: self.get_rune_balances_for_output(outpoint)?,
      sat_ranges: self.list(outpoint)?,
      value: utxo_entry.value().parse(self).total_value(),
    }))
  }

  pub(crate) fn get_output_info(&self, outpoint: OutPoint) -> Result<Option<(api::Output, TxOut)>> {
    let sat_ranges = self.list(outpoint)?;

    let indexed;

    let txout = if outpoint == OutPoint::null() || outpoint == unbound_outpoint() {
      let mut value = 0;

      if let Some(ranges) = &sat_ranges {
        for (start, end) in ranges {
          value += end - start;
        }
      }

      indexed = true;

      TxOut {
        value: Amount::from_sat(value),
        script_pubkey: ScriptBuf::new(),
      }
    } else {
      indexed = self.contains_output(&outpoint)?;

      let Some(tx) = self.get_transaction(outpoint.txid)? else {
        return Ok(None);
      };

      let Some(txout) = tx.output.into_iter().nth(outpoint.vout as usize) else {
        return Ok(None);
      };

      txout
    };

    let inscriptions = self.get_inscriptions_for_output(outpoint)?;

    let runes = self.get_rune_balances_for_output(outpoint)?;

    let spent = self.is_output_spent(outpoint)?;

    Ok(Some((
      api::Output::new(
        self.settings.chain(),
        inscriptions,
        outpoint,
        txout.clone(),
        indexed,
        runes,
        sat_ranges,
        spent,
      ),
      txout,
    )))
  }
}

#[cfg(test)]
mod tests {
  use {super::*, crate::index::testing::Context};

  #[test]
  fn height_limit() {
    {
      let context = Context::builder().args(["--height-limit", "0"]).build();
      context.mine_blocks(1);
      assert_eq!(context.index.block_height().unwrap(), None);
      assert_eq!(context.index.block_count().unwrap(), 0);
    }

    {
      let context = Context::builder().args(["--height-limit", "1"]).build();
      context.mine_blocks(1);
      assert_eq!(context.index.block_height().unwrap(), Some(Height(0)));
      assert_eq!(context.index.block_count().unwrap(), 1);
    }

    {
      let context = Context::builder().args(["--height-limit", "2"]).build();
      context.mine_blocks(2);
      assert_eq!(context.index.block_height().unwrap(), Some(Height(1)));
      assert_eq!(context.index.block_count().unwrap(), 2);
    }
  }

  #[test]
  fn inscriptions_below_first_inscription_height_are_skipped() {
    let inscription = inscription("text/plain;charset=utf-8", "hello");
    let template = TransactionTemplate {
      inputs: &[(1, 0, 0, inscription.to_witness())],
      ..default()
    };

    {
      let context = Context::builder().build();
      context.mine_blocks(1);
      let txid = context.core.broadcast_tx(template.clone());
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks(1);

      assert_eq!(
        context.index.get_inscription_by_id(inscription_id).unwrap(),
        Some(inscription)
      );

      assert_eq!(
        context
          .index
          .get_inscription_satpoint_by_id(inscription_id)
          .unwrap(),
        Some(SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        })
      );
    }

    {
      let context = Context::builder().chain(Chain::Mainnet).build();
      context.mine_blocks(1);
      let txid = context.core.broadcast_tx(template);
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscription_satpoint_by_id(inscription_id)
          .unwrap(),
        None,
      );
    }
  }

  #[test]
  fn inscriptions_are_not_indexed_if_no_index_inscriptions_flag_is_set() {
    let inscription = inscription("text/plain;charset=utf-8", "hello");
    let template = TransactionTemplate {
      inputs: &[(1, 0, 0, inscription.to_witness())],
      ..default()
    };

    {
      let context = Context::builder().build();
      context.mine_blocks(1);
      let txid = context.core.broadcast_tx(template.clone());
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks(1);

      assert_eq!(
        context.index.get_inscription_by_id(inscription_id).unwrap(),
        Some(inscription)
      );

      assert_eq!(
        context
          .index
          .get_inscription_satpoint_by_id(inscription_id)
          .unwrap(),
        Some(SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        })
      );
    }

    {
      let context = Context::builder().arg("--no-index-inscriptions").build();
      context.mine_blocks(1);
      let txid = context.core.broadcast_tx(template);
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscription_satpoint_by_id(inscription_id)
          .unwrap(),
        None,
      );
    }
  }

  #[test]
  fn list_first_coinbase_transaction() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context
        .index
        .list(
          "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b:0"
            .parse()
            .unwrap()
        )
        .unwrap()
        .unwrap(),
      &[(0, 50 * COIN_VALUE)],
    )
  }

  #[test]
  fn list_second_coinbase_transaction() {
    let context = Context::builder().arg("--index-sats").build();
    let txid = context.mine_blocks(1)[0].txdata[0].compute_txid();
    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      &[(50 * COIN_VALUE, 100 * COIN_VALUE)],
    )
  }

  #[test]
  fn list_split_ranges_are_tracked_correctly() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let split_coinbase_output = TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      outputs: 2,
      fee: 0,
      ..default()
    };
    let txid = context.core.broadcast_tx(split_coinbase_output);

    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      &[(50 * COIN_VALUE, 75 * COIN_VALUE)],
    );

    assert_eq!(
      context.index.list(OutPoint::new(txid, 1)).unwrap().unwrap(),
      &[(75 * COIN_VALUE, 100 * COIN_VALUE)],
    );
  }

  #[test]
  fn list_merge_ranges_are_tracked_correctly() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(2);
    let merge_coinbase_outputs = TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default()), (2, 0, 0, Default::default())],
      fee: 0,
      ..default()
    };

    let txid = context.core.broadcast_tx(merge_coinbase_outputs);
    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      &[
        (50 * COIN_VALUE, 100 * COIN_VALUE),
        (100 * COIN_VALUE, 150 * COIN_VALUE)
      ],
    );
  }

  #[test]
  fn list_fee_paying_transaction_range() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let fee_paying_tx = TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      outputs: 2,
      fee: 10,
      ..default()
    };
    let txid = context.core.broadcast_tx(fee_paying_tx);
    let coinbase_txid = context.mine_blocks(1)[0].txdata[0].compute_txid();

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      &[(50 * COIN_VALUE, 7499999995)],
    );

    assert_eq!(
      context.index.list(OutPoint::new(txid, 1)).unwrap().unwrap(),
      &[(7499999995, 9999999990)],
    );

    assert_eq!(
      context
        .index
        .list(OutPoint::new(coinbase_txid, 0))
        .unwrap()
        .unwrap(),
      &[(10000000000, 15000000000), (9999999990, 10000000000)],
    );
  }

  #[test]
  fn list_two_fee_paying_transaction_range() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(2);
    let first_fee_paying_tx = TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      fee: 10,
      ..default()
    };
    let second_fee_paying_tx = TransactionTemplate {
      inputs: &[(2, 0, 0, Default::default())],
      fee: 10,
      ..default()
    };
    context.core.broadcast_tx(first_fee_paying_tx);
    context.core.broadcast_tx(second_fee_paying_tx);

    let coinbase_txid = context.mine_blocks(1)[0].txdata[0].compute_txid();

    assert_eq!(
      context
        .index
        .list(OutPoint::new(coinbase_txid, 0))
        .unwrap()
        .unwrap(),
      &[
        (15000000000, 20000000000),
        (9999999990, 10000000000),
        (14999999990, 15000000000)
      ],
    );
  }

  #[test]
  fn list_null_output() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let no_value_output = TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      fee: 50 * COIN_VALUE,
      ..default()
    };
    let txid = context.core.broadcast_tx(no_value_output);
    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      &[],
    );
  }

  #[test]
  fn list_null_input() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let no_value_output = TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      fee: 50 * COIN_VALUE,
      ..default()
    };
    context.core.broadcast_tx(no_value_output);
    context.mine_blocks(1);

    let no_value_input = TransactionTemplate {
      inputs: &[(2, 1, 0, Default::default())],
      fee: 0,
      ..default()
    };
    let txid = context.core.broadcast_tx(no_value_input);
    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      &[],
    );
  }

  #[test]
  fn list_spent_output() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks(1);
    context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      fee: 0,
      ..default()
    });
    context.mine_blocks(1);
    let txid = context.core.tx(1, 0).compute_txid();
    assert_matches!(context.index.list(OutPoint::new(txid, 0)).unwrap(), None);
  }

  #[test]
  fn list_unknown_output() {
    let context = Context::builder().arg("--index-sats").build();

    assert_eq!(
      context
        .index
        .list(
          "0000000000000000000000000000000000000000000000000000000000000000:0"
            .parse()
            .unwrap()
        )
        .unwrap(),
      None
    );
  }

  #[test]
  fn find_first_sat() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context.index.find(Sat(0)).unwrap().unwrap(),
      SatPoint {
        outpoint: "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b:0"
          .parse()
          .unwrap(),
        offset: 0,
      }
    )
  }

  #[test]
  fn find_second_sat() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context.index.find(Sat(1)).unwrap().unwrap(),
      SatPoint {
        outpoint: "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b:0"
          .parse()
          .unwrap(),
        offset: 1,
      }
    )
  }

  #[test]
  fn find_first_sat_of_second_block() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks(1);
    let tx = context.core.tx(1, 0);
    assert_eq!(
      context.index.find(Sat(50 * COIN_VALUE)).unwrap().unwrap(),
      SatPoint {
        outpoint: OutPoint {
          txid: tx.compute_txid(),
          vout: 0,
        },
        offset: 0,
      }
    )
  }

  #[test]
  fn find_unmined_sat() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(context.index.find(Sat(50 * COIN_VALUE)).unwrap(), None);
  }

  #[test]
  fn find_first_sat_spent_in_second_block() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks(1);
    let spend_txid = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      fee: 0,
      ..default()
    });
    context.mine_blocks(1);
    assert_eq!(
      context.index.find(Sat(50 * COIN_VALUE)).unwrap().unwrap(),
      SatPoint {
        outpoint: OutPoint::new(spend_txid, 0),
        offset: 0,
      }
    )
  }

  #[test]
  fn inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscriptions_without_sats_are_unbound() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, Default::default())],
        fee: 50 * 100_000_000,
        ..default()
      });

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: unbound_outpoint(),
          offset: 0,
        },
        None,
      );

      context.mine_blocks(1);

      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(4, 0, 0, Default::default())],
        fee: 50 * 100_000_000,
        ..default()
      });

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(5, 1, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: unbound_outpoint(),
          offset: 1,
        },
        None,
      );
    }
  }

  #[test]
  fn unaligned_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      let send_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, Default::default()), (2, 1, 0, Default::default())],
        ..default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: send_txid,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn merged_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(2);

      let first_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let first_inscription_id = InscriptionId {
        txid: first_txid,
        index: 0,
      };

      let second_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, inscription("text/png", [1; 100]).to_witness())],
        ..default()
      });
      let second_inscription_id = InscriptionId {
        txid: second_txid,
        index: 0,
      };

      context.mine_blocks(1);

      let merged_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 0, Default::default()), (3, 2, 0, Default::default())],
        ..default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        first_inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: merged_txid,
            vout: 0,
          },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second_inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: merged_txid,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(100 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscriptions_that_are_sent_to_second_output_are_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      let send_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, Default::default()), (2, 1, 0, Default::default())],
        outputs: 2,
        ..default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: send_txid,
            vout: 1,
          },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn one_input_fee_spent_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, Default::default())],
        fee: 50 * COIN_VALUE,
        ..default()
      });

      let coinbase_tx = context.mine_blocks(1)[0].txdata[0].compute_txid();

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: coinbase_tx,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn two_input_fee_spent_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(2);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, Default::default()), (3, 1, 0, Default::default())],
        fee: 50 * COIN_VALUE,
        ..default()
      });

      let coinbase_tx = context.mine_blocks(1)[0].txdata[0].compute_txid();

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: coinbase_tx,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscription_can_be_fee_spent_in_first_transaction() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      let coinbase_tx = context.mine_blocks(1)[0].txdata[0].compute_txid();

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: coinbase_tx,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn lost_inscriptions() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn multiple_inscriptions_can_be_lost() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let first_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });
      let first_inscription_id = InscriptionId {
        txid: first_txid,
        index: 0,
      };

      context.mine_blocks_with_subsidy(1, 0);
      context.mine_blocks(1);

      let second_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 0, 0, inscription("text/plain", "hello").to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });
      let second_inscription_id = InscriptionId {
        txid: second_txid,
        index: 0,
      };

      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        first_inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second_inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 50 * COIN_VALUE,
        },
        Some(150 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn lost_sats_are_tracked_correctly() {
    let context = Context::builder().args(["--index-sats"]).build();
    assert_eq!(context.index.statistic(Statistic::LostSats), 0);

    context.mine_blocks(1);
    assert_eq!(context.index.statistic(Statistic::LostSats), 0);

    context.mine_blocks_with_subsidy(1, 0);
    assert_eq!(
      context.index.statistic(Statistic::LostSats),
      50 * COIN_VALUE
    );

    context.mine_blocks_with_subsidy(1, 0);
    assert_eq!(
      context.index.statistic(Statistic::LostSats),
      100 * COIN_VALUE
    );

    context.mine_blocks(1);
    assert_eq!(
      context.index.statistic(Statistic::LostSats),
      100 * COIN_VALUE
    );
  }

  #[test]
  fn lost_sat_ranges_are_tracked_correctly() {
    let context = Context::builder().args(["--index-sats"]).build();

    let null_ranges = || {
      context
        .index
        .list(OutPoint::null())
        .unwrap()
        .unwrap_or_default()
    };

    assert!(null_ranges().is_empty());

    context.mine_blocks(1);

    assert!(null_ranges().is_empty());

    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(null_ranges(), [(100 * COIN_VALUE, 150 * COIN_VALUE)]);

    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      null_ranges(),
      [
        (100 * COIN_VALUE, 150 * COIN_VALUE),
        (150 * COIN_VALUE, 200 * COIN_VALUE)
      ]
    );

    context.mine_blocks(1);

    assert_eq!(
      null_ranges(),
      [
        (100 * COIN_VALUE, 150 * COIN_VALUE),
        (150 * COIN_VALUE, 200 * COIN_VALUE)
      ]
    );

    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      null_ranges(),
      [
        (100 * COIN_VALUE, 150 * COIN_VALUE),
        (150 * COIN_VALUE, 200 * COIN_VALUE),
        (250 * COIN_VALUE, 300 * COIN_VALUE)
      ]
    );
  }

  #[test]
  fn lost_inscriptions_get_lost_satpoints() {
    for context in Context::configurations() {
      context.mine_blocks_with_subsidy(1, 0);
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, inscription("text/plain", "hello").to_witness())],
        outputs: 2,
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks(1);

      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 1, Default::default()), (3, 1, 0, Default::default())],
        fee: 50 * COIN_VALUE,
        ..default()
      });
      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 75 * COIN_VALUE,
        },
        Some(100 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscription_skips_zero_value_first_output_of_inscribe_transaction() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        outputs: 2,
        output_values: &[0, 50 * COIN_VALUE],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 1 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscription_can_be_lost_in_first_transaction() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };
      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn lost_rare_sats_are_tracked() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks_with_subsidy(1, 0);
    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      context
        .index
        .rare_sat_satpoint(Sat(50 * COIN_VALUE))
        .unwrap()
        .unwrap(),
      SatPoint {
        outpoint: OutPoint::null(),
        offset: 0,
      },
    );

    assert_eq!(
      context
        .index
        .rare_sat_satpoint(Sat(100 * COIN_VALUE))
        .unwrap()
        .unwrap(),
      SatPoint {
        outpoint: OutPoint::null(),
        offset: 50 * COIN_VALUE,
      },
    );
  }

  #[test]
  fn old_schema_gives_correct_error() {
    let tempdir = {
      let context = Context::builder().build();

      let wtx = context.index.database.begin_write().unwrap();

      wtx
        .open_table(STATISTIC_TO_COUNT)
        .unwrap()
        .insert(&Statistic::Schema.key(), &0)
        .unwrap();

      wtx.commit().unwrap();

      context.tempdir
    };

    let path = tempdir.path().to_owned();

    let delimiter = if cfg!(windows) { '\\' } else { '/' };

    assert_eq!(
      Context::builder().tempdir(tempdir).try_build().err().unwrap().to_string(),
      format!("index at `{}{delimiter}regtest{delimiter}index.redb` appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index: index schema 0, ord schema {SCHEMA_VERSION}", path.display()));
  }

  #[test]
  fn new_schema_gives_correct_error() {
    let tempdir = {
      let context = Context::builder().build();

      let wtx = context.index.database.begin_write().unwrap();

      wtx
        .open_table(STATISTIC_TO_COUNT)
        .unwrap()
        .insert(&Statistic::Schema.key(), &u64::MAX)
        .unwrap();

      wtx.commit().unwrap();

      context.tempdir
    };

    let path = tempdir.path().to_owned();

    let delimiter = if cfg!(windows) { '\\' } else { '/' };

    assert_eq!(
      Context::builder().tempdir(tempdir).try_build().err().unwrap().to_string(),
      format!("index at `{}{delimiter}regtest{delimiter}index.redb` appears to have been built with a newer, incompatible version of ord, consider updating ord: index schema {}, ord schema {SCHEMA_VERSION}", path.display(), u64::MAX));
  }

  #[test]
  fn inscriptions_on_output() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context
          .index
          .get_inscriptions_for_output(OutPoint { txid, vout: 0 })
          .unwrap()
          .unwrap_or_default(),
        []
      );

      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscriptions_for_output(OutPoint { txid, vout: 0 })
          .unwrap()
          .unwrap_or_default(),
        [inscription_id]
      );

      let send_id = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, Default::default())],
        ..default()
      });

      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscriptions_for_output(OutPoint { txid, vout: 0 })
          .unwrap()
          .unwrap_or_default(),
        []
      );

      assert_eq!(
        context
          .index
          .get_inscriptions_for_output(OutPoint {
            txid: send_id,
            vout: 0,
          })
          .unwrap()
          .unwrap_or_default(),
        [inscription_id]
      );
    }
  }

  #[test]
  fn inscriptions_on_same_sat_after_the_first_are_not_unbound() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let first = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId {
        txid: first,
        index: 0,
      };

      assert_eq!(
        context
          .index
          .get_inscriptions_for_output(OutPoint {
            txid: first,
            vout: 0
          })
          .unwrap()
          .unwrap_or_default(),
        [inscription_id]
      );

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: first,
            vout: 0,
          },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      let second = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let inscription_id = InscriptionId {
        txid: second,
        index: 0,
      };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: second,
            vout: 0,
          },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      assert!(context
        .index
        .get_inscription_by_id(InscriptionId {
          txid: second,
          index: 0
        })
        .unwrap()
        .is_some());

      assert!(context
        .index
        .get_inscription_by_id(InscriptionId {
          txid: second,
          index: 0
        })
        .unwrap()
        .is_some());
    }
  }

  #[test]
  fn get_latest_inscriptions_with_no_more() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      let (inscriptions, more) = context.index.get_inscriptions_paginated(100, 0).unwrap();
      assert_eq!(inscriptions, &[inscription_id]);
      assert!(!more);
    }
  }

  #[test]
  fn get_latest_inscriptions_with_more() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let mut ids = Vec::new();

      for i in 0..101 {
        let txid = context.core.broadcast_tx(TransactionTemplate {
          inputs: &[(i + 1, 0, 0, inscription("text/plain", "hello").to_witness())],
          ..default()
        });
        context.mine_blocks(1);
        ids.push(InscriptionId { txid, index: 0 });
      }

      ids.reverse();
      ids.pop();

      assert_eq!(ids.len(), 100);

      let (inscriptions, more) = context.index.get_inscriptions_paginated(100, 0).unwrap();
      assert_eq!(inscriptions, ids);
      assert!(more);
    }
  }

  #[test]
  fn unrecognized_even_field_inscriptions_are_cursed_and_unbound() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[2],
        b"bar",
        &[4],
        b"ord",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: unbound_outpoint(),
          offset: 0,
        },
        None,
      );

      assert_eq!(context.index.inscription_number(inscription_id), -1);
    }
  }

  #[test]
  fn unrecognized_even_field_inscriptions_are_unbound_after_jubilee() {
    for context in Context::configurations() {
      context.mine_blocks(109);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[2],
        b"bar",
        &[4],
        b"ord",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: unbound_outpoint(),
          offset: 0,
        },
        None,
      );

      assert_eq!(context.index.inscription_number(inscription_id), 0);
    }
  }

  #[test]
  fn inscriptions_are_uncursed_after_jubilee() {
    for context in Context::configurations() {
      context.mine_blocks(108);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[1],
        b"text/plain;charset=utf-8",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness.clone())],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.core.height(), 109);

      assert_eq!(context.index.inscription_number(inscription_id), -1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.core.height(), 110);

      assert_eq!(context.index.inscription_number(inscription_id), 0);
    }
  }

  #[test]
  fn duplicate_field_inscriptions_are_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[1],
        b"text/plain;charset=utf-8",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.index.inscription_number(inscription_id), -1);
    }
  }

  #[test]
  fn incomplete_field_inscriptions_are_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let witness = envelope(&[b"ord", &[1]]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.index.inscription_number(inscription_id), -1);
    }
  }

  #[test]
  fn inscriptions_with_pushnum_opcodes_are_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([])
        .push_opcode(opcodes::all::OP_PUSHNUM_1)
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.index.inscription_number(inscription_id), -1);
    }
  }

  #[test]
  fn inscriptions_with_stutter_are_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([])
        .push_opcode(opcodes::all::OP_PUSHNUM_1)
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.index.inscription_number(inscription_id), -1);
    }
  }

  // https://github.com/ordinals/ord/issues/2062
  #[test]
  fn zero_value_transaction_inscription_not_cursed_but_unbound() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, Default::default())],
        fee: 50 * 100_000_000,
        ..default()
      });

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: unbound_outpoint(),
          offset: 0,
        },
        None,
      );

      assert_eq!(context.index.inscription_number(inscription_id), 0);
    }
  }

  #[test]
  fn transaction_with_inscription_inside_zero_value_2nd_input_should_be_unbound_and_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      // create zero value input
      context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, Default::default())],
        fee: 50 * 100_000_000,
        ..default()
      });

      context.mine_blocks(1);

      let witness = inscription("text/plain", "hello").to_witness();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, witness.clone()), (2, 1, 0, witness.clone())],
        ..default()
      });

      let second_inscription_id = InscriptionId { txid, index: 1 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        second_inscription_id,
        SatPoint {
          outpoint: unbound_outpoint(),
          offset: 0,
        },
        None,
      );

      assert_eq!(context.index.inscription_number(second_inscription_id), -1);
    }
  }

  #[test]
  fn multiple_inscriptions_in_same_tx_all_but_first_input_are_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);
      context.mine_blocks(1);
      context.mine_blocks(1);

      let witness = envelope(&[b"ord", &[1], b"text/plain;charset=utf-8", &[], b"bar"]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (1, 0, 0, witness.clone()),
          (2, 0, 0, witness.clone()),
          (3, 0, 0, witness.clone()),
        ],
        ..default()
      });

      let first = InscriptionId { txid, index: 0 };
      let second = InscriptionId { txid, index: 1 };
      let third = InscriptionId { txid, index: 2 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        first,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 50 * COIN_VALUE,
        },
        Some(100 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        third,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 100 * COIN_VALUE,
        },
        Some(150 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(first), 0);
      assert_eq!(context.index.inscription_number(second), -1);
      assert_eq!(context.index.inscription_number(third), -2);
    }
  }

  #[test]
  fn multiple_inscriptions_same_input_are_cursed_reinscriptions() {
    for context in Context::configurations() {
      context.core.mine_blocks(1);

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"foo")
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"bar")
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"qix")
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let first = InscriptionId { txid, index: 0 };
      let second = InscriptionId { txid, index: 1 };
      let third = InscriptionId { txid, index: 2 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        first,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        third,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(first), 0);
      assert_eq!(context.index.inscription_number(second), -1);
      assert_eq!(context.index.inscription_number(third), -2);
    }
  }

  #[test]
  fn multiple_inscriptions_different_inputs_and_same_inputs() {
    for context in Context::configurations() {
      context.core.mine_blocks(1);
      context.core.mine_blocks(1);
      context.core.mine_blocks(1);

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"foo")
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"bar")
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"qix")
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (1, 0, 0, witness.clone()),
          (2, 0, 0, witness.clone()),
          (3, 0, 0, witness.clone()),
        ],
        ..default()
      });

      let first = InscriptionId { txid, index: 0 }; // normal
      let second = InscriptionId { txid, index: 1 }; // cursed reinscription
      let fourth = InscriptionId { txid, index: 3 }; // cursed but bound
      let ninth = InscriptionId { txid, index: 8 }; // cursed reinscription

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        first,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        fourth,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 50 * COIN_VALUE,
        },
        Some(100 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        ninth,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 100 * COIN_VALUE,
        },
        Some(150 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(first), 0);

      assert_eq!(
        context
          .index
          .get_inscription_id_by_inscription_number(-3)
          .unwrap()
          .unwrap(),
        fourth
      );

      assert_eq!(context.index.inscription_number(fourth), -3);

      assert_eq!(context.index.inscription_number(ninth), -8);
    }
  }

  #[test]
  fn inscription_fee_distributed_evenly() {
    for context in Context::configurations() {
      context.core.mine_blocks(1);

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"foo")
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"bar")
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([1])
        .push_slice(b"text/plain;charset=utf-8")
        .push_slice([])
        .push_slice(b"qix")
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        fee: 33,
        ..default()
      });

      let first = InscriptionId { txid, index: 0 };
      let second = InscriptionId { txid, index: 1 };

      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscription_entry(first)
          .unwrap()
          .unwrap()
          .fee,
        11
      );

      assert_eq!(
        context
          .index
          .get_inscription_entry(second)
          .unwrap()
          .unwrap()
          .fee,
        11
      );
    }
  }

  #[test]
  fn reinscription_on_cursed_inscription_is_not_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);
      context.mine_blocks(1);

      let witness = envelope(&[b"ord", &[1], b"text/plain;charset=utf-8", &[], b"bar"]);

      let cursed_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness.clone()), (2, 0, 0, witness.clone())],
        outputs: 2,
        ..default()
      });

      let cursed = InscriptionId {
        txid: cursed_txid,
        index: 1,
      };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        cursed,
        SatPoint {
          outpoint: OutPoint {
            txid: cursed_txid,
            vout: 1,
          },
          offset: 0,
        },
        Some(100 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(cursed), -1);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[],
        b"reinscription on cursed",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 1, witness)],
        ..default()
      });

      let reinscription_on_cursed = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        reinscription_on_cursed,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(100 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(reinscription_on_cursed), 1);
    }
  }

  #[test]
  fn second_reinscription_on_cursed_inscription_is_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);
      context.mine_blocks(1);

      let witness = envelope(&[b"ord", &[1], b"text/plain;charset=utf-8", &[], b"bar"]);

      let cursed_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness.clone()), (2, 0, 0, witness.clone())],
        outputs: 2,
        ..default()
      });

      let cursed = InscriptionId {
        txid: cursed_txid,
        index: 1,
      };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        cursed,
        SatPoint {
          outpoint: OutPoint {
            txid: cursed_txid,
            vout: 1,
          },
          offset: 0,
        },
        Some(100 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(cursed), -1);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[],
        b"reinscription on cursed",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 1, witness)],
        ..default()
      });

      let reinscription_on_cursed = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        reinscription_on_cursed,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(100 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(reinscription_on_cursed), 1);

      let witness = envelope(&[
        b"ord",
        &[1],
        b"text/plain;charset=utf-8",
        &[],
        b"second reinscription on cursed",
      ]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(4, 1, 0, witness)],
        ..default()
      });

      let second_reinscription_on_cursed = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        second_reinscription_on_cursed,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(100 * COIN_VALUE),
      );

      assert_eq!(
        context
          .index
          .inscription_number(second_reinscription_on_cursed),
        -2
      );

      assert_eq!(
        vec![
          cursed,
          reinscription_on_cursed,
          second_reinscription_on_cursed
        ],
        context
          .index
          .get_inscriptions_on_output_with_satpoints(OutPoint { txid, vout: 0 })
          .unwrap()
          .unwrap_or_default()
          .iter()
          .map(|(_satpoint, inscription_id)| *inscription_id)
          .collect::<Vec<InscriptionId>>()
      )
    }
  }

  #[test]
  fn reinscriptions_on_output_correctly_ordered_and_transferred() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          1,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });

      let first = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          1,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });

      let second = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);
      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          3,
          1,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });

      let third = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      let location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      assert_eq!(
        vec![(location, first), (location, second), (location, third)],
        context
          .index
          .get_inscriptions_on_output_with_satpoints(OutPoint { txid, vout: 0 })
          .unwrap()
          .unwrap_or_default()
      )
    }
  }

  #[test]
  fn reinscriptions_are_ordered_correctly_for_many_outpoints() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let mut inscription_ids = Vec::new();
      for i in 1..=21 {
        let txid = context.core.broadcast_tx(TransactionTemplate {
          inputs: &[(
            i,
            if i == 1 { 0 } else { 1 },
            0,
            inscription("text/plain;charset=utf-8", format!("hello {i}")).to_witness(),
          )], // for the first inscription use coinbase, otherwise use the previous tx
          ..default()
        });

        inscription_ids.push(InscriptionId { txid, index: 0 });

        context.mine_blocks(1);
      }

      let final_txid = inscription_ids.last().unwrap().txid;
      let location = SatPoint {
        outpoint: OutPoint {
          txid: final_txid,
          vout: 0,
        },
        offset: 0,
      };

      let expected_result = inscription_ids
        .iter()
        .map(|id| (location, *id))
        .collect::<Vec<(SatPoint, InscriptionId)>>();

      assert_eq!(
        expected_result,
        context
          .index
          .get_inscriptions_on_output_with_satpoints(OutPoint {
            txid: final_txid,
            vout: 0
          })
          .unwrap()
          .unwrap_or_default()
      )
    }
  }

  #[test]
  fn recover_from_reorg() {
    for mut context in Context::configurations() {
      context.index.set_durability(redb::Durability::Immediate);

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          1,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });
      let first_id = InscriptionId { txid, index: 0 };
      let first_location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      context.mine_blocks(6);

      context
        .index
        .assert_inscription_location(first_id, first_location, Some(50 * COIN_VALUE));

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });
      let second_id = InscriptionId { txid, index: 0 };
      let second_location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      context.mine_blocks(1);

      context
        .index
        .assert_inscription_location(second_id, second_location, Some(100 * COIN_VALUE));

      context.core.invalidate_tip();
      context.mine_blocks(2);

      context
        .index
        .assert_inscription_location(first_id, first_location, Some(50 * COIN_VALUE));

      assert!(!context.index.inscription_exists(second_id).unwrap());
    }
  }

  #[test]
  fn recover_from_3_block_deep_and_consecutive_reorg() {
    for mut context in Context::configurations() {
      context.index.set_durability(redb::Durability::Immediate);

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          1,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });
      let first_id = InscriptionId { txid, index: 0 };
      let first_location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      context.mine_blocks(10);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });
      let second_id = InscriptionId { txid, index: 0 };
      let second_location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      context.mine_blocks(1);

      context
        .index
        .assert_inscription_location(second_id, second_location, Some(100 * COIN_VALUE));

      context.core.invalidate_tip();
      context.core.invalidate_tip();
      context.core.invalidate_tip();

      context.mine_blocks(4);

      assert!(!context.index.inscription_exists(second_id).unwrap());

      context.core.invalidate_tip();

      context.mine_blocks(2);

      context
        .index
        .assert_inscription_location(first_id, first_location, Some(50 * COIN_VALUE));
    }
  }

  #[test]
  fn recover_from_very_unlikely_7_block_deep_reorg() {
    for mut context in Context::configurations() {
      context.index.set_durability(redb::Durability::Immediate);

      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          1,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });

      context.mine_blocks(11);

      let first_id = InscriptionId { txid, index: 0 };
      let first_location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          0,
          0,
          inscription("text/plain;charset=utf-8", "hello").to_witness(),
        )],
        ..default()
      });

      let second_id = InscriptionId { txid, index: 0 };
      let second_location = SatPoint {
        outpoint: OutPoint { txid, vout: 0 },
        offset: 0,
      };

      context.mine_blocks(7);

      context
        .index
        .assert_inscription_location(second_id, second_location, Some(100 * COIN_VALUE));

      for _ in 0..7 {
        context.core.invalidate_tip();
      }

      context.mine_blocks(9);

      assert!(!context.index.inscription_exists(second_id).unwrap());

      context
        .index
        .assert_inscription_location(first_id, first_location, Some(50 * COIN_VALUE));
    }
  }

  #[test]
  fn inscription_without_parent_tag_has_no_parent_entry() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert!(context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap()
        .parents
        .is_empty());
    }
  }

  #[test]
  fn inscription_with_parent_tag_without_parent_has_no_parent_entry() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          0,
          0,
          Inscription {
            content_type: Some("text/plain".into()),
            body: Some("hello".into()),
            parents: vec![parent_inscription_id.value()],
            ..default()
          }
          .to_witness(),
        )],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert!(context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap()
        .parents
        .is_empty());
    }
  }

  #[test]
  fn inscription_with_parent_tag_and_parent_has_parent_entry() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          1,
          0,
          Inscription {
            content_type: Some("text/plain".into()),
            body: Some("hello".into()),
            parents: vec![parent_inscription_id.value()],
            ..default()
          }
          .to_witness(),
        )],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn inscription_with_two_parent_tags_and_parents_has_parent_entries() {
    for context in Context::configurations() {
      context.mine_blocks(2);

      let parent_txid_a = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let parent_txid_b = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, inscription("text/plain", "world").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id_a = InscriptionId {
        txid: parent_txid_a,
        index: 0,
      };
      let parent_inscription_id_b = InscriptionId {
        txid: parent_txid_b,
        index: 0,
      };

      let multi_parent_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello".into()),
        parents: vec![
          parent_inscription_id_a.value(),
          parent_inscription_id_b.value(),
        ],
        ..default()
      };
      let multi_parent_witness = multi_parent_inscription.to_witness();

      let revelation_input = (3, 1, 0, multi_parent_witness);

      let parent_b_input = (3, 2, 0, Witness::new());

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[revelation_input, parent_b_input],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id_a, parent_inscription_id_b]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_a)
          .unwrap(),
        vec![inscription_id]
      );
      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_b)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn inscription_with_repeated_parent_tags_and_parents_has_singular_parent_entry() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          1,
          0,
          Inscription {
            content_type: Some("text/plain".into()),
            body: Some("hello".into()),
            parents: vec![parent_inscription_id.value(), parent_inscription_id.value()],
            ..default()
          }
          .to_witness(),
        )],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn inscription_with_distinct_parent_tag_encodings_for_same_parent_has_singular_parent_entry() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let trailing_zero_inscription_id: Vec<u8> = parent_inscription_id
        .value()
        .into_iter()
        .chain(vec![0, 0, 0, 0])
        .collect();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          1,
          0,
          Inscription {
            content_type: Some("text/plain".into()),
            body: Some("hello".into()),
            parents: vec![parent_inscription_id.value(), trailing_zero_inscription_id],
            ..default()
          }
          .to_witness(),
        )],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn inscription_with_three_parent_tags_and_two_parents_has_two_parent_entries() {
    for context in Context::configurations() {
      context.mine_blocks(3);

      let parent_txid_a = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let parent_txid_b = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, inscription("text/plain", "world").to_witness())],
        ..default()
      });
      let parent_txid_c = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 0, 0, inscription("text/plain", "wazzup").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id_a = InscriptionId {
        txid: parent_txid_a,
        index: 0,
      };
      let parent_inscription_id_b = InscriptionId {
        txid: parent_txid_b,
        index: 0,
      };
      let parent_inscription_id_c = InscriptionId {
        txid: parent_txid_c,
        index: 0,
      };

      let multi_parent_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello".into()),
        parents: vec![
          parent_inscription_id_a.value(),
          parent_inscription_id_b.value(),
          parent_inscription_id_c.value(),
        ],
        ..default()
      };
      let multi_parent_witness = multi_parent_inscription.to_witness();

      let revealing_parent_a_input = (4, 1, 0, multi_parent_witness);

      let parent_c_input = (4, 3, 0, Witness::new());

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[revealing_parent_a_input, parent_c_input],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id_a, parent_inscription_id_c]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_a)
          .unwrap(),
        vec![inscription_id]
      );
      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_b)
          .unwrap(),
        Vec::new()
      );
      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_c)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn inscription_with_valid_and_malformed_parent_tags_only_lists_valid_entries() {
    for context in Context::configurations() {
      context.mine_blocks(3);

      let parent_txid_a = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });
      let parent_txid_b = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, inscription("text/plain", "world").to_witness())],
        ..default()
      });
      let parent_txid_c = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 0, 0, inscription("text/plain", "wazzup").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id_a = InscriptionId {
        txid: parent_txid_a,
        index: 0,
      };
      let parent_inscription_id_b = InscriptionId {
        txid: parent_txid_b,
        index: 0,
      };
      let parent_inscription_id_c = InscriptionId {
        txid: parent_txid_c,
        index: 0,
      };

      let malformed_inscription_id_b = parent_inscription_id_b
        .value()
        .into_iter()
        .chain(iter::once(0))
        .collect();

      let multi_parent_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello".into()),
        parents: vec![
          parent_inscription_id_a.value(),
          malformed_inscription_id_b,
          parent_inscription_id_c.value(),
        ],
        ..default()
      };
      let multi_parent_witness = multi_parent_inscription.to_witness();

      let revealing_parent_a_input = (4, 1, 0, multi_parent_witness);
      let parent_b_input = (4, 2, 0, Witness::new());
      let parent_c_input = (4, 3, 0, Witness::new());

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[revealing_parent_a_input, parent_b_input, parent_c_input],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id_a, parent_inscription_id_c]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_a)
          .unwrap(),
        vec![inscription_id]
      );
      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_b)
          .unwrap(),
        Vec::new()
      );
      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id_c)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn parents_can_be_in_preceding_input() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(2);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (2, 1, 0, Default::default()),
          (
            3,
            0,
            0,
            Inscription {
              content_type: Some("text/plain".into()),
              body: Some("hello".into()),
              parents: vec![parent_inscription_id.value()],
              ..default()
            }
            .to_witness(),
          ),
        ],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn parents_can_be_in_following_input() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(2);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (
            3,
            0,
            0,
            Inscription {
              content_type: Some("text/plain".into()),
              body: Some("hello".into()),
              parents: vec![parent_inscription_id.value()],
              ..default()
            }
            .to_witness(),
          ),
          (2, 1, 0, Default::default()),
        ],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert_eq!(
        context.index.get_parents_by_inscription_id(inscription_id),
        vec![parent_inscription_id]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id)
          .unwrap(),
        vec![inscription_id]
      );
    }
  }

  #[test]
  fn inscription_with_invalid_parent_tag_and_parent_has_no_parent_entry() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(
          2,
          1,
          0,
          Inscription {
            content_type: Some("text/plain".into()),
            body: Some("hello".into()),
            parents: vec![parent_inscription_id
              .value()
              .into_iter()
              .chain(iter::once(0))
              .collect()],
            ..default()
          }
          .to_witness(),
        )],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      assert!(context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap()
        .parents
        .is_empty());
    }
  }

  #[test]
  fn inscription_with_pointer() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello".into()),
        pointer: Some(100u64.to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 100,
        },
        Some(50 * COIN_VALUE + 100),
      );
    }
  }

  #[test]
  fn inscription_with_pointer_greater_than_output_value_assigned_default() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello".into()),
        pointer: Some((50 * COIN_VALUE).to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscription_with_pointer_into_fee_ignored_and_assigned_default_location() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello".into()),
        pointer: Some((25 * COIN_VALUE).to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription.to_witness())],
        fee: 25 * COIN_VALUE,
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscription_with_pointer_is_cursed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("pointer-child".into()),
        pointer: Some(0u64.to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(inscription_id), -1);
    }
  }

  #[test]
  fn inscription_with_pointer_to_parent_is_cursed_reinscription() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let parent_txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "parent").to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let parent_inscription_id = InscriptionId {
        txid: parent_txid,
        index: 0,
      };

      let child_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("pointer-child".into()),
        parents: vec![parent_inscription_id.value()],
        pointer: Some(0u64.to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, child_inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let child_inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        parent_inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        child_inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(child_inscription_id), -1);

      assert_eq!(
        context
          .index
          .get_parents_by_inscription_id(child_inscription_id),
        vec![parent_inscription_id]
      );

      assert_eq!(
        context
          .index
          .get_children_by_inscription_id(parent_inscription_id)
          .unwrap(),
        vec![child_inscription_id]
      );
    }
  }

  #[test]
  fn inscriptions_in_same_input_with_pointers_to_same_output() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let builder = script::Builder::new();

      let builder = Inscription {
        pointer: Some(100u64.to_le_bytes().to_vec()),
        ..default()
      }
      .append_reveal_script_to_builder(builder);

      let builder = Inscription {
        pointer: Some(300_000u64.to_le_bytes().to_vec()),
        ..default()
      }
      .append_reveal_script_to_builder(builder);

      let builder = Inscription {
        pointer: Some(1_000_000u64.to_le_bytes().to_vec()),
        ..default()
      }
      .append_reveal_script_to_builder(builder);

      let witness = Witness::from_slice(&[builder.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      context.mine_blocks(1);

      let first = InscriptionId { txid, index: 0 };
      let second = InscriptionId { txid, index: 1 };
      let third = InscriptionId { txid, index: 2 };

      context.index.assert_inscription_location(
        first,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 100,
        },
        Some(50 * COIN_VALUE + 100),
      );

      context.index.assert_inscription_location(
        second,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 300_000,
        },
        Some(50 * COIN_VALUE + 300_000),
      );

      context.index.assert_inscription_location(
        third,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 1_000_000,
        },
        Some(50 * COIN_VALUE + 1_000_000),
      );
    }
  }

  #[test]
  fn inscriptions_in_same_input_with_pointers_to_different_outputs() {
    for context in Context::configurations() {
      context.mine_blocks_with_subsidy(1, 300_000);

      let builder = script::Builder::new();

      let builder = Inscription {
        pointer: Some(100u64.to_le_bytes().to_vec()),
        ..default()
      }
      .append_reveal_script_to_builder(builder);

      let builder = Inscription {
        pointer: Some(100_111u64.to_le_bytes().to_vec()),
        ..default()
      }
      .append_reveal_script_to_builder(builder);

      let builder = Inscription {
        pointer: Some(299_999u64.to_le_bytes().to_vec()),
        ..default()
      }
      .append_reveal_script_to_builder(builder);

      let witness = Witness::from_slice(&[builder.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        outputs: 3,
        ..default()
      });

      context.mine_blocks(1);

      let first = InscriptionId { txid, index: 0 };
      let second = InscriptionId { txid, index: 1 };
      let third = InscriptionId { txid, index: 2 };

      context.index.assert_inscription_location(
        first,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 100,
        },
        Some(50 * COIN_VALUE + 100),
      );

      context.index.assert_inscription_location(
        second,
        SatPoint {
          outpoint: OutPoint { txid, vout: 1 },
          offset: 111,
        },
        Some(50 * COIN_VALUE + 100_111),
      );

      context.index.assert_inscription_location(
        third,
        SatPoint {
          outpoint: OutPoint { txid, vout: 2 },
          offset: 99_999,
        },
        Some(50 * COIN_VALUE + 299_999),
      );
    }
  }

  #[test]
  fn inscriptions_in_different_inputs_with_pointers_to_different_outputs() {
    for context in Context::configurations() {
      context.mine_blocks(3);

      let inscription_for_second_output = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello jupiter".into()),
        pointer: Some((50 * COIN_VALUE).to_le_bytes().to_vec()),
        ..default()
      };

      let inscription_for_third_output = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello mars".into()),
        pointer: Some((100 * COIN_VALUE).to_le_bytes().to_vec()),
        ..default()
      };

      let inscription_for_first_output = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello world".into()),
        pointer: Some(0u64.to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (1, 0, 0, inscription_for_second_output.to_witness()),
          (2, 0, 0, inscription_for_third_output.to_witness()),
          (3, 0, 0, inscription_for_first_output.to_witness()),
        ],
        outputs: 3,
        ..default()
      });

      context.mine_blocks(1);

      let inscription_for_second_output = InscriptionId { txid, index: 0 };
      let inscription_for_third_output = InscriptionId { txid, index: 1 };
      let inscription_for_first_output = InscriptionId { txid, index: 2 };

      context.index.assert_inscription_location(
        inscription_for_second_output,
        SatPoint {
          outpoint: OutPoint { txid, vout: 1 },
          offset: 0,
        },
        Some(100 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        inscription_for_third_output,
        SatPoint {
          outpoint: OutPoint { txid, vout: 2 },
          offset: 0,
        },
        Some(150 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        inscription_for_first_output,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscriptions_in_different_inputs_with_pointers_to_same_output() {
    for context in Context::configurations() {
      context.mine_blocks(3);

      let first_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello jupiter".into()),
        ..default()
      };

      let second_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello mars".into()),
        pointer: Some(1u64.to_le_bytes().to_vec()),
        ..default()
      };

      let third_inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello world".into()),
        pointer: Some(2u64.to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (1, 0, 0, first_inscription.to_witness()),
          (2, 0, 0, second_inscription.to_witness()),
          (3, 0, 0, third_inscription.to_witness()),
        ],
        outputs: 1,
        ..default()
      });

      context.mine_blocks(1);

      let first_inscription_id = InscriptionId { txid, index: 0 };
      let second_inscription_id = InscriptionId { txid, index: 1 };
      let third_inscription_id = InscriptionId { txid, index: 2 };

      context.index.assert_inscription_location(
        first_inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second_inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 1,
        },
        Some(50 * COIN_VALUE + 1),
      );

      context.index.assert_inscription_location(
        third_inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 2,
        },
        Some(50 * COIN_VALUE + 2),
      );
    }
  }

  #[test]
  fn inscriptions_with_pointers_to_same_sat_one_becomes_cursed_reinscriptions() {
    for context in Context::configurations() {
      context.mine_blocks(2);

      let inscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello jupiter".into()),
        ..default()
      };

      let cursed_reinscription = Inscription {
        content_type: Some("text/plain".into()),
        body: Some("hello mars".into()),
        pointer: Some(0u64.to_le_bytes().to_vec()),
        ..default()
      };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[
          (1, 0, 0, inscription.to_witness()),
          (2, 0, 0, cursed_reinscription.to_witness()),
        ],
        outputs: 2,
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };
      let cursed_reinscription_id = InscriptionId { txid, index: 1 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      context.index.assert_inscription_location(
        cursed_reinscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        Some(50 * COIN_VALUE),
      );

      assert_eq!(context.index.inscription_number(inscription_id), 0);

      assert_eq!(
        context.index.inscription_number(cursed_reinscription_id),
        -1
      );
    }
  }

  #[test]
  fn inscribe_into_fee() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let inscription = Inscription::default();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription.to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });

      let blocks = context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: blocks[0].txdata[0].compute_txid(),
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn inscribe_into_fee_with_reduced_subsidy() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let inscription = Inscription::default();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription.to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });

      let blocks = context.mine_blocks_with_subsidy(1, 25 * COIN_VALUE);

      let inscription_id = InscriptionId { txid, index: 0 };

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: blocks[0].txdata[0].compute_txid(),
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        Some(50 * COIN_VALUE),
      );
    }
  }

  #[test]
  fn pre_jubilee_first_reinscription_after_cursed_inscription_is_blessed() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      // Before the jubilee, an inscription on a sat using a pushnum opcode is
      // cursed and not vindicated.

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([])
        .push_opcode(opcodes::all::OP_PUSHNUM_1)
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      let entry = context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap();

      assert!(Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Cursed));

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Vindicated));

      let sat = entry.sat;

      assert_eq!(entry.inscription_number, -1);

      // Before the jubilee, reinscription on the same sat is not cursed and
      // not vindicated.

      let inscription = Inscription::default();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      let entry = context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap();

      assert_eq!(entry.inscription_number, 0);

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Cursed));

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Vindicated));

      assert_eq!(sat, entry.sat);

      // Before the jubilee, a third reinscription on the same sat is cursed
      // and not vindicated.

      let inscription = Inscription::default();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      let entry = context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap();

      assert!(Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Cursed));

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Vindicated));

      assert_eq!(entry.inscription_number, -2);

      assert_eq!(sat, entry.sat);
    }
  }

  #[test]
  fn post_jubilee_first_reinscription_after_vindicated_inscription_not_vindicated() {
    for context in Context::configurations() {
      context.mine_blocks(110);
      // After the jubilee, an inscription on a sat using a pushnum opcode is
      // vindicated and not cursed.

      let script = script::Builder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice(b"ord")
        .push_slice([])
        .push_opcode(opcodes::all::OP_PUSHNUM_1)
        .push_opcode(opcodes::all::OP_ENDIF)
        .into_script();

      let witness = Witness::from_slice(&[script.into_bytes(), Vec::new()]);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, witness)],
        ..default()
      });

      let inscription_id = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      let entry = context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap();

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Cursed));

      assert!(Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Vindicated));

      let sat = entry.sat;

      assert_eq!(entry.inscription_number, 0);

      // After the jubilee, a reinscription on the same is not cursed and not
      // vindicated.

      let inscription = Inscription::default();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(111, 1, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      let entry = context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap();

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Cursed));

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Vindicated));

      assert_eq!(entry.inscription_number, 1);

      assert_eq!(sat, entry.sat);

      // After the jubilee, a third reinscription on the same is vindicated and
      // not cursed.

      let inscription = Inscription::default();

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(112, 1, 0, inscription.to_witness())],
        ..default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId { txid, index: 0 };

      let entry = context
        .index
        .get_inscription_entry(inscription_id)
        .unwrap()
        .unwrap();

      assert!(!Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Cursed));

      assert!(Charm::charms(entry.charms)
        .iter()
        .any(|charm| *charm == Charm::Vindicated));

      assert_eq!(entry.inscription_number, 2);

      assert_eq!(sat, entry.sat);
    }
  }

  #[test]
  fn is_output_spent() {
    let context = Context::builder().build();

    assert!(!context.index.is_output_spent(OutPoint::null()).unwrap());
    assert!(!context
      .index
      .is_output_spent(Chain::Mainnet.genesis_coinbase_outpoint())
      .unwrap());

    context.mine_blocks(1);

    assert!(!context
      .index
      .is_output_spent(OutPoint {
        txid: context.core.tx(1, 0).compute_txid(),
        vout: 0,
      })
      .unwrap());

    context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0, Default::default())],
      ..default()
    });

    context.mine_blocks(1);

    assert!(context
      .index
      .is_output_spent(OutPoint {
        txid: context.core.tx(1, 0).compute_txid(),
        vout: 0,
      })
      .unwrap());
  }

  #[test]
  fn is_output_in_active_chain() {
    let context = Context::builder().build();

    assert!(context
      .index
      .is_output_in_active_chain(OutPoint::null())
      .unwrap());

    assert!(context
      .index
      .is_output_in_active_chain(Chain::Mainnet.genesis_coinbase_outpoint())
      .unwrap());

    context.mine_blocks(1);

    assert!(context
      .index
      .is_output_in_active_chain(OutPoint {
        txid: context.core.tx(1, 0).compute_txid(),
        vout: 0,
      })
      .unwrap());

    assert!(!context
      .index
      .is_output_in_active_chain(OutPoint {
        txid: context.core.tx(1, 0).compute_txid(),
        vout: 1,
      })
      .unwrap());

    assert!(!context
      .index
      .is_output_in_active_chain(OutPoint {
        txid: Txid::all_zeros(),
        vout: 0,
      })
      .unwrap());
  }

  #[test]
  fn output_addresses_are_updated() {
    let context = Context::builder()
      .arg("--index-addresses")
      .arg("--index-sats")
      .build();

    context.mine_blocks(2);

    let txid = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0, Witness::new()), (2, 0, 0, Witness::new())],
      outputs: 2,
      ..Default::default()
    });

    context.mine_blocks(1);

    let transaction = context.index.get_transaction(txid).unwrap().unwrap();

    let first_address = context
      .index
      .settings
      .chain()
      .address_from_script(&transaction.output[0].script_pubkey)
      .unwrap();

    let first_address_second_output = OutPoint {
      txid: transaction.compute_txid(),
      vout: 1,
    };

    assert_eq!(
      context.index.get_address_info(&first_address).unwrap(),
      [
        OutPoint {
          txid: transaction.compute_txid(),
          vout: 0
        },
        first_address_second_output
      ]
    );

    let txid = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(3, 1, 0, Witness::new())],
      p2tr: true,
      ..Default::default()
    });

    context.mine_blocks(1);

    let transaction = context.index.get_transaction(txid).unwrap().unwrap();

    let second_address = context
      .index
      .settings
      .chain()
      .address_from_script(&transaction.output[0].script_pubkey)
      .unwrap();

    assert_eq!(
      context.index.get_address_info(&first_address).unwrap(),
      [first_address_second_output]
    );

    assert_eq!(
      context.index.get_address_info(&second_address).unwrap(),
      [OutPoint {
        txid: transaction.compute_txid(),
        vout: 0
      }]
    );
  }

  #[test]
  fn fee_spent_inscriptions_are_numbered_last_in_block() {
    for context in Context::configurations() {
      context.mine_blocks(2);

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0, inscription("text/plain", "hello").to_witness())],
        fee: 50 * COIN_VALUE,
        ..default()
      });

      let a = InscriptionId { txid, index: 0 };

      let txid = context.core.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0, inscription("text/plain", "hello").to_witness())],
        ..default()
      });

      let b = InscriptionId { txid, index: 0 };

      context.mine_blocks(1);

      assert_eq!(context.index.inscription_number(a), 1);
      assert_eq!(context.index.inscription_number(b), 0);
    }
  }

  #[test]
  fn inscription_event_sender_channel() {
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(1024);
    let context = Context::builder().event_sender(event_sender).build();

    context.mine_blocks(1);

    let inscription = Inscription::default();
    let create_txid = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0, inscription.to_witness())],
      fee: 0,
      outputs: 1,
      ..default()
    });

    context.mine_blocks(1);

    let inscription_id = InscriptionId {
      txid: create_txid,
      index: 0,
    };
    let create_event = event_receiver.blocking_recv().unwrap();
    let expected_charms = if context.index.index_sats { 513 } else { 0 };
    assert_eq!(
      create_event,
      Event::InscriptionCreated {
        inscription_id,
        location: Some(SatPoint {
          outpoint: OutPoint {
            txid: create_txid,
            vout: 0
          },
          offset: 0
        }),
        sequence_number: 0,
        block_height: 2,
        charms: expected_charms,
        parent_inscription_ids: Vec::new(),
      }
    );

    // Transfer inscription
    let transfer_txid = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(2, 1, 0, Default::default())],
      fee: 0,
      outputs: 1,
      ..default()
    });

    context.mine_blocks(1);

    let transfer_event = event_receiver.blocking_recv().unwrap();
    assert_eq!(
      transfer_event,
      Event::InscriptionTransferred {
        block_height: 3,
        inscription_id,
        new_location: SatPoint {
          outpoint: OutPoint {
            txid: transfer_txid,
            vout: 0
          },
          offset: 0
        },
        old_location: SatPoint {
          outpoint: OutPoint {
            txid: create_txid,
            vout: 0
          },
          offset: 0
        },
        sequence_number: 0,
      }
    );
  }

  #[test]
  fn rune_event_sender_channel() {
    const RUNE: u128 = 99246114928149462;

    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(1024);
    let context = Context::builder()
      .arg("--index-runes")
      .event_sender(event_sender)
      .build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          mints: 0,
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          ..default()
        },
      )],
      [],
    );

    assert_eq!(
      event_receiver.blocking_recv().unwrap(),
      Event::RuneEtched {
        block_height: 8,
        txid: txid0,
        rune_id: id,
      }
    );

    let txid1 = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          mint: Some(id),
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid0,
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          mints: 1,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 0,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );

    assert_eq!(
      event_receiver.blocking_recv().unwrap(),
      Event::RuneMinted {
        block_height: 9,
        txid: txid1,
        rune_id: id,
        amount: 1000,
      }
    );

    let txid2 = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(9, 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
          ..Default::default()
        }
        .encipher(),
      ),
      ..Default::default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: 8,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            ..default()
          },
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..Default::default()
          }),
          timestamp: 8,
          mints: 1,
          ..Default::default()
        },
      )],
      [(
        OutPoint {
          txid: txid2,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );

    event_receiver.blocking_recv().unwrap();

    pretty_assert_eq!(
      event_receiver.blocking_recv().unwrap(),
      Event::RuneTransferred {
        block_height: 10,
        txid: txid2,
        rune_id: id,
        amount: 1000,
        outpoint: OutPoint {
          txid: txid2,
          vout: 0,
        },
      }
    );

    let txid3 = context.core.broadcast_tx(TransactionTemplate {
      inputs: &[(10, 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 111,
            output: 0,
          }],
          ..Default::default()
        }
        .encipher(),
      ),
      op_return_index: Some(0),
      ..Default::default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: 8,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            ..default()
          },
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..Default::default()
          }),
          timestamp: 8,
          mints: 1,
          burned: 111,
          ..Default::default()
        },
      )],
      [(
        OutPoint {
          txid: txid3,
          vout: 1,
        },
        vec![(id, 889)],
      )],
    );

    event_receiver.blocking_recv().unwrap();

    pretty_assert_eq!(
      event_receiver.blocking_recv().unwrap(),
      Event::RuneBurned {
        block_height: 11,
        txid: txid3,
        amount: 111,
        rune_id: id,
      }
    );
  }

  #[test]
  fn assert_schema_statistic_key_is_zero() {
    // other schema statistic keys may chenge when the schema changes, but for
    // good error messages in older versions, the schema statistic key must be
    // zero
    assert_eq!(Statistic::Schema.key(), 0);
  }
}
