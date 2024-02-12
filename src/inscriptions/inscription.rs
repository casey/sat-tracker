use {
  super::*,
  anyhow::ensure,
  bitcoin::{
    blockdata::{opcodes, script},
    ScriptBuf,
  },
  brotli::enc::{writer::CompressorWriter, BrotliEncoderParams},
  http::header::HeaderValue,
  io::{Cursor, Read, Write},
  std::str,
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq, Default)]
pub struct Inscription {
  pub body: Option<Vec<u8>>,
  pub content_encoding: Option<Vec<u8>>,
  pub content_type: Option<Vec<u8>>,
  pub delegate: Option<Vec<u8>>,
  pub duplicate_field: bool,
  pub incomplete_field: bool,
  pub metadata: Option<Vec<u8>>,
  pub metaprotocol: Option<Vec<u8>>,
  pub parent: Option<Vec<u8>>,
  pub pointer: Option<Vec<u8>>,
  pub unrecognized_even_field: bool,
}

impl Inscription {
  #[cfg(test)]
  pub(crate) fn new(content_type: Option<Vec<u8>>, body: Option<Vec<u8>>) -> Self {
    Self {
      content_type,
      body,
      ..Default::default()
    }
  }

  pub(crate) fn from_file(
    chain: Chain,
    compress: bool,
    delegate: Option<InscriptionId>,
    metadata: Option<Vec<u8>>,
    metaprotocol: Option<String>,
    parent: Option<InscriptionId>,
    path: impl AsRef<Path>,
    pointer: Option<u64>,
  ) -> Result<Self, Error> {
    let path = path.as_ref();

    let body = fs::read(path).with_context(|| format!("io error reading {}", path.display()))?;

    let (content_type, compression_mode) = Media::content_type_for_path(path)?;

    let (body, content_encoding) = if compress {
      let mut compressed = Vec::new();

      {
        CompressorWriter::with_params(
          &mut compressed,
          body.len(),
          &BrotliEncoderParams {
            lgblock: 24,
            lgwin: 24,
            mode: compression_mode,
            quality: 11,
            size_hint: body.len(),
            ..Default::default()
          },
        )
        .write_all(&body)?;

        let mut decompressor = brotli::Decompressor::new(compressed.as_slice(), compressed.len());

        let mut decompressed = Vec::new();

        decompressor.read_to_end(&mut decompressed)?;

        ensure!(decompressed == body, "decompression roundtrip failed");
      }

      if compressed.len() < body.len() {
        (compressed, Some("br".as_bytes().to_vec()))
      } else {
        (body, None)
      }
    } else {
      (body, None)
    };

    if let Some(limit) = chain.inscription_content_size_limit() {
      let len = body.len();
      if len > limit {
        bail!("content size of {len} bytes exceeds {limit} byte limit for {chain} inscriptions");
      }
    }

    Ok(Self {
      body: Some(body),
      content_encoding,
      content_type: Some(content_type.into()),
      delegate: delegate.map(|delegate| delegate.value()),
      metadata,
      metaprotocol: metaprotocol.map(|metaprotocol| metaprotocol.into_bytes()),
      parent: parent.map(|parent| parent.value()),
      pointer: pointer.map(Self::pointer_value),
      ..Default::default()
    })
  }

  pub(crate) fn without_file(
    delegate: Option<InscriptionId>,
    metadata: Option<Vec<u8>>,
    metaprotocol: Option<String>,
    parent: Option<InscriptionId>,
    pointer: Option<u64>,
  ) -> Result<Self, Error> {
    Ok(Self {
      body: None,
      content_encoding: None,
      content_type: None,
      delegate: delegate.map(|delegate| delegate.value()),
      metadata,
      metaprotocol: metaprotocol.map(|metaprotocol| metaprotocol.into_bytes()),
      parent: parent.map(|parent| parent.value()),
      pointer: pointer.map(Self::pointer_value),
      ..Default::default()
    })
  }

  pub(crate) fn pointer_value(pointer: u64) -> Vec<u8> {
    let mut bytes = pointer.to_le_bytes().to_vec();

    while bytes.last().copied() == Some(0) {
      bytes.pop();
    }

    bytes
  }

