use super::*;

#[derive(StructOpt)]
pub(crate) enum Arguments {
  Find {
    #[structopt(long)]
    blocksdir: Option<PathBuf>,
    ordinal: Ordinal,
    height: u64,
  },
  Name {
    name: String,
  },
  Range {
    #[structopt(long)]
    name: bool,
    height: u64,
  },
  Supply,
  Traits {
    ordinal: Ordinal,
  },
}

impl Arguments {
  pub(crate) fn run(self) -> Result<()> {
    match self {
      Self::Find {
        blocksdir,
        ordinal,
        height,
      } => crate::find::run(blocksdir.as_deref(), ordinal, height),
      Self::Name { name } => crate::name::run(&name),
      Self::Range { height, name } => crate::range::run(height, name),
      Self::Supply => crate::supply::run(),
      Self::Traits { ordinal } => crate::traits::run(ordinal),
    }
  }
}
