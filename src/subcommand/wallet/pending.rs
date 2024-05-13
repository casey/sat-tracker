use super::*;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct PendingOutput {
  pub rune: SpacedRune,
  pub commit: Txid,
}
#[derive(Debug, Parser)]
pub(crate) struct Pending {}

impl Pending {
  pub(crate) fn run(self, wallet: Wallet) -> SubcommandResult {
    let etchings = wallet
      .pending_etchings()?
      .into_iter()
      .map(|(_, entry)| {
        let spaced_rune = entry.output.rune.unwrap().rune;

        PendingOutput {
          rune: spaced_rune,
          commit: entry.commit.txid()
        }
      })
      .collect::<Vec<PendingOutput>>();

    Ok(Some(Box::new(etchings) as Box<dyn Output>))
  }
}