  pub(crate) fn append_reveal_script_to_builder(
    &self,
    mut builder: script::Builder,
  ) -> script::Builder {
    builder = builder
      .push_opcode(opcodes::OP_FALSE)
      .push_opcode(opcodes::all::OP_IF)
      .push_slice(envelope::PROTOCOL_ID);

    Tag::ContentType.encode(&mut builder, &self.content_type);
    Tag::ContentEncoding.encode(&mut builder, &self.content_encoding);
    Tag::Metaprotocol.encode(&mut builder, &self.metaprotocol);
    Tag::Parent.encode(&mut builder, &self.parent);
    Tag::Delegate.encode(&mut builder, &self.delegate);
    Tag::Pointer.encode(&mut builder, &self.pointer);
    Tag::Metadata.encode(&mut builder, &self.metadata);

    if let Some(body) = &self.body {
      builder = builder.push_slice(envelope::BODY_TAG);
      for chunk in body.chunks(MAX_SCRIPT_ELEMENT_SIZE) {
        builder = builder.push_slice::<&script::PushBytes>(chunk.try_into().unwrap());
      }
    }

    builder.push_opcode(opcodes::all::OP_ENDIF)
  }

  #[cfg(test)]
  pub(crate) fn append_reveal_script(&self, builder: script::Builder) -> ScriptBuf {
    self.append_reveal_script_to_builder(builder).into_script()
  }

  pub(crate) fn append_batch_reveal_script_to_builder(
    inscriptions: &[Inscription],
    mut builder: script::Builder,
  ) -> script::Builder {
    for inscription in inscriptions {
      builder = inscription.append_reveal_script_to_builder(builder);
    }

    builder
  }

  pub(crate) fn append_batch_reveal_script(
    inscriptions: &[Inscription],
    builder: script::Builder,
  ) -> ScriptBuf {
    Inscription::append_batch_reveal_script_to_builder(inscriptions, builder).into_script()
  }

  fn inscription_id_field(field: &Option<Vec<u8>>) -> Option<InscriptionId> {
    let value = field.as_ref()?;

    if value.len() < Txid::LEN {
      return None;
    }

    if value.len() > Txid::LEN + 4 {
      return None;
    }

    let (txid, index) = value.split_at(Txid::LEN);

    if let Some(last) = index.last() {
      // Accept fixed length encoding with 4 bytes (with potential trailing zeroes)
      // or variable length (no trailing zeroes)
      if index.len() != 4 && *last == 0 {
        return None;
      }
    }

    let txid = Txid::from_slice(txid).unwrap();

    let index = [
      index.first().copied().unwrap_or(0),
      index.get(1).copied().unwrap_or(0),
      index.get(2).copied().unwrap_or(0),
      index.get(3).copied().unwrap_or(0),
    ];

    let index = u32::from_le_bytes(index);

    Some(InscriptionId { txid, index })
  }

  pub(crate) fn media(&self) -> Media {
    if self.body.is_none() {
      return Media::Unknown;
    }

    let Some(content_type) = self.content_type() else {
      return Media::Unknown;
    };

    content_type.parse().unwrap_or(Media::Unknown)
  }

  pub(crate) fn body(&self) -> Option<&[u8]> {
    Some(self.body.as_ref()?)
  }

  pub(crate) fn into_body(self) -> Option<Vec<u8>> {
    self.body
  }

  pub(crate) fn content_length(&self) -> Option<usize> {
    Some(self.body()?.len())
  }

  pub(crate) fn content_type(&self) -> Option<&str> {
    str::from_utf8(self.content_type.as_ref()?).ok()
  }

  pub(crate) fn content_encoding(&self) -> Option<HeaderValue> {
    HeaderValue::from_str(str::from_utf8(self.content_encoding.as_ref()?).unwrap_or_default()).ok()
  }

  pub(crate) fn delegate(&self) -> Option<InscriptionId> {
    Self::inscription_id_field(&self.delegate)
  }

  pub(crate) fn metadata(&self) -> Option<Value> {
    ciborium::from_reader(Cursor::new(self.metadata.as_ref()?)).ok()
  }

  pub(crate) fn metaprotocol(&self) -> Option<&str> {
    str::from_utf8(self.metaprotocol.as_ref()?).ok()
  }

  pub(crate) fn parent(&self) -> Option<InscriptionId> {
    Self::inscription_id_field(&self.parent)
  }

