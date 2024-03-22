use super::*;

const MAX_SPACERS: u32 = 0b00000111_11111111_11111111_11111111;

#[derive(Default, Serialize, Debug, PartialEq)]
pub struct Runestone {
  pub cenotaph: bool,
  pub claim: Option<RuneId>,
  pub default_output: Option<u32>,
  pub edicts: Vec<Edict>,
  pub etching: Option<Etching>,
}

struct Message {
  cenotaph: bool,
  edicts: Vec<Edict>,
  fields: HashMap<u128, VecDeque<u128>>,
}

enum Payload {
  Valid(Vec<u8>),
  Invalid,
}

impl Message {
  fn from_integers(tx: &Transaction, payload: &[u128]) -> Self {
    let mut edicts = Vec::new();
    let mut fields = HashMap::<u128, VecDeque<u128>>::new();
    let mut cenotaph = false;

    for i in (0..payload.len()).step_by(2) {
      let tag = payload[i];

      if Tag::Body == tag {
        let mut id = RuneId::default();
        for chunk in payload[i + 1..].chunks(4) {
          if chunk.len() != 4 {
            cenotaph = true;
            break;
          }

          let Some(next) = id.next(chunk[0], chunk[1]) else {
            cenotaph = true;
            break;
          };

          let Some(edict) = Edict::from_integers(tx, next, chunk[2], chunk[3]) else {
            cenotaph = true;
            break;
          };

          id = next;
          edicts.push(edict);
        }
        break;
      }

      let Some(&value) = payload.get(i + 1) else {
        cenotaph = true;
        break;
      };

      fields.entry(tag).or_default().push_back(value);
    }

    Self {
      cenotaph,
      edicts,
      fields,
    }
  }
}

impl Runestone {
  pub fn from_transaction(transaction: &Transaction) -> Option<Self> {
    Self::decipher(transaction).ok().flatten()
  }

  fn decipher(transaction: &Transaction) -> Result<Option<Self>, script::Error> {
    let payload = match Runestone::payload(transaction)? {
      Some(Payload::Valid(payload)) => payload,
      Some(Payload::Invalid) => {
        return Ok(Some(Self {
          cenotaph: true,
          ..Default::default()
        }))
      }
      None => return Ok(None),
    };

    let Some(integers) = Runestone::integers(&payload) else {
      return Ok(Some(Self {
        cenotaph: true,
        ..Default::default()
      }));
    };

    let Message {
      cenotaph,
      edicts,
      mut fields,
    } = Message::from_integers(transaction, &integers);

    let claim = Tag::Claim.take(&mut fields, |[block, tx]| {
      Some(RuneId {
        block: block.try_into().ok()?,
        tx: tx.try_into().ok()?,
      })
    });

    let deadline = Tag::Deadline.take(&mut fields, |[deadline]| u32::try_from(deadline).ok());

    let default_output = Tag::DefaultOutput.take(&mut fields, |[default_output]| {
      let default_output = u32::try_from(default_output).ok()?;
      (default_output.into_usize() < transaction.output.len()).then_some(default_output)
    });

    let divisibility = Tag::Divisibility
      .take(&mut fields, |[divisibility]| {
        let divisibility = u8::try_from(divisibility).ok()?;
        (divisibility <= MAX_DIVISIBILITY).then_some(divisibility)
      })
      .unwrap_or_default();

    let limit = Tag::Limit.take(&mut fields, |[limit]| (limit <= MAX_LIMIT).then_some(limit));

    let rune = Tag::Rune.take(&mut fields, |[rune]| Some(Rune(rune)));

    let spacers = Tag::Spacers
      .take(&mut fields, |[spacers]| {
        let spacers = u32::try_from(spacers).ok()?;
        (spacers <= MAX_SPACERS).then_some(spacers)
      })
      .unwrap_or_default();

    let symbol = Tag::Symbol.take(&mut fields, |[symbol]| {
      char::from_u32(u32::try_from(symbol).ok()?)
    });

    let term = Tag::Term.take(&mut fields, |[term]| u32::try_from(term).ok());

    let mut flags = Tag::Flags
      .take(&mut fields, |[flags]| Some(flags))
      .unwrap_or_default();

    let etch = Flag::Etch.take(&mut flags);

    let mint = Flag::Mint.take(&mut flags);

    let etching = if etch {
      Some(Etching {
        divisibility,
        rune,
        spacers,
        symbol,
        mint: mint.then_some(Mint {
          deadline,
          limit,
          term,
        }),
      })
    } else {
      None
    };

    Ok(Some(Self {
      cenotaph: cenotaph || flags != 0 || fields.keys().any(|tag| tag % 2 == 0),
      claim,
      default_output,
      edicts,
      etching,
    }))
  }

