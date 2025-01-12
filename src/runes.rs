use super::*;

#[derive(Debug, PartialEq)]
pub enum MintError {
  Cap(u128),
  End(u64),
  Start(u64),
  Unmintable,
}

impl Display for MintError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      MintError::Cap(cap) => write!(f, "limited to {cap} mints"),
      MintError::End(end) => write!(f, "mint ended on block {end}"),
      MintError::Start(start) => write!(f, "mint starts on block {start}"),
      MintError::Unmintable => write!(f, "not mintable"),
    }
  }
}

#[cfg(test)]
mod tests {
  use {super::*, crate::index::testing::Context};

  const RUNE: u128 = 99246114928149462;

  #[test]
  fn index_starts_with_no_runes() {
    let context = Context::builder().arg("--index-runes").build();
    context.assert_runes([], []);
  }

  #[test]
  fn default_index_does_not_index_runes() {
    let context = Context::builder().build();

    context.mine_blocks(1);

    context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes([], []);
  }

  #[test]
  fn empty_runestone_does_not_create_rune() {
    let context = Context::builder().arg("--index-runes").build();

    context.mine_blocks(1);

    context.etch(Default::default(), 1);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(1, 0, 0, Witness::new())],
      op_return: Some(Runestone::default().encipher()),
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes([], []);
  }

  #[test]
  fn etching_with_no_edicts_creates_rune() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn etching_with_edict_creates_rune() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn runes_must_be_greater_than_or_equal_to_minimum_for_height() {
    let minimum = Rune::minimum_at_height(
      Chain::Regtest.network(),
      Height((Runestone::COMMIT_CONFIRMATIONS + 2).into()),
    )
    .0;

    {
      let context = Context::builder()
        .chain(Chain::Regtest)
        .arg("--index-runes")
        .build();

      context.etch(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching {
            rune: Some(Rune(minimum - 1)),
            premine: Some(u128::MAX),
            ..default()
          }),
          ..default()
        },
        1,
      );

      context.assert_runes([], []);
    }

    {
      let context = Context::builder()
        .chain(Chain::Regtest)
        .arg("--index-runes")
        .build();

      let (txid, id) = context.etch(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching {
            rune: Some(Rune(minimum)),
            premine: Some(u128::MAX),
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
            etching: txid,
            spaced_rune: SpacedRune {
              rune: Rune(minimum),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id.block,
            ..default()
          },
        )],
        [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
      );
    }
  }

  #[test]
  fn etching_cannot_specify_reserved_rune() {
    {
      let context = Context::builder().arg("--index-runes").build();

      context.etch(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching {
            rune: Some(Rune::reserved(0, 0)),
            ..default()
          }),
          ..default()
        },
        1,
      );

      context.assert_runes([], []);
    }

    {
      let context = Context::builder().arg("--index-runes").build();

      let (txid, id) = context.etch(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching {
            rune: Some(Rune(Rune::reserved(0, 0).n() - 1)),
            premine: Some(u128::MAX),
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
            etching: txid,
            spaced_rune: SpacedRune {
              rune: Rune(Rune::reserved(0, 0).n() - 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id.block,
            ..default()
          },
        )],
        [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
      );
    }
  }

  #[test]
  fn reserved_runes_may_be_etched() {
    let context = Context::builder().arg("--index-runes").build();

    context.mine_blocks(1);

    let txid0 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(1, 0, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching {
            rune: None,
            premine: Some(u128::MAX),
            ..default()
          }),
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    let id0 = RuneId { block: 2, tx: 1 };

    context.mine_blocks(1);

    context.assert_runes(
      [(
        id0,
        RuneEntry {
          block: id0.block,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune::reserved(id0.block, id0.tx),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: 2,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id0, u128::MAX)],
      )],
    );

    context.mine_blocks(1);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching {
            premine: Some(u128::MAX),
            rune: None,
            ..default()
          }),
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    context.mine_blocks(1);

    let id1 = RuneId { block: 4, tx: 1 };

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune::reserved(id0.block, id0.tx),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: 2,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune::reserved(id1.block, id0.tx),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: 4,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid0,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
      ],
    );
  }

  #[test]
  fn etching_with_non_zero_divisibility_and_rune() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          divisibility: Some(1),
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.mine_blocks(1);

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          etching: txid,
          divisibility: 1,
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn allocations_over_max_supply_are_ignored() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn allocations_partially_over_max_supply_are_honored() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: u128::MAX / 2,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          symbol: None,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn etching_may_allocate_less_than_max_supply() {
    let context = Context::builder().arg("--index-runes").build();

    context.mine_blocks(1);

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 100,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(100),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 100,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, 100)])],
    );
  }

  #[test]
  fn etching_may_allocate_to_multiple_outputs() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: 100,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: 100,
            output: 1,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(200),
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
          burned: 100,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 200,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, 100)])],
    );
  }

  #[test]
  fn allocations_to_invalid_outputs_produce_cenotaph() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: 100,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: 100,
            output: 3,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 0,
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn input_runes_may_be_allocated() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: u128::MAX,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );
  }

  #[test]
  fn etched_rune_is_allocated_with_zero_supply_for_cenotaph() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          ..default()
        }),
        pointer: Some(10),
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
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn etched_rune_parameters_are_unset_for_cenotaph() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          premine: Some(u128::MAX),
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            cap: Some(1),
            amount: Some(1),
            offset: (Some(1), Some(1)),
            height: (None, None),
          }),
          divisibility: Some(1),
          symbol: Some('$'),
          spacers: Some(1),
          turbo: true,
        }),
        pointer: Some(10),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          burned: 0,
          divisibility: 0,
          etching: txid0,
          terms: None,
          mints: 0,
          number: 0,
          premine: 0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          symbol: None,
          timestamp: id.block,
          turbo: false,
        },
      )],
      [],
    );
  }

  #[test]
  fn reserved_runes_are_not_allocated_in_cenotaph() {
    let context = Context::builder().arg("--index-runes").build();

    context.mine_blocks(1);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(1, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          }],
          etching: Some(Etching::default()),
          pointer: Some(10),
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes([], []);
  }

  #[test]
  fn input_runes_are_burned_if_an_unrecognized_even_tag_is_encountered() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          pointer: Some(10),
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
          burned: u128::MAX,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn unallocated_runes_are_assigned_to_first_non_op_return_output() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(Runestone::default().encipher()),
      ..default()
    });

    context.mine_blocks(1);

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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );
  }

  #[test]
  fn unallocated_runes_are_burned_if_no_non_op_return_output_is_present() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(Runestone::default().encipher()),
      outputs: 0,
      ..default()
    });

    context.mine_blocks(1);

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
          premine: u128::MAX,
          timestamp: id.block,
          burned: u128::MAX,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn unallocated_runes_are_assigned_to_default_output() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          pointer: Some(1),
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 1,
        },
        vec![(id, u128::MAX)],
      )],
    );
  }

  #[test]
  fn unallocated_runes_are_burned_if_default_output_is_op_return() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          pointer: Some(2),
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          burned: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn unallocated_runes_in_transactions_with_no_runestone_are_assigned_to_first_non_op_return_output(
  ) {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: None,
      ..default()
    });

    context.mine_blocks(1);

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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );
  }

  #[test]
  fn duplicate_runes_are_forbidden() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );

    context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.mine_blocks(1);

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn output_may_hold_multiple_runes() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id0) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id0,
        RuneEntry {
          block: id0.block,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id0.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id0, u128::MAX)],
      )],
    );

    let (txid1, id1) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE + 1)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid0,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
      ],
    );

    let txid2 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[
        (id0.block.try_into().unwrap(), 1, 0, Witness::new()),
        (id1.block.try_into().unwrap(), 1, 0, Witness::new()),
      ],
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [(
        OutPoint {
          txid: txid2,
          vout: 0,
        },
        vec![(id0, u128::MAX), (id1, u128::MAX)],
      )],
    );
  }

  #[test]
  fn multiple_input_runes_on_the_same_input_may_be_allocated() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id0) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id0,
        RuneEntry {
          block: id0.block,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id0.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id0, u128::MAX)],
      )],
    );

    let (txid1, id1) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE + 1)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid0,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
      ],
    );

    let txid2 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[
        (id0.block.try_into().unwrap(), 1, 0, Witness::new()),
        (id1.block.try_into().unwrap(), 1, 0, Witness::new()),
      ],
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [(
        OutPoint {
          txid: txid2,
          vout: 0,
        },
        vec![(id0, u128::MAX), (id1, u128::MAX)],
      )],
    );

    let txid3 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[((id1.block + 1).try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id: id0,
              amount: u128::MAX / 2,
              output: 1,
            },
            Edict {
              id: id1,
              amount: u128::MAX / 2,
              output: 1,
            },
          ],
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid3,
            vout: 0,
          },
          vec![(id0, u128::MAX / 2 + 1), (id1, u128::MAX / 2 + 1)],
        ),
        (
          OutPoint {
            txid: txid3,
            vout: 1,
          },
          vec![(id0, u128::MAX / 2), (id1, u128::MAX / 2)],
        ),
      ],
    );
  }

  #[test]
  fn multiple_input_runes_on_different_inputs_may_be_allocated() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id0) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id0,
        RuneEntry {
          block: id0.block,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id0.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id0, u128::MAX)],
      )],
    );

    let (txid1, id1) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE + 1)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid0,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
      ],
    );

    let txid2 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[
        (id0.block.try_into().unwrap(), 1, 0, Witness::new()),
        (id1.block.try_into().unwrap(), 1, 0, Witness::new()),
      ],
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id: id0,
              amount: u128::MAX,
              output: 0,
            },
            Edict {
              id: id1,
              amount: u128::MAX,
              output: 0,
            },
          ],
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [(
        OutPoint {
          txid: txid2,
          vout: 0,
        },
        vec![(id0, u128::MAX), (id1, u128::MAX)],
      )],
    );
  }

  #[test]
  fn unallocated_runes_are_assigned_to_first_non_op_return_output_when_op_return_is_not_last_output(
  ) {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .into_script(),
      ),
      op_return_index: Some(0),
      ..default()
    });

    context.mine_blocks(1);

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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 1 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn multiple_runes_may_be_etched_in_one_block() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id0) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let (txid1, id1) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE + 1)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid0,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
      ],
    );
  }

  #[test]
  fn edicts_with_id_zero_are_skipped() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id: RuneId::default(),
              amount: 100,
              output: 0,
            },
            Edict {
              id,
              amount: u128::MAX,
              output: 0,
            },
          ],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );
  }

  #[test]
  fn edicts_which_refer_to_input_rune_with_no_balance_are_skipped() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id0) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id0,
        RuneEntry {
          block: id0.block,
          etching: txid0,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id0.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id0, u128::MAX)],
      )],
    );

    let (txid1, id1) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE + 1)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      1,
    );

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid0,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
      ],
    );

    let txid2 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id0.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id: id0,
              amount: u128::MAX,
              output: 0,
            },
            Edict {
              id: id1,
              amount: u128::MAX,
              output: 0,
            },
          ],
          ..default()
        }
        .encipher(),
      ),
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes(
      [
        (
          id0,
          RuneEntry {
            block: id0.block,
            etching: txid0,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id0.block,
            ..default()
          },
        ),
        (
          id1,
          RuneEntry {
            block: id1.block,
            etching: txid1,
            spaced_rune: SpacedRune {
              rune: Rune(RUNE + 1),
              spacers: 0,
            },
            premine: u128::MAX,
            timestamp: id1.block,
            number: 1,
            ..default()
          },
        ),
      ],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id1, u128::MAX)],
        ),
        (
          OutPoint {
            txid: txid2,
            vout: 0,
          },
          vec![(id0, u128::MAX)],
        ),
      ],
    );
  }

  #[test]
  fn edicts_over_max_inputs_are_ignored() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX / 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX / 2),
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
          premine: u128::MAX / 2,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX / 2)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: u128::MAX,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX / 2,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, u128::MAX / 2)],
      )],
    );
  }

  #[test]
  fn edicts_may_transfer_runes_to_op_return_outputs() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 1,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          burned: u128::MAX,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn outputs_with_no_runes_have_no_balance() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn edicts_which_transfer_no_runes_to_output_create_no_balance_entry() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: 0,
            output: 1,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn split_in_etching() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 0,
          output: 5,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      4,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (OutPoint { txid, vout: 0 }, vec![(id, u128::MAX / 4 + 1)]),
        (OutPoint { txid, vout: 1 }, vec![(id, u128::MAX / 4 + 1)]),
        (OutPoint { txid, vout: 2 }, vec![(id, u128::MAX / 4 + 1)]),
        (OutPoint { txid, vout: 3 }, vec![(id, u128::MAX / 4)]),
      ],
    );
  }

  #[test]
  fn split_in_etching_with_preceding_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: 1000,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: 0,
            output: 5,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      4,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint { txid, vout: 0 },
          vec![(id, 1000 + (u128::MAX - 1000) / 4 + 1)],
        ),
        (
          OutPoint { txid, vout: 1 },
          vec![(id, (u128::MAX - 1000) / 4 + 1)],
        ),
        (
          OutPoint { txid, vout: 2 },
          vec![(id, (u128::MAX - 1000) / 4 + 1)],
        ),
        (
          OutPoint { txid, vout: 3 },
          vec![(id, (u128::MAX - 1000) / 4)],
        ),
      ],
    );
  }

  #[test]
  fn split_in_etching_with_following_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: 0,
            output: 5,
          },
          Edict {
            id: RuneId::default(),
            amount: 1000,
            output: 0,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      4,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (OutPoint { txid, vout: 0 }, vec![(id, u128::MAX / 4 + 1)]),
        (OutPoint { txid, vout: 1 }, vec![(id, u128::MAX / 4 + 1)]),
        (OutPoint { txid, vout: 2 }, vec![(id, u128::MAX / 4 + 1)]),
        (OutPoint { txid, vout: 3 }, vec![(id, u128::MAX / 4)]),
      ],
    );
  }

  #[test]
  fn split_with_amount_in_etching() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 1000,
          output: 5,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(4000),
          ..default()
        }),
        ..default()
      },
      4,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 4000,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (OutPoint { txid, vout: 0 }, vec![(id, 1000)]),
        (OutPoint { txid, vout: 1 }, vec![(id, 1000)]),
        (OutPoint { txid, vout: 2 }, vec![(id, 1000)]),
        (OutPoint { txid, vout: 3 }, vec![(id, 1000)]),
      ],
    );
  }

  #[test]
  fn split_in_etching_with_amount_with_preceding_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: u128::MAX - 3000,
            output: 0,
          },
          Edict {
            id: RuneId::default(),
            amount: 1000,
            output: 5,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      4,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (OutPoint { txid, vout: 0 }, vec![(id, u128::MAX - 2000)]),
        (OutPoint { txid, vout: 1 }, vec![(id, 1000)]),
        (OutPoint { txid, vout: 2 }, vec![(id, 1000)]),
      ],
    );
  }

  #[test]
  fn split_in_etching_with_amount_with_following_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![
          Edict {
            id: RuneId::default(),
            amount: 1000,
            output: 5,
          },
          Edict {
            id: RuneId::default(),
            amount: u128::MAX,
            output: 0,
          },
        ],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
          ..default()
        }),
        ..default()
      },
      4,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint { txid, vout: 0 },
          vec![(id, u128::MAX - 4000 + 1000)],
        ),
        (OutPoint { txid, vout: 1 }, vec![(id, 1000)]),
        (OutPoint { txid, vout: 2 }, vec![(id, 1000)]),
        (OutPoint { txid, vout: 3 }, vec![(id, 1000)]),
      ],
    );
  }

  #[test]
  fn split() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 0,
            output: 3,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, u128::MAX / 2 + 1)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, u128::MAX / 2)],
        ),
      ],
    );
  }

  #[test]
  fn split_with_preceding_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id,
              amount: 1000,
              output: 0,
            },
            Edict {
              id,
              amount: 0,
              output: 3,
            },
          ],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, 1000 + (u128::MAX - 1000) / 2 + 1)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, (u128::MAX - 1000) / 2)],
        ),
      ],
    );
  }

  #[test]
  fn split_with_following_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id,
              amount: 0,
              output: 3,
            },
            Edict {
              id,
              amount: 1000,
              output: 1,
            },
          ],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, u128::MAX / 2 + 1)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, u128::MAX / 2)],
        ),
      ],
    );
  }

  #[test]
  fn split_with_amount() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 3,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, u128::MAX - 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, 1000)],
        ),
      ],
    );
  }

  #[test]
  fn split_with_amount_with_preceding_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 4,
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id,
              amount: u128::MAX - 2000,
              output: 0,
            },
            Edict {
              id,
              amount: 1000,
              output: 5,
            },
          ],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, u128::MAX - 2000 + 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, 1000)],
        ),
      ],
    );
  }

  #[test]
  fn split_with_amount_with_following_edict() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 4,
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id,
              amount: 1000,
              output: 5,
            },
            Edict {
              id,
              amount: u128::MAX,
              output: 0,
            },
          ],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, u128::MAX - 4000 + 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 2,
          },
          vec![(id, 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 3,
          },
          vec![(id, 1000)],
        ),
      ],
    );
  }

  #[test]
  fn etching_may_specify_symbol() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          symbol: Some('$'),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          symbol: Some('$'),
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn allocate_all_remaining_runes_in_etching() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 0,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
    );
  }

  #[test]
  fn allocate_all_remaining_runes_in_inputs() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: u128::MAX,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 0,
            output: 1,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid1,
          vout: 1,
        },
        vec![(id, u128::MAX)],
      )],
    );
  }

  #[test]
  fn rune_can_be_minted_without_edict() {
    let context = Context::builder().arg("--index-runes").build();

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

    let txid1 = context.core.broadcast_template(TransactionTemplate {
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
  }

  #[test]
  fn rune_cannot_be_minted_less_than_limit_amount() {
    let context = Context::builder().arg("--index-runes").build();

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

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          mint: Some(id),
          edicts: vec![Edict {
            id,
            amount: 111,
            output: 0,
          }],
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
  }

  #[test]
  fn etching_with_amount_can_be_minted() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            cap: Some(100),
            amount: Some(1000),
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
          premine: 0,
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

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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

    // claim the rune
    let txid2 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(4, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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
          mints: 2,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 0,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid2,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
      ],
    );

    // claim the rune in a burn runestone
    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(5, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          pointer: Some(10),
          mint: Some(id),
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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
          burned: 1000,
          etching: txid0,
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          mints: 3,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          premine: 0,
          timestamp: id.block,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid2,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
      ],
    );
  }

  #[test]
  fn open_mints_can_be_limited_with_offset_end() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            offset: (None, Some(2)),
            ..default()
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        offset: (None, Some(2)),
        cap: Some(100),
        ..default()
      }),
      timestamp: id.block,
      ..default()
    };

    context.assert_runes([(id, entry)], []);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
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

    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_can_be_limited_with_offset_start() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            offset: (Some(2), None),
            ..default()
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        offset: (Some(2), None),
        cap: Some(100),
        ..default()
      }),
      timestamp: id.block,
      ..default()
    };

    context.assert_runes([(id, entry)], []);

    context.core.broadcast_template(TransactionTemplate {
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

    context.assert_runes([(id, entry)], []);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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

    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_can_be_limited_with_height_start() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            height: (Some(10), None),
            ..default()
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        height: (Some(10), None),
        cap: Some(100),
        ..default()
      }),
      timestamp: id.block,
      ..default()
    };

    context.assert_runes([(id, entry)], []);

    context.core.broadcast_template(TransactionTemplate {
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

    context.assert_runes([(id, entry)], []);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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

    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_can_be_limited_with_height_end() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            height: (None, Some(10)),
            ..default()
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        height: (None, Some(10)),
        cap: Some(100),
        ..default()
      }),
      timestamp: id.block,
      ..default()
    };

    context.assert_runes([(id, entry)], []);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
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

    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_must_be_ended_with_etched_height_plus_offset_end() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            height: (None, Some(100)),
            offset: (None, Some(2)),
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        height: (None, Some(100)),
        offset: (None, Some(2)),
        cap: Some(100),
      }),
      timestamp: id.block,
      ..default()
    };

    context.assert_runes([(id, entry)], []);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
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
    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_must_be_ended_with_height_end() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            height: (None, Some(10)),
            offset: (None, Some(100)),
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        height: (None, Some(10)),
        offset: (None, Some(100)),
        cap: Some(100),
      }),
      timestamp: id.block,
      ..default()
    };

    context.assert_runes([(id, entry)], []);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
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
    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_must_be_started_with_height_start() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            height: (Some(11), None),
            offset: (Some(1), None),
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry0 = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        height: (Some(11), None),
        offset: (Some(1), None),
        cap: Some(100),
      }),
      timestamp: id.block,
      ..default()
    };

    context.mine_blocks(1);

    context.core.broadcast_template(TransactionTemplate {
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

    context.assert_runes([(id, entry0)], []);

    context.mine_blocks(1);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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

    entry0.mints += 1;

    context.assert_runes(
      [(id, entry0)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_must_be_started_with_etched_height_plus_offset_start() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            height: (Some(9), None),
            offset: (Some(3), None),
          }),
          ..default()
        }),
        ..default()
      },
      1,
    );

    let mut entry = RuneEntry {
      block: id.block,
      etching: txid0,
      spaced_rune: SpacedRune {
        rune: Rune(RUNE),
        spacers: 0,
      },
      terms: Some(Terms {
        amount: Some(1000),
        height: (Some(9), None),
        offset: (Some(3), None),
        cap: Some(100),
      }),
      timestamp: id.block,
      ..default()
    };

    context.mine_blocks(1);

    context.core.broadcast_template(TransactionTemplate {
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

    context.assert_runes([(id, entry)], []);

    context.mine_blocks(1);

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
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

    entry.mints += 1;

    context.assert_runes(
      [(id, entry)],
      [(
        OutPoint {
          txid: txid1,
          vout: 0,
        },
        vec![(id, 1000)],
      )],
    );
  }

  #[test]
  fn open_mints_with_offset_end_zero_can_be_premined() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 1111,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(1111),
          terms: Some(Terms {
            amount: Some(1000),
            offset: (None, Some(0)),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          terms: Some(Terms {
            amount: Some(1000),
            offset: (None, Some(0)),
            ..default()
          }),
          timestamp: id.block,
          premine: 1111,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, 1111)])],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      outputs: 2,
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          terms: Some(Terms {
            amount: Some(1000),
            offset: (None, Some(0)),
            ..default()
          }),
          premine: 1111,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, 1111)])],
    );
  }

  #[test]
  fn open_mints_can_be_limited_to_cap() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(2),
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
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(2),
            ..default()
          }),
          ..default()
        },
      )],
      [],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          mints: 1,
          etching: txid0,
          terms: Some(Terms {
            cap: Some(2),
            amount: Some(1000),
            ..default()
          }),
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

    let txid2 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(2),
            ..default()
          }),
          mints: 2,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
        (
          OutPoint {
            txid: txid2,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
      ],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(4, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(2),
            ..default()
          }),
          mints: 2,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
        (
          OutPoint {
            txid: txid2,
            vout: 0,
          },
          vec![(id, 1000)],
        ),
      ],
    );
  }

  #[test]
  fn open_mints_without_a_cap_are_unmintable() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            amount: Some(1000),
            offset: (None, Some(2)),
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
          terms: Some(Terms {
            amount: Some(1000),
            offset: (None, Some(2)),
            ..default()
          }),
          ..default()
        },
      )],
      [],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 1000,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          mints: 0,
          etching: txid0,
          terms: Some(Terms {
            amount: Some(1000),
            offset: (None, Some(2)),
            ..default()
          }),
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn open_mint_claims_can_use_split() {
    let context = Context::builder().arg("--index-runes").build();

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
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(3, 0, 0, Witness::new())],
      outputs: 2,
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 0,
            output: 3,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          timestamp: id.block,
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          mints: 1,
          ..default()
        },
      )],
      [
        (
          OutPoint {
            txid: txid1,
            vout: 0,
          },
          vec![(id, 500)],
        ),
        (
          OutPoint {
            txid: txid1,
            vout: 1,
          },
          vec![(id, 500)],
        ),
      ],
    );
  }

  #[test]
  fn runes_can_be_etched_and_premined_in_the_same_transaction() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(2000),
          terms: Some(Terms {
            amount: Some(1000),
            ..default()
          }),
          ..default()
        }),
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 2000,
          output: 0,
        }],
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          terms: Some(Terms {
            amount: Some(1000),
            ..default()
          }),
          timestamp: id.block,
          premine: 2000,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, 2000)])],
    );
  }

  #[test]
  fn omitted_edicts_defaults_to_mint_amount() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          terms: Some(Terms {
            offset: (None, Some(1)),
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
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          terms: Some(Terms {
            amount: None,
            offset: (None, Some(1)),
            ..default()
          }),
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn premines_can_claim_over_mint_amount() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(2000),
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(1),
            ..default()
          }),
          ..default()
        }),
        edicts: vec![Edict {
          id: RuneId::default(),
          amount: 2000,
          output: 0,
        }],
        ..default()
      },
      1,
    );

    context.assert_runes(
      [(
        id,
        RuneEntry {
          block: id.block,
          etching: txid,
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(1),
            ..default()
          }),
          timestamp: id.block,
          premine: 2000,
          mints: 0,
          ..default()
        },
      )],
      [(OutPoint { txid, vout: 0 }, vec![(id, 2000)])],
    );
  }

  #[test]
  fn transactions_cannot_claim_more_than_mint_amount() {
    let context = Context::builder().arg("--index-runes").build();

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

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 2000,
            output: 0,
          }],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          timestamp: id.block,
          mints: 1,
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
  }

  #[test]
  fn multiple_edicts_in_one_transaction_may_claim_open_mint() {
    let context = Context::builder().arg("--index-runes").build();

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
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );

    let txid1 = context.core.broadcast_template(TransactionTemplate {
      inputs: &[(2, 0, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![
            Edict {
              id,
              amount: 500,
              output: 0,
            },
            Edict {
              id,
              amount: 500,
              output: 0,
            },
            Edict {
              id,
              amount: 500,
              output: 0,
            },
          ],
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
          spaced_rune: SpacedRune {
            rune: Rune(RUNE),
            spacers: 0,
          },
          terms: Some(Terms {
            amount: Some(1000),
            cap: Some(100),
            ..default()
          }),
          timestamp: id.block,
          mints: 1,
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
  }

  #[test]
  fn commits_are_not_valid_in_non_taproot_witnesses() {
    let context = Context::builder().arg("--index-runes").build();

    let block_count = context.index.block_count().unwrap().into_usize();

    context.mine_blocks(1);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Witness::new())],
      p2tr: false,
      ..default()
    });

    context.mine_blocks(Runestone::COMMIT_CONFIRMATIONS.into());

    let mut witness = Witness::new();

    let runestone = Runestone {
      etching: Some(Etching {
        rune: Some(Rune(RUNE)),
        terms: Some(Terms {
          amount: Some(1000),
          ..default()
        }),
        ..default()
      }),
      ..default()
    };

    let tapscript = script::Builder::new()
      .push_slice::<&PushBytes>(
        runestone
          .etching
          .unwrap()
          .rune
          .unwrap()
          .commitment()
          .as_slice()
          .try_into()
          .unwrap(),
      )
      .into_script();

    witness.push(tapscript);

    witness.push([]);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count + 1, 1, 0, witness)],
      op_return: Some(runestone.encipher()),
      outputs: 1,
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes([], []);
  }

  #[test]
  fn immature_commits_are_not_valid() {
    let context = Context::builder().arg("--index-runes").build();

    let block_count = context.index.block_count().unwrap().into_usize();

    context.mine_blocks(1);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Witness::new())],
      p2tr: true,
      ..default()
    });

    context.mine_blocks((Runestone::COMMIT_CONFIRMATIONS - 2).into());

    let mut witness = Witness::new();

    let runestone = Runestone {
      etching: Some(Etching {
        rune: Some(Rune(RUNE)),
        terms: Some(Terms {
          amount: Some(1000),
          ..default()
        }),
        ..default()
      }),
      ..default()
    };

    let tapscript = script::Builder::new()
      .push_slice::<&PushBytes>(
        runestone
          .etching
          .unwrap()
          .rune
          .unwrap()
          .commitment()
          .as_slice()
          .try_into()
          .unwrap(),
      )
      .into_script();

    witness.push(tapscript);

    witness.push([]);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count + 1, 1, 0, witness)],
      op_return: Some(runestone.encipher()),
      outputs: 1,
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes([], []);
  }

  #[test]
  fn immature_commits_are_not_valid_even_when_bitcoind_is_ahead() {
    let context = Context::builder().arg("--index-runes").build();

    let block_count = context.index.block_count().unwrap().into_usize();

    context.mine_blocks_with_update(1, false);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Witness::new())],
      p2tr: true,
      ..default()
    });

    context.mine_blocks_with_update((Runestone::COMMIT_CONFIRMATIONS - 2).into(), false);

    let mut witness = Witness::new();

    let runestone = Runestone {
      etching: Some(Etching {
        rune: Some(Rune(RUNE)),
        terms: Some(Terms {
          amount: Some(1000),
          ..default()
        }),
        ..default()
      }),
      ..default()
    };

    let tapscript = script::Builder::new()
      .push_slice::<&PushBytes>(
        runestone
          .etching
          .unwrap()
          .rune
          .unwrap()
          .commitment()
          .as_slice()
          .try_into()
          .unwrap(),
      )
      .into_script();

    witness.push(tapscript);

    witness.push([]);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count + 1, 1, 0, witness)],
      op_return: Some(runestone.encipher()),
      outputs: 1,
      ..default()
    });

    context.mine_blocks_with_update(2, false);

    context.mine_blocks_with_update(1, true);

    context.assert_runes([], []);
  }

  #[test]
  fn etchings_are_not_valid_without_commitment() {
    let context = Context::builder().arg("--index-runes").build();

    let block_count = context.index.block_count().unwrap().into_usize();

    context.mine_blocks(1);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count, 0, 0, Witness::new())],
      p2tr: true,
      ..default()
    });

    context.mine_blocks(Runestone::COMMIT_CONFIRMATIONS.into());

    let mut witness = Witness::new();

    let runestone = Runestone {
      etching: Some(Etching {
        rune: Some(Rune(RUNE)),
        terms: Some(Terms {
          amount: Some(1000),
          ..default()
        }),
        ..default()
      }),
      ..default()
    };

    let tapscript = script::Builder::new()
      .push_slice::<&PushBytes>([].as_slice().try_into().unwrap())
      .into_script();

    witness.push(tapscript);

    witness.push([]);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(block_count + 1, 1, 0, witness)],
      op_return: Some(runestone.encipher()),
      outputs: 1,
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes([], []);
  }

  #[test]
  fn tx_commits_to_rune_ignores_invalid_script() {
    let context = Context::builder().arg("--index-runes").build();

    context.mine_blocks(1);

    let runestone = Runestone {
      etching: Some(Etching {
        rune: Some(Rune(RUNE)),
        terms: Some(Terms {
          amount: Some(1000),
          ..default()
        }),
        ..default()
      }),
      ..default()
    };

    let mut witness = Witness::new();

    witness.push([opcodes::all::OP_PUSHDATA4.to_u8()]);
    witness.push([]);

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(1, 0, 0, witness)],
      op_return: Some(runestone.encipher()),
      outputs: 1,
      ..default()
    });

    context.mine_blocks(1);

    context.assert_runes([], []);
  }

  #[test]
  fn edict_with_amount_zero_and_no_destinations_is_ignored() {
    let context = Context::builder().arg("--index-runes").build();

    let (txid0, id) = context.etch(
      Runestone {
        etching: Some(Etching {
          rune: Some(Rune(RUNE)),
          premine: Some(u128::MAX),
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
          premine: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [(
        OutPoint {
          txid: txid0,
          vout: 0,
        },
        vec![(id, u128::MAX)],
      )],
    );

    context.core.broadcast_template(TransactionTemplate {
      inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
      op_return: Some(
        Runestone {
          edicts: vec![Edict {
            id,
            amount: 0,
            output: 1,
          }],
          ..default()
        }
        .encipher(),
      ),
      outputs: 0,
      ..default()
    });

    context.mine_blocks(1);

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
          premine: u128::MAX,
          burned: u128::MAX,
          timestamp: id.block,
          ..default()
        },
      )],
      [],
    );
  }

  #[test]
  fn genesis_rune() {
    assert_eq!(
      Chain::Mainnet.first_rune_height(),
      SUBSIDY_HALVING_INTERVAL * 4,
    );

    Context::builder()
      .chain(Chain::Mainnet)
      .arg("--index-runes")
      .build()
      .assert_runes(
        [(
          RuneId { block: 1, tx: 0 },
          RuneEntry {
            block: 1,
            burned: 0,
            divisibility: 0,
            etching: Txid::all_zeros(),
            mints: 0,
            number: 0,
            premine: 0,
            spaced_rune: SpacedRune {
              rune: Rune(2055900680524219742),
              spacers: 128,
            },
            symbol: Some('\u{29C9}'),
            terms: Some(Terms {
              amount: Some(1),
              cap: Some(u128::MAX),
              height: (
                Some((SUBSIDY_HALVING_INTERVAL * 4).into()),
                Some((SUBSIDY_HALVING_INTERVAL * 5).into()),
              ),
              offset: (None, None),
            }),
            timestamp: 0,
            turbo: true,
          },
        )],
        [],
      );
  }
}