  pub(crate) fn pointer(&self) -> Option<u64> {
    let value = self.pointer.as_ref()?;

    if value.iter().skip(8).copied().any(|byte| byte != 0) {
      return None;
    }

    let pointer = [
      value.first().copied().unwrap_or(0),
      value.get(1).copied().unwrap_or(0),
      value.get(2).copied().unwrap_or(0),
      value.get(3).copied().unwrap_or(0),
      value.get(4).copied().unwrap_or(0),
      value.get(5).copied().unwrap_or(0),
      value.get(6).copied().unwrap_or(0),
      value.get(7).copied().unwrap_or(0),
    ];

    Some(u64::from_le_bytes(pointer))
  }

  #[cfg(test)]
  pub(crate) fn to_witness(&self) -> Witness {
    let builder = script::Builder::new();

    let script = self.append_reveal_script(builder);

    let mut witness = Witness::new();

    witness.push(script);
    witness.push([]);

    witness
  }

  pub(crate) fn hidden(&self) -> bool {
    use regex::bytes::Regex;

    const BVM_NETWORK: &[u8] = b"<body style=\"background:#F61;color:#fff;\">\
                        <h1 style=\"height:100%\">bvm.network</h1></body>";

    lazy_static! {
      static ref BRC_420: Regex = Regex::new(r"^\s*/content/[[:xdigit:]]{64}i\d+\s*$").unwrap();
    }

    self
      .body()
      .map(|body| BRC_420.is_match(body) || body.starts_with(BVM_NETWORK))
      .unwrap_or_default()
      || self.metaprotocol.is_some()
      || matches!(self.media(), Media::Code(_) | Media::Text | Media::Unknown)
  }
}

#[cfg(test)]
mod tests {
  use {super::*, std::io::Write};

  #[test]
  fn reveal_script_chunks_body() {
    assert_eq!(
      inscription("foo", [])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      7
    );

    assert_eq!(
      inscription("foo", [0; 1])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      8
    );

    assert_eq!(
      inscription("foo", [0; 520])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      8
    );

    assert_eq!(
      inscription("foo", [0; 521])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      9
    );

    assert_eq!(
      inscription("foo", [0; 1040])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      9
    );

    assert_eq!(
      inscription("foo", [0; 1041])
        .append_reveal_script(script::Builder::new())
        .instructions()
        .count(),
      10
    );
  }

  #[test]
  fn reveal_script_chunks_metadata() {
    assert_eq!(
      Inscription {
        metadata: None,
        ..Default::default()
      }
      .append_reveal_script(script::Builder::new())
      .instructions()
      .count(),
      4
    );

    assert_eq!(
      Inscription {
        metadata: Some(Vec::new()),
        ..Default::default()
      }
      .append_reveal_script(script::Builder::new())
      .instructions()
      .count(),
      4
    );

    assert_eq!(
      Inscription {
        metadata: Some(vec![0; 1]),
        ..Default::default()
      }
      .append_reveal_script(script::Builder::new())
      .instructions()
      .count(),
      6
    );

    assert_eq!(
      Inscription {
        metadata: Some(vec![0; 520]),
        ..Default::default()
      }
      .append_reveal_script(script::Builder::new())
      .instructions()
      .count(),
      6
    );

    assert_eq!(
      Inscription {
        metadata: Some(vec![0; 521]),
        ..Default::default()
      }
      .append_reveal_script(script::Builder::new())
      .instructions()
      .count(),
      8
    );
  }

  #[test]
  fn inscription_with_no_parent_field_has_no_parent() {
    assert!(Inscription {
      parent: None,
      ..Default::default()
    }
    .parent()
    .is_none());
  }

  #[test]
  fn inscription_with_parent_field_shorter_than_txid_length_has_no_parent() {
    assert!(Inscription {
      parent: Some(vec![]),
      ..Default::default()
    }
    .parent()
    .is_none());
  }

  #[test]
  fn inscription_with_parent_field_longer_than_txid_and_index_has_no_parent() {
    assert!(Inscription {
      parent: Some(vec![1; 37]),
      ..Default::default()
    }
    .parent()
    .is_none());
  }

