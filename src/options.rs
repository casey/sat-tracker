use super::*;

#[derive(Parser)]
pub(crate) struct Options {
  #[clap(long, default_value = "10MiB")]
  pub(crate) max_index_size: Bytes,
  #[clap(long)]
  pub(crate) cookie_file: Option<PathBuf>,
  #[clap(long)]
  pub(crate) rpc_url: Option<String>,
}