  pub fn encipher(&self) -> ScriptBuf {
    let mut payload = Vec::new();

    if let Some(etching) = self.etching {
      let mut flags = 0;
      Flag::Etch.set(&mut flags);

      if etching.mint.is_some() {
        Flag::Mint.set(&mut flags);
      }

      Tag::Flags.encode([flags], &mut payload);

      if let Some(rune) = etching.rune {
        Tag::Rune.encode([rune.0], &mut payload);
      }

      if etching.divisibility != 0 {
        Tag::Divisibility.encode([etching.divisibility.into()], &mut payload);
      }

      if etching.spacers != 0 {
        Tag::Spacers.encode([etching.spacers.into()], &mut payload);
      }

      if let Some(symbol) = etching.symbol {
        Tag::Symbol.encode([symbol.into()], &mut payload);
      }

      if let Some(mint) = etching.mint {
        if let Some(deadline) = mint.deadline {
          Tag::Deadline.encode([deadline.into()], &mut payload);
        }

        if let Some(limit) = mint.limit {
          Tag::Limit.encode([limit], &mut payload);
        }

        if let Some(term) = mint.term {
          Tag::Term.encode([term.into()], &mut payload);
        }
      }
    }

    if let Some(RuneId { block, tx }) = self.claim {
      Tag::Claim.encode([block.into(), tx.into()], &mut payload);
    }

    if let Some(default_output) = self.default_output {
      Tag::DefaultOutput.encode([default_output.into()], &mut payload);
    }

    if self.cenotaph {
      Tag::Cenotaph.encode([0], &mut payload);
    }

    if !self.edicts.is_empty() {
      varint::encode_to_vec(Tag::Body.into(), &mut payload);

      let mut edicts = self.edicts.clone();
      edicts.sort_by_key(|edict| edict.id);

      let mut previous = RuneId::default();
      for edict in edicts {
        let (block, tx) = previous.delta(edict.id).unwrap();
        varint::encode_to_vec(block, &mut payload);
        varint::encode_to_vec(tx, &mut payload);
        varint::encode_to_vec(edict.amount, &mut payload);
        varint::encode_to_vec(edict.output.into(), &mut payload);
        previous = edict.id;
      }
    }

    let mut builder = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_opcode(MAGIC_NUMBER);

    for chunk in payload.chunks(MAX_SCRIPT_ELEMENT_SIZE) {
      let push: &script::PushBytes = chunk.try_into().unwrap();
      builder = builder.push_slice(push);
    }

    builder.into_script()
  }

  fn payload(transaction: &Transaction) -> Result<Option<Payload>, script::Error> {
    // search transaction outputs for payload
    for output in &transaction.output {
      let mut instructions = output.script_pubkey.instructions();

      // payload starts with OP_RETURN
      if instructions.next().transpose()? != Some(Instruction::Op(opcodes::all::OP_RETURN)) {
        continue;
      }

      // followed by the protocol identifier
      if instructions.next().transpose()? != Some(Instruction::Op(MAGIC_NUMBER)) {
        continue;
      }

      // construct the payload by concatinating remaining data pushes
      let mut payload = Vec::new();

      for result in instructions {
        if let Instruction::PushBytes(push) = result? {
          payload.extend_from_slice(push.as_bytes());
        } else {
          return Ok(Some(Payload::Invalid));
        }
      }

      return Ok(Some(Payload::Valid(payload)));
    }

    Ok(None)
  }

  fn integers(payload: &[u8]) -> Option<Vec<u128>> {
    let mut integers = Vec::new();
    let mut i = 0;

    while i < payload.len() {
      let (integer, length) = varint::decode(&payload[i..])?;
      integers.push(integer);
      i += length;
    }

    Some(integers)
  }
}

#[cfg(test)]
mod tests {
  use {super::*, bitcoin::script::PushBytes};

