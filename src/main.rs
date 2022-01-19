use {
  crate::{consts::*, epoch::Epoch, height::Height, index::Index, ordinal::Ordinal},
  bitcoin::{
    blockdata::constants::{genesis_block, COIN_VALUE},
    consensus::Decodable,
    Block, Network,
  },
  command::Command,
  derive_more::{Add, Display, FromStr},
  integer_cbrt::IntegerCubeRoot,
  integer_sqrt::IntegerSquareRoot,
  redb::{
    Database, MultimapTable, ReadOnlyMultimapTable, ReadOnlyTable, ReadableMultimapTable,
    ReadableTable, Table,
  },
  std::{
    cmp::Ordering,
    fs,
    ops::{Add, AddAssign, Deref, Range, Sub},
    path::{Path, PathBuf},
    process,
    str::FromStr,
  },
  structopt::StructOpt,
};

mod consts;
mod epoch;
mod height;
mod index;
mod name;
mod ordinal;

mod command;

type Result<T = (), E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

fn main() {
  env_logger::init();

  if let Err(error) = Command::from_args().run() {
    eprintln!("error: {}", error);
    process::exit(1);
  }
}
