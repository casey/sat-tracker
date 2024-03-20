use super::*;

#[derive(Default, Serialize, Debug, PartialEq, Copy, Clone)]
pub struct Edict {
  pub id: RuneId,
  pub amount: u128,
  pub output: u32,
}

impl Edict {
  pub(crate) fn from_integers(
    tx: &Transaction,
    id: u128,
    amount: u128,
    output: u128,
  ) -> Option<Self> {
    let id = RuneId::try_from(id).ok()?;

    if id.block == 0 && id.tx > 0 {
      return None;
    }

    let Ok(output) = u32::try_from(output) else {
      return None;
    };

    if output > u32::try_from(tx.output.len()).unwrap() {
      return None;
    }

    Some(Self { id, amount, output })
  }
}