  #[test]
  fn inscription_with_parent_field_index_with_trailing_zeroes_and_fixed_length_has_parent() {
    let mut parent = vec![1; 36];

    parent[35] = 0;

    assert!(Inscription {
      parent: Some(parent),
      ..Default::default()
    }
    .parent()
    .is_some());
  }

  #[test]
  fn inscription_with_parent_field_index_with_trailing_zeroes_and_variable_length_has_no_parent() {
    let mut parent = vec![1; 35];

    parent[34] = 0;

    assert!(Inscription {
      parent: Some(parent),
      ..Default::default()
    }
    .parent()
    .is_none());
  }

  #[test]
  fn inscription_delegate_txid_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        delegate: Some(vec![
          0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
          0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
          0x1e, 0x1f,
        ]),
        ..Default::default()
      }
      .delegate()
      .unwrap()
      .txid,
      "1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100"
        .parse()
        .unwrap()
    );
  }

  #[test]
  fn inscription_parent_txid_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        parent: Some(vec![
          0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
          0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
          0x1e, 0x1f,
        ]),
        ..Default::default()
      }
      .parent()
      .unwrap()
      .txid,
      "1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100"
        .parse()
        .unwrap()
    );
  }

  #[test]
  fn inscription_parent_with_zero_byte_index_field_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        parent: Some(vec![1; 32]),
        ..Default::default()
      }
      .parent()
      .unwrap()
      .index,
      0
    );
  }

  #[test]
  fn inscription_parent_with_one_byte_index_field_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        parent: Some(vec![
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0x01
        ]),
        ..Default::default()
      }
      .parent()
      .unwrap()
      .index,
      1
    );
  }

  #[test]
  fn inscription_parent_with_two_byte_index_field_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        parent: Some(vec![
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0x01, 0x02
        ]),
        ..Default::default()
      }
      .parent()
      .unwrap()
      .index,
      0x0201,
    );
  }

  #[test]
  fn inscription_parent_with_three_byte_index_field_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        parent: Some(vec![
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0x01, 0x02, 0x03
        ]),
        ..Default::default()
      }
      .parent()
      .unwrap()
      .index,
      0x030201,
    );
  }

  #[test]
  fn inscription_parent_with_four_byte_index_field_is_deserialized_correctly() {
    assert_eq!(
      Inscription {
        parent: Some(vec![
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
          0xff, 0xff, 0x01, 0x02, 0x03, 0x04,
        ]),
        ..Default::default()
      }
      .parent()
      .unwrap()
      .index,
      0x04030201,
    );
  }

  #[test]
  fn metadata_function_decodes_metadata() {
    assert_eq!(
      Inscription {
        metadata: Some(vec![0x44, 0, 1, 2, 3]),
        ..Default::default()
      }
      .metadata()
      .unwrap(),
      Value::Bytes(vec![0, 1, 2, 3]),
    );
  }

  #[test]
  fn metadata_function_returns_none_if_no_metadata() {
    assert_eq!(
      Inscription {
        metadata: None,
        ..Default::default()
      }
      .metadata(),
      None,
    );
  }

  #[test]
  fn metadata_function_returns_none_if_metadata_fails_to_parse() {
    assert_eq!(
      Inscription {
        metadata: Some(vec![0x44]),
        ..Default::default()
      }
      .metadata(),
      None,
    );
  }

  #[test]
  fn pointer_decode() {
    assert_eq!(
      Inscription {
        pointer: None,
        ..Default::default()
      }
      .pointer(),
      None
    );
    assert_eq!(
      Inscription {
        pointer: Some(vec![0]),
        ..Default::default()
      }
      .pointer(),
      Some(0),
    );
    assert_eq!(
      Inscription {
        pointer: Some(vec![1, 2, 3, 4, 5, 6, 7, 8]),
        ..Default::default()
      }
      .pointer(),
      Some(0x0807060504030201),
    );
    assert_eq!(
      Inscription {
        pointer: Some(vec![1, 2, 3, 4, 5, 6]),
        ..Default::default()
      }
      .pointer(),
      Some(0x0000060504030201),
    );
    assert_eq!(
      Inscription {
        pointer: Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0]),
        ..Default::default()
      }
      .pointer(),
      Some(0x0807060504030201),
    );
    assert_eq!(
      Inscription {
        pointer: Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 1]),
        ..Default::default()
      }
      .pointer(),
      None,
    );
    assert_eq!(
      Inscription {
        pointer: Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 1]),
        ..Default::default()
      }
      .pointer(),
      None,
    );
  }

  #[test]
  fn pointer_encode() {
    assert_eq!(
      Inscription {
        pointer: None,
        ..Default::default()
      }
      .to_witness(),
      envelope(&[b"ord"]),
    );

    assert_eq!(
      Inscription {
        pointer: Some(vec![1, 2, 3]),
        ..Default::default()
      }
      .to_witness(),
      envelope(&[b"ord", &[2], &[1, 2, 3]]),
    );
  }

  #[test]
  fn pointer_value() {
    let mut file = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();

    write!(file, "foo").unwrap();

    let inscription = Inscription::from_file(
      Chain::Mainnet,
      false,
      None,
      None,
      None,
      None,
      file.path(),
      None,
    )
    .unwrap();

    assert_eq!(inscription.pointer, None);

    let inscription = Inscription::from_file(
      Chain::Mainnet,
      false,
      None,
      None,
      None,
      None,
      file.path(),
      Some(0),
    )
    .unwrap();

    assert_eq!(inscription.pointer, Some(Vec::new()));

    let inscription = Inscription::from_file(
      Chain::Mainnet,
      false,
      None,
      None,
      None,
      None,
      file.path(),
      Some(1),
    )
    .unwrap();

    assert_eq!(inscription.pointer, Some(vec![1]));

    let inscription = Inscription::from_file(
      Chain::Mainnet,
      false,
      None,
      None,
      None,
      None,
      file.path(),
      Some(256),
    )
    .unwrap();

    assert_eq!(inscription.pointer, Some(vec![0, 1]));
  }

  #[test]
  fn hidden() {
    #[track_caller]
    fn case(content_type: Option<&str>, body: Option<&str>, expected: bool) {
      assert_eq!(
        Inscription {
          content_type: content_type.map(|content_type| content_type.as_bytes().into()),
          body: body.map(|content_type| content_type.as_bytes().into()),
          ..Default::default()
        }
        .hidden(),
        expected
      );
    }

    case(None, None, true);
    case(Some("foo"), Some(""), true);
    case(Some("text/plain"), None, true);
    case(
      Some("text/plain"),
      Some("The fox jumped. The cow danced."),
      true,
    );
    case(Some("text/plain;charset=utf-8"), Some("foo"), true);
    case(Some("text/plain;charset=cn-big5"), Some("foo"), true);
    case(Some("application/json"), Some("foo"), true);
    case(
      Some("text/markdown"),
      Some("/content/09a8d837ec0bcaec668ecf405e696a16bee5990863659c224ff888fb6f8f45e7i0"),
      true,
    );
    case(
      Some("text/html"),
      Some("/content/09a8d837ec0bcaec668ecf405e696a16bee5990863659c224ff888fb6f8f45e7i0"),
      true,
    );
    case(Some("application/yaml"), Some(""), true);
    case(
      Some("text/html;charset=utf-8"),
      Some("/content/09a8d837ec0bcaec668ecf405e696a16bee5990863659c224ff888fb6f8f45e7i0"),
      true,
    );
    case(
      Some("text/html"),
      Some("  /content/09a8d837ec0bcaec668ecf405e696a16bee5990863659c224ff888fb6f8f45e7i0  \n"),
      true,
    );
    case(
      Some("text/html"),
      Some(
        r#"<body style="background:#F61;color:#fff;"><h1 style="height:100%">bvm.network</h1></body>"#,
      ),
      true,
    );
    case(
      Some("text/html"),
      Some(
        r#"<body style="background:#F61;color:#fff;"><h1 style="height:100%">bvm.network</h1></body>foo"#,
      ),
      true,
    );

    assert!(Inscription {
      content_type: Some("text/plain".as_bytes().into()),
      body: Some(b"{\xc3\x28}".as_slice().into()),
      ..Default::default()
    }
    .hidden());

    assert!(Inscription {
      content_type: Some("text/html".as_bytes().into()),
      body: Some("hello".as_bytes().into()),
      metaprotocol: Some(Vec::new()),
      ..Default::default()
    }
    .hidden());
  }
}