  fn decipher(integers: &[u128]) -> Runestone {
    let payload = payload(integers);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    Runestone::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .push_opcode(MAGIC_NUMBER)
          .push_slice(payload)
          .into_script(),
        value: 0,
      }],
      lock_time: LockTime::ZERO,
      version: 2,
    })
    .unwrap()
    .unwrap()
  }

  fn payload(integers: &[u128]) -> Vec<u8> {
    let mut payload = Vec::new();

    for integer in integers {
      payload.extend(varint::encode(*integer));
    }

    payload
  }

  #[test]
  fn from_transaction_returns_none_if_decipher_returns_error() {
    assert_eq!(
      Runestone::from_transaction(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: ScriptBuf::from_bytes(vec![opcodes::all::OP_PUSHBYTES_4.to_u8()]),
          value: 0,
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      None
    );
  }

  #[test]
  fn deciphering_transaction_with_no_outputs_returns_none() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: Vec::new(),
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(None)
    );
  }

  #[test]
  fn deciphering_transaction_with_non_op_return_output_returns_none() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new().push_slice([]).into_script(),
          value: 0
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(None)
    );
  }

  #[test]
  fn deciphering_transaction_with_bare_op_return_returns_none() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .into_script(),
          value: 0
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(None)
    );
  }

  #[test]
  fn deciphering_transaction_with_non_matching_op_return_returns_none() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_slice(b"FOOO")
            .into_script(),
          value: 0
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(None)
    );
  }

  #[test]
  fn deciphering_valid_runestone_with_invalid_script_returns_script_error() {
    Runestone::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: ScriptBuf::from_bytes(vec![opcodes::all::OP_PUSHBYTES_4.to_u8()]),
        value: 0,
      }],
      lock_time: LockTime::ZERO,
      version: 2,
    })
    .unwrap_err();
  }

  #[test]
  fn deciphering_valid_runestone_with_invalid_script_postfix_returns_script_error() {
    let mut script_pubkey = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_opcode(MAGIC_NUMBER)
      .into_script()
      .into_bytes();

    script_pubkey.push(opcodes::all::OP_PUSHBYTES_4.to_u8());

    Runestone::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: ScriptBuf::from_bytes(script_pubkey),
        value: 0,
      }],
      lock_time: LockTime::ZERO,
      version: 2,
    })
    .unwrap_err();
  }

  #[test]
  fn deciphering_runestone_with_truncated_varint_succeeds() {
    Runestone::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .push_opcode(MAGIC_NUMBER)
          .push_slice([128])
          .into_script(),
        value: 0,
      }],
      lock_time: LockTime::ZERO,
      version: 2,
    })
    .unwrap();
  }

  #[test]
  fn outputs_with_non_pushdata_opcodes_are_cenotaph() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(MAGIC_NUMBER)
              .push_opcode(opcodes::all::OP_VERIFY)
              .push_slice([0])
              .push_slice::<&script::PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice::<&script::PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice([2, 0])
              .into_script(),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(MAGIC_NUMBER)
              .push_slice([0])
              .push_slice::<&script::PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice::<&script::PushBytes>(varint::encode(2).as_slice().try_into().unwrap())
              .push_slice([3, 0])
              .into_script(),
            value: 0,
          },
        ],
        lock_time: LockTime::ZERO,
        version: 2,
      })
      .unwrap()
      .unwrap(),
      Runestone {
        cenotaph: true,
        ..Default::default()
      }
    );
  }

  #[test]
  fn deciphering_empty_runestone_is_successful() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(MAGIC_NUMBER)
            .into_script(),
          value: 0
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(Some(Runestone::default()))
    );
  }

  #[test]
  fn error_in_input_aborts_search_for_runestone() {
    let payload = payload(&[0, 1, 2, 3]);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    let script_pubkey = vec![
      opcodes::all::OP_RETURN.to_u8(),
      opcodes::all::OP_PUSHBYTES_9.to_u8(),
      MAGIC_NUMBER.to_u8(),
      opcodes::all::OP_PUSHBYTES_4.to_u8(),
    ];

    Runestone::decipher(&Transaction {
      input: Vec::new(),
      output: vec![
        TxOut {
          script_pubkey: ScriptBuf::from_bytes(script_pubkey),
          value: 0,
        },
        TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(MAGIC_NUMBER)
            .push_slice(payload)
            .into_script(),
          value: 0,
        },
      ],
      lock_time: LockTime::ZERO,
      version: 2,
    })
    .unwrap_err();
  }

  #[test]
  fn deciphering_non_empty_runestone_is_successful() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        ..Default::default()
      }
    );
  }

  #[test]
  fn decipher_etching() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching::default()),
        ..Default::default()
      }
    );
  }

  #[test]
  fn decipher_etching_with_rune() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Rune.into(),
        4,
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn etch_flag_is_required_to_etch_rune_even_if_mint_is_set() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Mint.mask(),
        Tag::Term.into(),
        4,
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        ..Default::default()
      },
    );
  }

  #[test]
  fn decipher_etching_with_term() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask() | Flag::Mint.mask(),
        Tag::Term.into(),
        4,
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          mint: Some(Mint {
            term: Some(4),
            ..Default::default()
          }),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn decipher_etching_with_limit() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask() | Flag::Mint.mask(),
        Tag::Limit.into(),
        4,
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          mint: Some(Mint {
            limit: Some(4),
            ..Default::default()
          }),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn invalid_varint_produces_cenotaph() {
    pretty_assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(MAGIC_NUMBER)
            .push_slice([128])
            .into_script(),
          value: 0,
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      })
      .unwrap()
      .unwrap(),
      Runestone {
        cenotaph: true,
        ..Default::default()
      }
    );
  }

  #[test]
  fn duplicate_even_tags_produce_cenotaph() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Rune.into(),
        4,
        Tag::Rune.into(),
        5,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          ..Default::default()
        }),
        cenotaph: true,
        ..Default::default()
      }
    );
  }

  #[test]
  fn duplicate_odd_tags_are_ignored() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Divisibility.into(),
        4,
        Tag::Divisibility.into(),
        5,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: None,
          divisibility: 4,
          ..Default::default()
        }),
        ..Default::default()
      }
    );
  }

  #[test]
  fn unrecognized_odd_tag_is_ignored() {
    assert_eq!(
      decipher(&[Tag::Nop.into(), 100, Tag::Body.into(), 1, 1, 2, 0]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestone_with_unrecognized_even_tag_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Cenotaph.into(), 0, Tag::Body.into(), 1, 1, 2, 0]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestone_with_unrecognized_flag_is_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Cenotaph.mask(),
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestone_with_edict_id_with_zero_block_and_nonzero_tx_is_cenotaph() {
    pretty_assert_eq!(
      decipher(&[Tag::Body.into(), 0, 1, 2, 0]),
      Runestone {
        edicts: Vec::new(),
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestone_with_output_over_max_is_cenotaph() {
    pretty_assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 2]),
      Runestone {
        edicts: Vec::new(),
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn tag_with_no_value_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Flags.into(), 1, Tag::Flags.into()]),
      Runestone {
        etching: Some(Etching::default()),
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn trailing_integers_in_body_is_cenotaph() {
    let mut integers = vec![Tag::Body.into(), 1, 1, 2, 0];

    for i in 0..4 {
      pretty_assert_eq!(
        decipher(&integers),
        Runestone {
          cenotaph: i > 0,
          edicts: vec![Edict {
            id: rune_id(1),
            amount: 2,
            output: 0,
          }],
          ..Default::default()
        }
      );

      integers.push(0);
    }
  }

  #[test]
  fn decipher_etching_with_divisibility() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Rune.into(),
        4,
        Tag::Divisibility.into(),
        5,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          divisibility: 5,
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn divisibility_above_max_is_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Rune.into(),
        4,
        Tag::Divisibility.into(),
        (MAX_DIVISIBILITY + 1).into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn symbol_above_max_is_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Symbol.into(),
        u128::from(u32::from(char::MAX) + 1),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching::default()),
        ..Default::default()
      },
    );
  }

  #[test]
  fn decipher_etching_with_symbol() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Rune.into(),
        4,
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          symbol: Some('a'),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn decipher_etching_with_all_etching_tags() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask() | Flag::Mint.mask(),
        Tag::Rune.into(),
        4,
        Tag::Deadline.into(),
        7,
        Tag::Divisibility.into(),
        1,
        Tag::Spacers.into(),
        5,
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Term.into(),
        2,
        Tag::Limit.into(),
        3,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          mint: Some(Mint {
            deadline: Some(7),
            term: Some(2),
            limit: Some(3),
          }),
          divisibility: 1,
          symbol: Some('a'),
          spacers: 5,
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn recognized_even_etching_fields_in_non_etching_are_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Rune.into(),
        4,
        Tag::Divisibility.into(),
        1,
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Term.into(),
        2,
        Tag::Limit.into(),
        3,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        ..Default::default()
      },
    );
  }

  #[test]
  fn decipher_etching_with_divisibility_and_symbol() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Rune.into(),
        4,
        Tag::Divisibility.into(),
        1,
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          rune: Some(Rune(4)),
          divisibility: 1,
          symbol: Some('a'),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
  }

  #[test]
  fn tag_values_are_not_parsed_as_tags() {
    pretty_assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Divisibility.into(),
        Tag::Body.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching::default()),
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestone_may_contain_multiple_edicts() {
    pretty_assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, 0, 3, 5, 0]),
      Runestone {
        edicts: vec![
          Edict {
            id: rune_id(1),
            amount: 2,
            output: 0,
          },
          Edict {
            id: rune_id(4),
            amount: 5,
            output: 0,
          },
        ],
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestones_with_invalid_rune_id_blocks_are_cenotaph() {
    pretty_assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, u128::MAX, 1, 0, 0,]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn runestones_with_invalid_rune_id_txs_are_cenotaph() {
    pretty_assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, 1, u128::MAX, 0, 0,]),
      Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn payload_pushes_are_concatenated() {
    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(MAGIC_NUMBER)
            .push_slice::<&PushBytes>(
              varint::encode(Tag::Flags.into())
                .as_slice()
                .try_into()
                .unwrap()
            )
            .push_slice::<&PushBytes>(
              varint::encode(Flag::Etch.mask())
                .as_slice()
                .try_into()
                .unwrap()
            )
            .push_slice::<&PushBytes>(
              varint::encode(Tag::Divisibility.into())
                .as_slice()
                .try_into()
                .unwrap()
            )
            .push_slice::<&PushBytes>(varint::encode(5).as_slice().try_into().unwrap())
            .push_slice::<&PushBytes>(
              varint::encode(Tag::Body.into())
                .as_slice()
                .try_into()
                .unwrap()
            )
            .push_slice::<&script::PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
            .push_slice::<&script::PushBytes>(varint::encode(1).as_slice().try_into().unwrap())
            .push_slice::<&PushBytes>(varint::encode(2).as_slice().try_into().unwrap())
            .push_slice::<&PushBytes>(varint::encode(0).as_slice().try_into().unwrap())
            .into_script(),
          value: 0
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(Some(Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        etching: Some(Etching {
          divisibility: 5,
          ..Default::default()
        }),
        ..Default::default()
      }))
    );
  }

  #[test]
  fn runestone_may_be_in_second_output() {
    let payload = payload(&[0, 1, 1, 2, 0]);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: ScriptBuf::new(),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: 0
          }
        ],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(Some(Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        ..Default::default()
      }))
    );
  }

  #[test]
  fn runestone_may_be_after_non_matching_op_return() {
    let payload = payload(&[0, 1, 1, 2, 0]);

    let payload: &PushBytes = payload.as_slice().try_into().unwrap();

    assert_eq!(
      Runestone::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_slice(b"FOO")
              .into_script(),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: 0
          }
        ],
        lock_time: LockTime::ZERO,
        version: 2,
      }),
      Ok(Some(Runestone {
        edicts: vec![Edict {
          id: rune_id(1),
          amount: 2,
          output: 0,
        }],
        ..Default::default()
      }))
    );
  }

  #[test]
  fn runestone_size() {
    #[track_caller]
    fn case(edicts: Vec<Edict>, etching: Option<Etching>, size: usize) {
      assert_eq!(
        Runestone {
          edicts,
          etching,
          ..Default::default()
        }
        .encipher()
        .len(),
        size
      );
    }

    case(Vec::new(), None, 2);

    case(
      Vec::new(),
      Some(Etching {
        rune: Some(Rune(0)),
        ..Default::default()
      }),
      7,
    );

    case(
      Vec::new(),
      Some(Etching {
        divisibility: MAX_DIVISIBILITY,
        rune: Some(Rune(0)),
        ..Default::default()
      }),
      9,
    );

    case(
      Vec::new(),
      Some(Etching {
        divisibility: MAX_DIVISIBILITY,
        mint: Some(Mint {
          deadline: Some(10000),
          limit: Some(1),
          term: Some(1),
        }),
        rune: Some(Rune(0)),
        symbol: Some('$'),
        spacers: 1,
      }),
      20,
    );

    case(
      Vec::new(),
      Some(Etching {
        rune: Some(Rune(u128::MAX)),
        ..Default::default()
      }),
      25,
    );

    case(
      vec![Edict {
        amount: 0,
        id: RuneId { block: 0, tx: 0 },
        output: 0,
      }],
      Some(Etching {
        divisibility: MAX_DIVISIBILITY,
        rune: Some(Rune(u128::MAX)),
        ..Default::default()
      }),
      32,
    );

    case(
      vec![Edict {
        amount: u128::MAX,
        id: RuneId { block: 0, tx: 0 },
        output: 0,
      }],
      Some(Etching {
        divisibility: MAX_DIVISIBILITY,
        rune: Some(Rune(u128::MAX)),
        ..Default::default()
      }),
      50,
    );

    case(
      vec![Edict {
        amount: 0,
        id: RuneId {
          block: 1_000_000,
          tx: u32::MAX,
        },
        output: 0,
      }],
      None,
      14,
    );

    case(
      vec![Edict {
        amount: u128::MAX,
        id: RuneId {
          block: 1_000_000,
          tx: u32::MAX,
        },
        output: 0,
      }],
      None,
      32,
    );

    case(
      vec![
        Edict {
          amount: u128::MAX,
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
        Edict {
          amount: u128::MAX,
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
      ],
      None,
      54,
    );

    case(
      vec![
        Edict {
          amount: u128::MAX,
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
        Edict {
          amount: u128::MAX,
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
        Edict {
          amount: u128::MAX,
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
      ],
      None,
      76,
    );

    case(
      vec![
        Edict {
          amount: u64::MAX.into(),
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        };
        4
      ],
      None,
      62,
    );

    case(
      vec![
        Edict {
          amount: u64::MAX.into(),
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        };
        5
      ],
      None,
      75,
    );

    case(
      vec![
        Edict {
          amount: u64::MAX.into(),
          id: RuneId {
            block: 0,
            tx: u32::MAX,
          },
          output: 0,
        };
        5
      ],
      None,
      73,
    );

    case(
      vec![
        Edict {
          amount: 1_000_000_000_000_000_000,
          id: RuneId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        };
        5
      ],
      None,
      70,
    );
  }

  #[test]
  fn etching_with_term_greater_than_maximum_is_still_an_etching() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Etch.mask(),
        Tag::Term.into(),
        u128::from(u64::MAX) + 1,
      ]),
      Runestone {
        etching: Some(Etching::default()),
        cenotaph: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn encipher() {
    #[track_caller]
    fn case(runestone: Runestone, expected: &[u128]) {
      let script_pubkey = runestone.encipher();

      let transaction = Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey,
          value: 0,
        }],
        lock_time: LockTime::ZERO,
        version: 2,
      };

      let Payload::Valid(payload) = Runestone::payload(&transaction).unwrap().unwrap() else {
        panic!("invalid payload")
      };

      pretty_assert_eq!(Runestone::integers(&payload).unwrap(), expected);

      let runestone = {
        let mut edicts = runestone.edicts;
        edicts.sort_by_key(|edict| edict.id);
        Runestone {
          edicts,
          ..runestone
        }
      };

      pretty_assert_eq!(
        Runestone::from_transaction(&transaction).unwrap(),
        runestone
      );
    }

    case(Runestone::default(), &[]);

    case(
      Runestone {
        etching: Some(Etching {
          divisibility: 1,
          mint: Some(Mint {
            deadline: Some(2),
            limit: Some(3),
            term: Some(5),
          }),
          symbol: Some('@'),
          rune: Some(Rune(4)),
          spacers: 6,
        }),
        edicts: vec![
          Edict {
            amount: 8,
            id: rune_id(9),
            output: 0,
          },
          Edict {
            amount: 5,
            id: rune_id(6),
            output: 1,
          },
        ],
        default_output: Some(0),
        cenotaph: true,
        claim: Some(rune_id(12)),
      },
      &[
        Tag::Flags.into(),
        Flag::Etch.mask() | Flag::Mint.mask(),
        Tag::Rune.into(),
        4,
        Tag::Divisibility.into(),
        1,
        Tag::Spacers.into(),
        6,
        Tag::Symbol.into(),
        '@'.into(),
        Tag::Deadline.into(),
        2,
        Tag::Limit.into(),
        3,
        Tag::Term.into(),
        5,
        Tag::Claim.into(),
        1,
        Tag::Claim.into(),
        12,
        Tag::DefaultOutput.into(),
        0,
        Tag::Cenotaph.into(),
        0,
        Tag::Body.into(),
        1,
        6,
        5,
        1,
        0,
        3,
        8,
        0,
      ],
    );

    case(
      Runestone {
        etching: Some(Etching {
          divisibility: 0,
          mint: None,
          symbol: None,
          rune: Some(Rune(3)),
          spacers: 0,
        }),
        cenotaph: false,
        ..Default::default()
      },
      &[Tag::Flags.into(), Flag::Etch.mask(), Tag::Rune.into(), 3],
    );

    case(
      Runestone {
        etching: Some(Etching {
          divisibility: 0,
          mint: None,
          symbol: None,
          rune: None,
          spacers: 0,
        }),
        cenotaph: false,
        ..Default::default()
      },
      &[Tag::Flags.into(), Flag::Etch.mask()],
    );

    case(
      Runestone {
        cenotaph: true,
        ..Default::default()
      },
      &[Tag::Cenotaph.into(), 0],
    );
  }

  #[test]
  fn runestone_payload_is_chunked() {
    let script = Runestone {
      edicts: vec![
        Edict {
          id: RuneId::default(),
          amount: 0,
          output: 0
        };
        129
      ],
      ..Default::default()
    }
    .encipher();

    assert_eq!(script.instructions().count(), 3);

    let script = Runestone {
      edicts: vec![
        Edict {
          id: RuneId::default(),
          amount: 0,
          output: 0
        };
        130
      ],
      ..Default::default()
    }
    .encipher();

    assert_eq!(script.instructions().count(), 4);
  }

  #[test]
  fn max_spacers() {
    let mut rune = String::new();

    for (i, c) in Rune(u128::MAX).to_string().chars().enumerate() {
      if i > 0 {
        rune.push('•');
      }

      rune.push(c);
    }

    assert_eq!(MAX_SPACERS, rune.parse::<SpacedRune>().unwrap().spacers);
  }

  #[test]
  fn edict_output_greater_than_32_max_produces_cenotaph() {
    assert!(decipher(&[Tag::Body.into(), 1, 1, 1, u128::from(u32::MAX) + 1]).cenotaph);
  }

  #[test]
  fn invalid_claim_produces_cenotaph() {
    assert!(decipher(&[Tag::Claim.into(), 1]).cenotaph);
  }

  #[test]
  fn invalid_deadline_produces_cenotaph() {
    assert!(decipher(&[Tag::Deadline.into(), u128::MAX]).cenotaph);
  }

  #[test]
  fn invalid_default_output_produces_cenotaph() {
    assert!(decipher(&[Tag::DefaultOutput.into(), 1]).cenotaph);
    assert!(decipher(&[Tag::DefaultOutput.into(), u128::MAX]).cenotaph);
  }

  #[test]
  fn invalid_divisibility_does_not_produce_cenotaph() {
    assert!(!decipher(&[Tag::Divisibility.into(), u128::MAX]).cenotaph);
  }

  #[test]
  fn invalid_limit_produces_cenotaph() {
    assert!(decipher(&[Tag::Limit.into(), u128::MAX]).cenotaph);
    assert!(decipher(&[Tag::Limit.into(), u128::from(u64::MAX) + 1]).cenotaph);
  }

  #[test]
  fn min_and_max_runes_are_not_cenotaphs() {
    assert!(!decipher(&[Tag::Rune.into(), 0]).cenotaph);
    assert!(!decipher(&[Tag::Rune.into(), u128::MAX]).cenotaph);
  }

  #[test]
  fn invalid_spacers_does_not_produce_cenotaph() {
    assert!(!decipher(&[Tag::Spacers.into(), u128::MAX]).cenotaph);
  }

  #[test]
  fn invalid_symbol_does_not_produce_cenotaph() {
    assert!(!decipher(&[Tag::Symbol.into(), u128::MAX]).cenotaph);
  }

  #[test]
  fn invalid_term_produces_cenotaph() {
    assert!(decipher(&[Tag::Term.into(), u128::MAX]).cenotaph);
  }
}
