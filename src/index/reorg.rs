use {super::*, updater::BlockData};

#[derive(Debug, PartialEq)]
pub(crate) enum Error {
  Recoverable { height: u32, depth: u32 },
  Unrecoverable,
}

impl Display for Error {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Recoverable { height, depth } => {
        write!(f, "{depth} block deep reorg detected at height {height}")
      }
      Self::Unrecoverable => write!(f, "unrecoverable reorg detected"),
    }
  }
}

impl std::error::Error for Error {}

const MAX_SAVEPOINTS: u32 = 2;
const SAVEPOINT_INTERVAL: u32 = 10;
const CHAIN_TIP_DISTANCE: u32 = 21;

pub(crate) struct Reorg {}

impl Reorg {
  pub(crate) fn detect_reorg(block: &BlockData, height: u32, index: &Index) -> Result {
    let bitcoind_prev_blockhash = block.header.prev_blockhash;

    match index.block_hash(height.checked_sub(1))? {
      Some(index_prev_blockhash) if index_prev_blockhash == bitcoind_prev_blockhash => Ok(()),
      Some(index_prev_blockhash) if index_prev_blockhash != bitcoind_prev_blockhash => {
        let max_recoverable_reorg_depth =
          (MAX_SAVEPOINTS - 1) * SAVEPOINT_INTERVAL + height % SAVEPOINT_INTERVAL;

        for depth in 1..max_recoverable_reorg_depth {
          let index_block_hash = index.block_hash(height.checked_sub(depth))?;
          let bitcoind_block_hash = index
            .client
            .get_block_hash(u64::from(height.saturating_sub(depth)))
            .into_option()?;

          if index_block_hash == bitcoind_block_hash {
            return Err(anyhow!(reorg::Error::Recoverable { height, depth }));
          }
        }

        Err(anyhow!(reorg::Error::Unrecoverable))
      }
      _ => Ok(()),
    }
  }

  pub(crate) fn handle_reorg(index: &Index, height: u32, depth: u32) -> Result {
    log::info!("rolling back database after reorg of depth {depth} at height {height}");

    if let redb::Durability::None = index.durability {
      panic!("set index durability to `Durability::Immediate` to test reorg handling");
    }

    let mut wtx = index.begin_write()?;

    let oldest_savepoint =
      wtx.get_persistent_savepoint(wtx.list_persistent_savepoints()?.min().unwrap())?;

    wtx.restore_savepoint(&oldest_savepoint)?;

    Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
    wtx.commit()?;

    log::info!(
      "successfully rolled back database to height {}",
      index.begin_read()?.block_count()?
    );

    Ok(())
  }

  pub(crate) fn update_savepoints(index: &Index, height: u32) -> Result {
    if let redb::Durability::None = index.durability {
      return Ok(());
    }

    let last_savepoint_height: u32 = index
      .begin_read()?
      .0
      .open_table(STATISTIC_TO_COUNT)?
      .get(&Statistic::LastSavepointHeight.key())?
      .map(|last_savepoint_height| last_savepoint_height.value())
      .unwrap_or(0)
      .try_into()
      .unwrap();

    let blocks: u32 = index
      .client
      .get_blockchain_info()?
      .headers
      .try_into()
      .unwrap();

    if (height < SAVEPOINT_INTERVAL
      || height.saturating_sub(last_savepoint_height) >= SAVEPOINT_INTERVAL)
      && blocks.saturating_sub(height) <= CHAIN_TIP_DISTANCE
    {
      let wtx = index.begin_write()?;

      let savepoints = wtx.list_persistent_savepoints()?.collect::<Vec<u64>>();

      if savepoints.len() >= usize::try_from(MAX_SAVEPOINTS).unwrap() {
        wtx.delete_persistent_savepoint(savepoints.into_iter().min().unwrap())?;
      }

      Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
      wtx.commit()?;

      let wtx = index.begin_write()?;

      log::debug!("creating savepoint at height {}", height);
      wtx.persistent_savepoint()?;

      wtx
        .open_table(STATISTIC_TO_COUNT)?
        .insert(&Statistic::LastSavepointHeight.key(), &u64::from(height))?;

      Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
      wtx.commit()?;
    }

    Ok(())
  }
}
