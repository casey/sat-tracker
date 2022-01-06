use std::fs::File;
use std::io;
use std::io::{Seek, SeekFrom, Write};
use bitcoin::{Block, BlockHeader, Network, OutPoint, Transaction, TxIn, TxOut};
use bitcoin::blockdata::constants::{COIN_VALUE, genesis_block, MAX_SEQUENCE};
use bitcoin::blockdata::script;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::sha256d;
use {
  executable_path::executable_path,
  std::{error::Error, process::Command, str},
};

type Result = std::result::Result<(), Box<dyn Error>>;

fn generate_transaction(height: usize) -> Transaction {
  // Base
  let mut ret = Transaction {
    version: 1,
    lock_time: 0,
    input: vec![],
    output: vec![],
  };

  // Inputs
  let in_script = script::Builder::new().push_scriptint(height as i64)
      .into_script();
  ret.input.push(TxIn {
    previous_output: OutPoint::null(),
    script_sig: in_script,
    sequence: MAX_SEQUENCE,
    witness: vec![],
  });

  // Outputs
  let out_script = script::Builder::new().into_script();
  ret.output.push(TxOut {
    value: 50 * COIN_VALUE,
    script_pubkey: out_script
  });

  // end
  ret
}

fn serialize_block(output: &mut File, block: &Block) -> io::Result<()> {
  output.write(&[0xf9, 0xbe, 0xb4, 0xd9])?;
  let size_field = output.stream_position()?;
  output.write(&[0u8; 4])?;
  let size = block.consensus_encode(&mut *output)?;
  output.seek(SeekFrom::Start(size_field))?;
  output.write(&(size as u32).to_le_bytes())?;
  output.seek(SeekFrom::Current(size as i64))?;

  Ok(())
}

fn populate_blockfile(mut output: File, height: usize) -> io::Result<()> {
  let genesis = genesis_block(Network::Bitcoin);
  serialize_block(&mut output, &genesis)?;

  let mut prev_block = genesis.clone();
  for _ in 1..=height {
    let tx = generate_transaction(height);
    let hash: sha256d::Hash = tx.txid().into();
    let merkle_root = hash.into();
    let block = Block {
      header: BlockHeader {
        version: 0,
        prev_blockhash: prev_block.block_hash(),
        merkle_root,
        time: 0,
        bits: 0,
        nonce: 0
      },
      txdata: vec![tx],
    };

    serialize_block(&mut output, &block)?;
    prev_block = block;
  }

  Ok(())
}

#[test]
fn find_satoshi_zero() -> Result {
  let tmpdir = tempfile::tempdir()?;
  populate_blockfile(File::create(tmpdir.path().join("blk00000.dat"))?, 0)?;
  let output = Command::new(executable_path("bitcoin-atoms"))
    .args(["find-satoshi", "--blocksdir", tmpdir.path().to_str().unwrap(), "0", "0"])
    .output()?;

  if !output.status.success() {
    panic!(
      "Command failed {}: {}",
      output.status,
      str::from_utf8(&output.stderr)?
    );
  }

  assert_eq!(
    str::from_utf8(&output.stdout)?,
    "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b:0\n"
  );

  Ok(())
}

#[test]
fn find_first_satoshi_of_second_block() -> Result {
  let tmpdir = tempfile::tempdir()?;
  populate_blockfile(File::create(tmpdir.path().join("blk00000.dat"))?, 1)?;
  let output = Command::new(executable_path("bitcoin-atoms"))
    .args(["find-satoshi", "--blocksdir", tmpdir.path().to_str().unwrap(), "5000000000", "1"])
    .output()?;

  if !output.status.success() {
    panic!(
      "Command failed {}: {}",
      output.status,
      str::from_utf8(&output.stderr)?
    );
  }

  assert_eq!(
    str::from_utf8(&output.stdout)?,
    "e5fb252959bdc7727c80296dbc53e1583121503bb2e266a609ebc49cf2a74c1d:0\n",
  );

  Ok(())
}
