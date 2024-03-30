use {
  super::*,
  entry::{ResumeEntry, ResumeEntryValue},
  index::entry::Entry,
  indicatif::{ProgressBar, ProgressStyle},
  log::log_enabled,
  redb::{
    Database, DatabaseError, ReadTransaction, ReadableTable, RepairSession, StorageError,
    TableDefinition, WriteTransaction,
  },
  std::sync::Once,
};

mod entry;

const SCHEMA_VERSION: u64 = 1;

define_table! { RUNE_TO_INFO, u128, ResumeEntryValue }
define_table! { STATISTICS, u64, u64 }

#[derive(Copy, Clone)]
pub(crate) enum Statistic {
  Schema = 0,
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

pub(crate) struct Db {
  database: Database,
  durability: redb::Durability,
}

impl Db {
  pub(crate) fn open(
    wallet_name: &String,
    settings: &Settings,
    wallet_db: Option<PathBuf>,
  ) -> Result<Self> {
    let path = wallet_db.unwrap_or_else(|| settings.data_dir().join(format!("{wallet_name}.redb")));
    let path_clone = path.clone().to_owned();
    let once = Once::new();
    let progress_bar = Mutex::new(None);
    let integration_test = settings.integration_test();

    let durability = if cfg!(test) {
      redb::Durability::None
    } else {
      redb::Durability::Immediate
    };

    let repair_callback = move |progress: &mut RepairSession| {
      once.call_once(|| {
        println!(
          "Wallet database file `{}` needs recovery. This can take some time.",
          path_clone.display()
        )
      });

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
      .set_repair_callback(repair_callback)
      .open(&path)
    {
      Ok(database) => {
        {
          let schema_version = database
            .begin_read()?
            .open_table(STATISTICS)?
            .get(&Statistic::Schema.key())?
            .map(|x| x.value())
            .unwrap_or(0);

          match schema_version.cmp(&SCHEMA_VERSION) {
            cmp::Ordering::Less =>
              bail!(
                "wallet database at `{}` appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
                path.display()
              ),
            cmp::Ordering::Greater =>
              bail!(
                "wallet database at `{}` appears to have been built with a newer, incompatible version of ord, consider updating ord: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
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
        let database = Database::builder().create(&path)?;

        let mut tx = database.begin_write()?;

        tx.set_durability(durability);

        tx.open_table(RUNE_TO_INFO)?;

        {
          let mut statistics = tx.open_table(STATISTICS)?;
          statistics.insert(&Statistic::Schema.key(), &SCHEMA_VERSION)?;
        }

        tx.commit()?;

        database
      }
      Err(error) => bail!("failed to open index: {error}"),
    };

    Ok(Self {
      database,
      durability,
    })
  }

  fn begin_read(&self) -> Result<ReadTransaction> {
    Ok(self.database.begin_read()?)
  }

  fn begin_write(&self) -> Result<WriteTransaction> {
    let mut wtx = self.database.begin_write()?;
    wtx.set_durability(self.durability);
    Ok(wtx)
  }

  pub(crate) fn store(&self, rune: Rune, commit: &Transaction, reveal: &Transaction) -> Result {
    let wtx = self.begin_write()?;

    wtx.open_table(RUNE_TO_INFO)?.insert(
      rune.0,
      ResumeEntry {
        commit: commit.clone(),
        reveal: reveal.clone(),
      }
      .store(),
    )?;

    wtx.commit()?;

    Ok(())
  }

  pub(crate) fn retrieve(&self, rune: Rune) -> Result<Option<ResumeEntry>> {
    let rtx = self.begin_read()?;

    Ok(
      rtx
        .open_table(RUNE_TO_INFO)?
        .get(rune.0)?
        .map(|result| ResumeEntry::load(result.value())),
    )
  }

  pub(crate) fn clear(&self, rune: Rune) -> Result {
    let wtx = self.begin_write()?;

    wtx.open_table(RUNE_TO_INFO)?.remove(rune.0)?;
    wtx.commit()?;

    Ok(())
  }

  pub(crate) fn pending(&self) -> Result<Vec<(Rune, ResumeEntry)>> {
    let rtx = self.begin_read()?;

    Ok(
      rtx
        .open_table(RUNE_TO_INFO)?
        .iter()?
        .map(|result| {
          result.map(|(key, value)| (Rune(key.value()), ResumeEntry::load(value.value())))
        })
        .collect::<Result<Vec<(Rune, ResumeEntry)>, StorageError>>()?,
    )
  }
}

//#[cfg(test)]
//mod tests {
//  use super::*;
//
//  #[test]
//  fn resume_entry() {
//    let commit = Transaction {
//      ..Default::default()
//    };
//
//    let reveal = Transaction {
//      ..Default::default()
//    };
//
//    let entry = ResumeEntry { commit, reveal };
//
//    let value = (Vec::new(), Vec::new());
//
//    assert_eq!(entry.clone().store(), value);
//    assert_eq!(ResumeEntry::load(value), entry);
//  }
//}
