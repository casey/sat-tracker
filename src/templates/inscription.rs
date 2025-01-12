use super::*;

#[derive(Boilerplate, Default)]
pub struct InscriptionHtml {
  pub chain: Chain,
  pub charms: u16,
  pub child_count: u64,
  pub children: Vec<InscriptionId>,
  pub fee: u64,
  pub height: u32,
  pub inscription: Inscription,
  pub id: InscriptionId,
  pub number: i32,
  pub next: Option<InscriptionId>,
  pub output: Option<TxOut>,
  pub parents: Vec<InscriptionId>,
  pub previous: Option<InscriptionId>,
  pub rune: Option<SpacedRune>,
  pub sat: Option<Sat>,
  pub satpoint: SatPoint,
  pub timestamp: DateTime<Utc>,
}

impl PageContent for InscriptionHtml {
  fn title(&self) -> String {
    format!("Inscription {}", self.number)
  }
}

impl InscriptionHtml {
  pub fn burn_metadata(&self) -> Option<Value> {
    let script_pubkey = &self.output.as_ref()?.script_pubkey;

    if !script_pubkey.is_op_return() {
      return None;
    }

    let script::Instruction::PushBytes(metadata) = script_pubkey.instructions().nth(1)?.ok()?
    else {
      return None;
    };

    ciborium::from_reader(Cursor::new(metadata)).ok()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn without_sat_nav_links_or_output() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          <dt>id</dt>
          <dd class=collapse>1{64}i1</dd>
          <dt>preview</dt>
          <dd><a href=/preview/1{64}i1>link</a></dd>
          <dt>content</dt>
          <dd><a href=/content/1{64}i1>link</a></dd>
          <dt>content length</dt>
          <dd>10 bytes</dd>
          <dt>content type</dt>
          <dd>text/plain;charset=utf-8</dd>
          <dt>timestamp</dt>
          <dd><time>1970-01-01 00:00:00 UTC</time></dd>
          <dt>height</dt>
          <dd><a href=/block/0>0</a></dd>
          <dt>fee</dt>
          <dd>1</dd>
          <dt>reveal transaction</dt>
          <dd><a class=collapse href=/tx/1{64}>1{64}</a></dd>
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/1{64}:1:0>1{64}:1:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/1{64}:1>1{64}:1</a></dd>
          <dt>offset</dt>
          <dd>0</dd>
          <dt>details</dt>
          <dd>
            <details>
              <summary>...</summary>
              <dl>
                <dt>ethereum teleburn address</dt>
                <dd class=collapse>0xa1DfBd1C519B9323FD7Fd8e498Ac16c2E502F059</dd>
              </dl>
            </details>
          </dd>
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_output() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        output: Some(tx_out(1, address(0))),
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        .*<h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          .*
          <dt>address</dt>
          <dd><a class=collapse href=/address/bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4>bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4</a></dd>
          <dt>value</dt>
          <dd>1</dd>
          .*
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_sat() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        output: Some(tx_out(1, address(0))),
        sat: Some(Sat(1)),
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        .*
        <dl>
          .*
          <dt>sat</dt>
          <dd><a href=/sat/1>1</a></dd>
          <dt>sat name</dt>
          <dd><a href=/sat/nvtdijuwxlo>nvtdijuwxlo</a></dd>
          <dt>preview</dt>
          .*
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_prev_and_next() {
    assert_regex_match!(
      InscriptionHtml {
        children: Vec::new(),
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(2),
        next: Some(inscription_id(3)),
        number: 1,
        output: Some(tx_out(1, address(0))),
        previous: Some(inscription_id(1)),
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <a class=prev href=/inscription/1{64}i1>❮</a>
        <iframe .* src=/preview/2{64}i2></iframe>
        <a class=next href=/inscription/3{64}i3>❯</a>
        </div>
        .*
      "
      .unindent()
    );
  }

  #[test]
  fn with_cursed_and_unbound() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(2),
        number: -1,
        output: Some(tx_out(1, address(0))),
        satpoint: SatPoint {
          outpoint: unbound_outpoint(),
          offset: 0
        },
        timestamp: timestamp(0),
        ..default()
      },
      "
        <h1>Inscription -1</h1>
        .*
        <dl>
          .*
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/0{64}:0:0>0{64}:0:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/0{64}:0>0{64}:0</a></dd>
          .*
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_parent() {
    assert_regex_match!(
      InscriptionHtml {
        parents: vec![inscription_id(2)],
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          <dt>parents</dt>
          <dd>
            <div class=thumbnails>
              <a href=/inscription/2{64}i2><iframe .* src=/preview/2{64}i2></iframe></a>
            </div>
            <div class=center>
              <a href=/parents/1{64}i1>all</a>
            </div>
          </dd>
          <dt>id</dt>
          <dd class=collapse>1{64}i1</dd>
          <dt>preview</dt>
          <dd><a href=/preview/1{64}i1>link</a></dd>
          <dt>content</dt>
          <dd><a href=/content/1{64}i1>link</a></dd>
          <dt>content length</dt>
          <dd>10 bytes</dd>
          <dt>content type</dt>
          <dd>text/plain;charset=utf-8</dd>
          <dt>timestamp</dt>
          <dd><time>1970-01-01 00:00:00 UTC</time></dd>
          <dt>height</dt>
          <dd><a href=/block/0>0</a></dd>
          <dt>fee</dt>
          <dd>1</dd>
          <dt>reveal transaction</dt>
          <dd><a class=collapse href=/tx/1{64}>1{64}</a></dd>
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/1{64}:1:0>1{64}:1:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/1{64}:1>1{64}:1</a></dd>
          <dt>offset</dt>
          <dd>0</dd>
          <dt>details</dt>
          <dd>
            <details>
              <summary>...</summary>
              <dl>
                <dt>ethereum teleburn address</dt>
                <dd class=collapse>0xa1DfBd1C519B9323FD7Fd8e498Ac16c2E502F059</dd>
              </dl>
            </details>
          </dd>
        </dl>
"
      .unindent()
    );
  }

  #[test]
  fn with_children() {
    assert_regex_match!(
      InscriptionHtml {
        child_count: 2,
        children: vec![inscription_id(2), inscription_id(3)],
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          <dt>children</dt>
          <dd>
            <div class=thumbnails>
              <a href=/inscription/2{64}i2><iframe .* src=/preview/2{64}i2></iframe></a>
              <a href=/inscription/3{64}i3><iframe .* src=/preview/3{64}i3></iframe></a>
            </div>
            <div class=center>
              <a href=/children/1{64}i1>all \\(2\\)</a>
            </div>
          </dd>
          <dt>id</dt>
          <dd class=collapse>1{64}i1</dd>
          <dt>preview</dt>
          <dd><a href=/preview/1{64}i1>link</a></dd>
          <dt>content</dt>
          <dd><a href=/content/1{64}i1>link</a></dd>
          <dt>content length</dt>
          <dd>10 bytes</dd>
          <dt>content type</dt>
          <dd>text/plain;charset=utf-8</dd>
          <dt>timestamp</dt>
          <dd><time>1970-01-01 00:00:00 UTC</time></dd>
          <dt>height</dt>
          <dd><a href=/block/0>0</a></dd>
          <dt>fee</dt>
          <dd>1</dd>
          <dt>reveal transaction</dt>
          <dd><a class=collapse href=/tx/1{64}>1{64}</a></dd>
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/1{64}:1:0>1{64}:1:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/1{64}:1>1{64}:1</a></dd>
          <dt>offset</dt>
          <dd>0</dd>
          <dt>details</dt>
          <dd>
            <details>
              <summary>...</summary>
              <dl>
                <dt>ethereum teleburn address</dt>
                <dd class=collapse>0xa1DfBd1C519B9323FD7Fd8e498Ac16c2E502F059</dd>
              </dl>
            </details>
          </dd>
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_paginated_children() {
    assert_regex_match!(
      InscriptionHtml {
        child_count: 1,
        children: vec![inscription_id(2)],
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          <dt>children</dt>
          <dd>
            <div class=thumbnails>
              <a href=/inscription/2{64}i2><iframe .* src=/preview/2{64}i2></iframe></a>
            </div>
            <div class=center>
              <a href=/children/1{64}i1>all \\(1\\)</a>
            </div>
          </dd>
          <dt>id</dt>
          <dd class=collapse>1{64}i1</dd>
          <dt>preview</dt>
          <dd><a href=/preview/1{64}i1>link</a></dd>
          <dt>content</dt>
          <dd><a href=/content/1{64}i1>link</a></dd>
          <dt>content length</dt>
          <dd>10 bytes</dd>
          <dt>content type</dt>
          <dd>text/plain;charset=utf-8</dd>
          <dt>timestamp</dt>
          <dd><time>1970-01-01 00:00:00 UTC</time></dd>
          <dt>height</dt>
          <dd><a href=/block/0>0</a></dd>
          <dt>fee</dt>
          <dd>1</dd>
          <dt>reveal transaction</dt>
          <dd><a class=collapse href=/tx/1{64}>1{64}</a></dd>
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/1{64}:1:0>1{64}:1:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/1{64}:1>1{64}:1</a></dd>
          <dt>offset</dt>
          <dd>0</dd>
          <dt>details</dt>
          <dd>
            <details>
              <summary>...</summary>
              <dl>
                <dt>ethereum teleburn address</dt>
                <dd class=collapse>0xa1DfBd1C519B9323FD7Fd8e498Ac16c2E502F059</dd>
              </dl>
            </details>
          </dd>
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_rune() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: inscription("text/plain;charset=utf-8", "HELLOWORLD"),
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        rune: Some(SpacedRune {
          rune: Rune(26),
          spacers: 1
        }),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        .*
        <dl>
          <dt>rune</dt>
          <dd><a href=/rune/A•A>A•A</a></dd>
          .*
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_content_encoding() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: Inscription {
          content_encoding: Some("br".into()),
          ..inscription("text/plain;charset=utf-8", "HELLOWORLD")
        },
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        .*
        <dl>
          .*
          <dt>content encoding</dt>
          <dd>br</dd>
          .*
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn with_burn_metadata() {
    let script_pubkey = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_slice([
        0xA2, 0x63, b'f', b'o', b'o', 0x63, b'b', b'a', b'r', 0x63, b'b', b'a', b'z', 0x01,
      ])
      .into_script();

    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: Inscription {
          content_encoding: Some("br".into()),
          ..inscription("text/plain;charset=utf-8", "HELLOWORLD")
        },
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        output: Some(TxOut {
          value: Amount::from_sat(1),
          script_pubkey,
        }),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        .*
        <dl>
          .*
          <dt>burn metadata</dt>
          <dd>
            <dl><dt>foo</dt><dd>bar</dd><dt>baz</dt><dd>1</dd></dl>
          </dd>
          .*
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn single_delegate() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: Inscription {
          delegates: vec![inscription_id(2).value()],
          ..inscription("text/plain;charset=utf-8", "HELLOWORLD")
        },
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          <dt>id</dt>
          <dd class=collapse>1{64}i1</dd>
          <dt>delegates</dt>
          <dd><a href=/inscription/2{64}i2>2{64}i2</a></dd>
          <dt>preview</dt>
          <dd><a href=/preview/1{64}i1>link</a></dd>
          <dt>content</dt>
          <dd><a href=/content/1{64}i1>link</a></dd>
          <dt>content length</dt>
          <dd>10 bytes</dd>
          <dt>content type</dt>
          <dd>text/plain;charset=utf-8</dd>
          <dt>timestamp</dt>
          <dd><time>1970-01-01 00:00:00 UTC</time></dd>
          <dt>height</dt>
          <dd><a href=/block/0>0</a></dd>
          <dt>fee</dt>
          <dd>1</dd>
          <dt>reveal transaction</dt>
          <dd><a class=collapse href=/tx/1{64}>1{64}</a></dd>
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/1{64}:1:0>1{64}:1:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/1{64}:1>1{64}:1</a></dd>
          <dt>offset</dt>
          <dd>0</dd>
          <dt>details</dt>
          <dd>
            <details>
              <summary>...</summary>
              <dl>
                <dt>ethereum teleburn address</dt>
                <dd class=collapse>0xa1DfBd1C519B9323FD7Fd8e498Ac16c2E502F059</dd>
              </dl>
            </details>
          </dd>
        </dl>
      "
      .unindent()
    );
  }

  #[test]
  fn multiple_delegate() {
    assert_regex_match!(
      InscriptionHtml {
        fee: 1,
        inscription: Inscription {
          delegates: vec![
            inscription_id(2).value(),
            inscription_id(3).value(),
            inscription_id(4).value()
          ],
          ..inscription("text/plain;charset=utf-8", "HELLOWORLD")
        },
        id: inscription_id(1),
        number: 1,
        satpoint: satpoint(1, 0),
        ..default()
      },
      "
        <h1>Inscription 1</h1>
        <div class=inscription>
        <div>❮</div>
        <iframe .* src=/preview/1{64}i1></iframe>
        <div>❯</div>
        </div>
        <dl>
          <dt>id</dt>
          <dd class=collapse>1{64}i1</dd>
          <dt>delegates</dt>
          <dd><a href=/inscription/2{64}i2>2{64}i2</a></dd>
          <dd><a href=/inscription/3{64}i3>3{64}i3</a></dd>
          <dd><a href=/inscription/4{64}i4>4{64}i4</a></dd>
          <dt>preview</dt>
          <dd><a href=/preview/1{64}i1>link</a></dd>
          <dt>content</dt>
          <dd><a href=/content/1{64}i1>link</a></dd>
          <dt>content length</dt>
          <dd>10 bytes</dd>
          <dt>content type</dt>
          <dd>text/plain;charset=utf-8</dd>
          <dt>timestamp</dt>
          <dd><time>1970-01-01 00:00:00 UTC</time></dd>
          <dt>height</dt>
          <dd><a href=/block/0>0</a></dd>
          <dt>fee</dt>
          <dd>1</dd>
          <dt>reveal transaction</dt>
          <dd><a class=collapse href=/tx/1{64}>1{64}</a></dd>
          <dt>location</dt>
          <dd><a class=collapse href=/satpoint/1{64}:1:0>1{64}:1:0</a></dd>
          <dt>output</dt>
          <dd><a class=collapse href=/output/1{64}:1>1{64}:1</a></dd>
          <dt>offset</dt>
          <dd>0</dd>
          <dt>details</dt>
          <dd>
            <details>
              <summary>...</summary>
              <dl>
                <dt>ethereum teleburn address</dt>
                <dd class=collapse>0xa1DfBd1C519B9323FD7Fd8e498Ac16c2E502F059</dd>
              </dl>
            </details>
          </dd>
        </dl>
      "
      .unindent()
    );
  }
}
