Recursion
=========

An important exception to [sandboxing](../inscriptions.md#sandboxing) is
recursion. Recursive endpoints are whitelisted endpoints that allow access to
on-chain data, including the content of other inscriptions.

Since changes to recursive endpoints might break inscriptions that rely on
them, recursive endpoints have backwards-compatibility guarantees not shared by
`ord server`'s other endpoints. In particular:

- Recursive endpoints will not be removed
- Object fields returned by recursive endpoints will not be renamed or change types

However, additional object fields may be added or reordered, so inscriptions
must handle additional, unexpected fields, and must not expect fields to be
returned in a specific order.

Recursion has a number of interesting use-cases:

- Remixing the content of existing inscriptions.

- Publishing snippets of code, images, audio, or stylesheets as shared public
  resources.

- Generative art collections where an algorithm is inscribed as JavaScript,
  and instantiated from multiple inscriptions with unique seeds.

- Generative profile picture collections where accessories and attributes are
  inscribed as individual images, or in a shared texture atlas, and then
  combined, collage-style, in unique combinations in multiple inscriptions.

## Endpoints

<details>
  <summary>
    <code>GET</code>
    <code><b>/content/&lt;INSCRIPTION_ID&gt;</b></code>
  </summary>

### Description

The content of the inscription with `<INSCRIPTION_ID>`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/content/6fb976ab49dcec017f1e201e84395983204ae1a7c2abf7ced0a85d692e442799i0 > skull.jpg
```

<i>no terminal output, just file creation</i>
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/blockhash</b></code>
  </summary>

### Description
Latest block hash.

### Example
```bash
curl -s  \
  http://0.0.0.0:80/r/blockhash
```

```json
"00000000000000000002891b440944e0ce40b37b6ccaa138c280e9edfc319d5d"
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/blockhash/&lt;HEIGHT&gt;</b></code>
  </summary>

### Description

Block hash at given block height as JSON string.

### Example

```bash
curl -s  \
  http://0.0.0.0:80/r/blockhash/840000
```

```json
"0000000000000000000320283a032748cef8227873ff4872689bf23f1cda83a5"
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/blockheight</b></code>
  </summary>

### Description

Latest block height.

### Example

```bash
curl -s  \
  http://0.0.0.0:80/r/blockheight
```

```json
866393
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/blockinfo/&lt;QUERY&gt;</b></code>
  </summary>

### Description

Block info. `<QUERY>` may be a block height or block hash.

### Example (blockheight)

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/blockinfo/0
```

```json
{
  "average_fee": 0,
  "average_fee_rate": 0,
  "bits": 486604799,
  "chainwork": "0000000000000000000000000000000000000000000000000000000100010001",
  "confirmations": 866396,
  "difficulty": 1.0,
  "hash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
  "feerate_percentiles": [
    0,
    0,
    0,
    0,
    0
  ],
  "height": 0,
  "max_fee": 0,
  "max_fee_rate": 0,
  "max_tx_size": 0,
  "median_fee": 0,
  "median_time": 1231006505,
  "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
  "min_fee": 0,
  "min_fee_rate": 0,
  "next_block": "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048",
  "nonce": 2083236893,
  "previous_block": null,
  "subsidy": 5000000000,
  "target": "00000000ffff0000000000000000000000000000000000000000000000000000",
  "timestamp": 1231006505,
  "total_fee": 0,
  "total_size": 0,
  "total_weight": 0,
  "transaction_count": 1,
  "version": 1
}
```

### Example (blockhash)

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/blockinfo/0000000000000000000320283a032748cef8227873ff4872689bf23f1cda83a5
```

```json
{
  "average_fee": 1234031,
  "average_fee_rate": 3770,
  "bits": 386089497,
  "chainwork": "0000000000000000000000000000000000000000753bdab0e0d745453677442b",
  "confirmations": 26397,
  "difficulty": 86388558925171.02,
  "hash": "0000000000000000000320283a032748cef8227873ff4872689bf23f1cda83a5",
  "feerate_percentiles": [
    108,
    134,
    200,
    350,
    1063
  ],
  ],
  "height": 840000,
  "height": 840000,
  "max_fee": 799987800,
  "max_fee_rate": 3604819,
  "max_tx_size": 166989,
  "median_fee": 34800,
  "median_fee": 34800,
  "median_time": 1713570208,
  "merkle_root": "031b417c3a1828ddf3d6527fc210daafcc9218e81f98257f88d4d43bd7a5894f",
  "min_fee": 2060,
  "min_fee_rate": 15,
  "next_block": "00000000000000000001b48a75d5a3077913f3f441eb7e08c13c43f768db2463",
  "nonce": 3932395645,
  "previous_block": "0000000000000000000172014ba58d66455762add0512355ad651207918494ab",
  "subsidy": 312500000,
  "target": "0000000000000000000342190000000000000000000000000000000000000000",
  "timestamp": 1713571767,
  "total_fee": 3762561499,
  "total_size": 2325218,
  "total_weight": 3991793,
  "transaction_count": 3050,
  "version": 710926336
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/blocktime</b></code>
  </summary>

### Description

UNIX time stamp of latest block.

### Example

```bash
curl -s  \
  http://0.0.0.0:80/r/blocktime
```

```json
1729362253
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/children/&lt;INSCRIPTION_ID&gt;</b></code>
  </summary>

### Description

The first 100 child inscription ids.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/children/e317a2a5d68bd1004ae15a06175a319272a10389ff125c98820389edef8b0a94i0
```

```json
{
  "ids": [
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei0",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei1",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei2",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei3",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei4",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei5",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei6",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei7",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei8",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei9",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei10",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei11",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei12",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei13",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei14",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei15",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei16",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei17",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei18",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei19",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei20",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei21",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei22",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei23",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei24",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei25",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei26",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei27",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei28",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei29",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei30",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei31",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei32",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei33",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei34",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei35",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei36",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei37",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei38",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei39",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei40",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei41",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei42",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei43",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei44",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei45",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei46",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei47",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei48",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei49",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei50",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei51",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei52",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei53",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei54",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei55",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei56",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei57",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei58",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei59",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei60",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei61",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei62",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei63",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei64",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei65",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei66",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei67",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei68",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei69",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei70",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei71",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei72",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei73",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei74",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei75",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei76",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei77",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei78",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei79",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei80",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei81",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei82",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei83",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei84",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei85",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei86",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei87",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei88",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei89",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei90",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei91",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei92",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei93",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei94",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei95",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei96",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei97",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei98",
    "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei99"
  ],
  "more": true,
  "page": 0
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/children/&lt;INSCRIPTION_ID&gt;/&lt;PAGE&gt;</b></code>
  </summary>

### Description

The set of 100 child inscription ids on `<PAGE>`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/children/e317a2a5d68bd1004ae15a06175a319272a10389ff125c98820389edef8b0a94i0/9
```

```json
{
  "ids": [
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci60",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci61",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci62",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci63",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci64",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci65",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci66",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci67",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci68",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci69",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci70",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci71",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci72",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci73",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci74",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci75",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci76",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci77",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci78",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci79",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci80",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci81",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci82",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci83",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci84",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci85",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci86",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci87",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci88",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci89",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci90",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci91",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci92",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci93",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci94",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci95",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci96",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci97",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci98",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci99",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci100",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci101",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci102",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci103",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci104",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci105",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci106",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci107",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci108",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci109",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci110",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci111",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci112",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci113",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci114",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci115",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci116",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci117",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci118",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci119",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci120",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci121",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci122",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci123",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci124",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci125",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci126",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci127",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci128",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci129",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci130",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci131",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci132",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci133",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci134",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci135",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci136",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci137",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci138",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci139",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci140",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci141",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci142",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci143",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci144",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci145",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci146",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci147",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci148",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci149",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci150",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci151",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci152",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci153",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci154",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci155",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci156",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci157",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci158",
    "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci159"
  ],
  "more": true,
  "page": 9
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/children/&lt;INSCRIPTION_ID&gt;/inscriptions</b></code>
  </summary>

### Description

Details of first 100 child inscriptions of `INSCRIPTION_ID`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/children/e317a2a5d68bd1004ae15a06175a319272a10389ff125c98820389edef8b0a94i0/inscriptions
```

```json
{
  "children": [
    {
      "charms": [],
      "fee": 417,
      "height": 861224,
      "id": "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei0",
      "number": 75744297,
      "output": "236ce10d9cd3f9f7f824a07686f7d7bce0d64a400f0328ce5bb2191a60d15262:0",
      "sat": null,
      "satpoint": "236ce10d9cd3f9f7f824a07686f7d7bce0d64a400f0328ce5bb2191a60d15262:0:0",
      "timestamp": 1726282054
    },
    {
      "charms": [
        "vindicated"
      ],
      "fee": 417,
      "height": 861224,
      "id": "89e4fb2e5ea5c6301b9ac915d1d05619776f5ca41fc02fb6e5dced16f2cabfdei1",
      "number": 75744298,
      "output": "0648b2a2d6dc6c882a4c61f86f1e2e4354eeafebad96e319dc98510f1bc4f2cd:1",
      "sat": null,
      "satpoint": "0648b2a2d6dc6c882a4c61f86f1e2e4354eeafebad96e319dc98510f1bc4f2cd:1:0",
      "timestamp": 1726282054
    },
    ...
  ],
  "more": true,
  "page": 0
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/children/&lt;INSCRIPTION_ID&gt;/inscriptions/&lt;PAGE&gt;</b></code>
  </summary>

### Description

Details of 100 child inscriptions of `INSCRIPTION_ID` paginated by `PAGE`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/children/e317a2a5d68bd1004ae15a06175a319272a10389ff125c98820389edef8b0a94i0/inscriptions/9
```

```json
{
  "children": [
    {
      "charms": [
        "vindicated"
      ],
      "fee": 418,
      "height": 861239,
      "id": "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci60",
      "number": 75750346,
      "output": "e8ebadbd9ce4e4372b1b9b30fd5cb831c1f48ff2d0f8f1d1de2e190a2f5bcbe8:1",
      "sat": null,
      "satpoint": "e8ebadbd9ce4e4372b1b9b30fd5cb831c1f48ff2d0f8f1d1de2e190a2f5bcbe8:1:0",
      "timestamp": 1726292222
    },
    {
      "charms": [
        "vindicated"
      ],
      "fee": 418,
      "height": 861239,
      "id": "b205c9d1dc054f24c13aeb886fba42d9dd0aac3cd9bdc4f034affc90f3a0bf3ci61",
      "number": 75750347,
      "output": "aa46f14bec8842edd7b7c1b79224cd186dda6c5577cd65196da77d7e27b00b0c:0",
      "sat": null,
      "satpoint": "aa46f14bec8842edd7b7c1b79224cd186dda6c5577cd65196da77d7e27b00b0c:0:0",
      "timestamp": 1726292222
    },
    ...
  ],
  "more": true,
  "page": 9
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/undelegated-content/&lt;INSCRIPTION_ID&gt;</b></code>
  </summary>

### Description

Undelegated content of an inscription.

</details>


<details>
  <summary>
    <code>GET</code>
    <code><b>/r/inscription/&lt;INSCRIPTION_ID&gt;</b></code>
  </summary>

### Description

Information about an inscription.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/inscriptions/13130e4b299ed361f2a734f6433844ef0f0211cd504e0ca8f4d4ab20f51b8127i0
```

```json
{
  "charms": [
    "vindicated"
  ],
  "content_type": "model/gltf-binary",
  "content_length": 3726620,
  "delegate": null,
  "fee": 7499396,
  "height": 866266,
  "id": "13130e4b299ed361f2a734f6433844ef0f0211cd504e0ca8f4d4ab20f51b8127i0",
  "number": 76545890,
  "output": "13130e4b299ed361f2a734f6433844ef0f0211cd504e0ca8f4d4ab20f51b8127:1",
  "sat": null,
  "satpoint": "13130e4b299ed361f2a734f6433844ef0f0211cd504e0ca8f4d4ab20f51b8127:1:0",
  "timestamp": 1729297535,
  "value": 1313,
  "address": "bc1phj8hgzeptthkur9se2jq5vex7vlyhc8ul689svxea0xsn6r43z7sekz6qh"
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/metadata/&lt;INSCRIPTION_ID&gt;</b></code>
  </summary>

### Description

JSON string containing the hex-encoded CBOR metadata.

### Example
```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/metadata/b1ef66c2d1a047cbaa6260b74daac43813924378fe08ef8545da4cb79e8fcf00i0
```

```json
"ac6c50484f544f475241504845526a5041524b4552204441596643414d4552416c43414e4f4e20454f532d31566446494c4d6f4b4f44414b20454b54415220313030644c454e53781a5a4549535320504c414e415220542a2038354d4d20462f312e346d5348555454455220535045454465312f31323568415045525455524563462f38664d4f44454c5318646650484f544f531903e8684c4f434154494f4e774c4f5320414e47454c45532c2043414c49464f524e49416443524557a36a415353495354414e4345826e41524941532042555244454c4c49684e4153204e495858664d414b45555087754544454e2053594d4f4e45204c415454414e5a494f6a4d494d49204d455945526e53414d414e544841204c455052456f4c4953455454452053414e54414e416e4a45535349434120564552474f4e63504f4e724d415941204e414b415241205352554f4348644841495283694a414b4920494348556c4a4f43454c594e2056454741724a4546464552534f4e2054414e475241444966504154524f4e6e434153455920524f4441524d4f52674c4943454e534563434330"
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/parents/&lt;INSCRIPTION_ID&gt;</b></code>
  </summary>

### Description

The first 100 parent inscription ids.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/parents/b1ef66c2d1a047cbaa6260b74daac43813924378fe08ef8545da4cb79e8fcf00i0
```

```json
{
  "ids": [
    "6fb976ab49dcec017f1e201e84395983204ae1a7c2abf7ced0a85d692e442799i0"
  ],
  "more": false,
  "page_index": 0
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/parents/&lt;INSCRIPTION_ID&gt;/&lt;PAGE&gt;</b></code>
  </summary>

### Description

The set of 100 parent inscription ids on `<PAGE>`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/parents/b1ef66c2d1a047cbaa6260b74daac43813924378fe08ef8545da4cb79e8fcf00i0/9
```

```json
{
  "ids": [],
  "more": false,
  "page_index": 9
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/parents/&lt;INSCRIPTION_ID&gt;/inscriptions</b></code>
  </summary>

### Description

Details of first 100 parent inscriptions of `INSCRIPTION_ID`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/parents/4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019i0/inscriptions
```

```json
{
  "parents": [
    {
      "charms": [],
      "fee": 21730,
      "height": 775167,
      "id": "92c409fb749b1005fe9a1482d3a74a8e73936a72644f4979df8184aba473841di0",
      "number": 4573,
      "output": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:13",
      "sat": null,
      "satpoint": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:13:0",
      "timestamp": 1675607405
    },
    {
      "charms": [],
      "fee": 14977,
      "height": 775167,
      "id": "c689cbcb8e31858c5e1476d04af4e7e7cedd1fb4fb9cae5bb62036936a08282di0",
      "number": 4576,
      "output": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:14",
      "sat": null,
      "satpoint": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:14:0",
      "timestamp": 1675607405
    },
    ...
  ],
  "more": true,
  "page": 0
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/parents/&lt;INSCRIPTION_ID&gt;/inscriptions/&lt;PAGE&gt;</b></code>
  </summary>

### Description

Details of 100 parent inscriptions of `INSCRIPTION_ID` paginated by `PAGE`.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/parents/4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019i0/inscriptions/1
```

```json
{
  "parents": [
    {
      "charms": [],
      "fee": 65049,
      "height": 775443,
      "id": "972994a55c338e8458bfd156642f4aa56bdab54c68658d6b64d932fedef3c81fi0",
      "number": 10804,
      "output": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:102",
      "sat": null,
      "satpoint": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:102:0",
      "timestamp": 1675780989
    },
    {
      "charms": [],
      "fee": 60111,
      "height": 775443,
      "id": "dbc21f2d3323df24a378fef3bdbe4e79c4947ce7da54968affcdefa7eda80d21i0",
      "number": 10805,
      "output": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:110",
      "sat": null,
      "satpoint": "4a86d375a70a4ecc7ffcd910e05f5e0771ae6a50133543f1bf6b5651adbf0019:110:0",
      "timestamp": 1675780989
    },
    ...
  ],
  "more": false,
  "page": 1
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/sat/&lt;SAT_NUMBER&gt;</b></code>
  </summary>

### Description

The first 100 inscription ids on a sat. Requires index with `--index-sats` flag.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/sat/153899938226999
```

```json
{
  "ids": [
    "f4ad941ee3892598f34777c4b7b3e2ccccece58ab21aa4364d0d2066daf5b427i3",
    "a4bca99fba23122e113bfb9a8010095b2005c4d73fa5b5532de60752b768a3e5i0",
    "11b4097bc9ff238c930ed4df44a6a5943ac1b570d424d7e13425244e3f345db7i0",
    "488c32e4dfcdc0fa376c2c2af2d572a12f4d33d3245689d1a9f74167f1e14678i0"
  ],
  "more": false,
  "page": 0
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/sat/&lt;SAT_NUMBER&gt;/&lt;PAGE&gt;</b></code>
  </summary>

### Description

The set of 100 inscription ids on `<PAGE>`. Requires index with `--index-sats` flag.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/sat/1499676120331756/1
```

```json
{
  "ids": [
    "c18b2db646cd23b9745bd40a249fc84975b1105a637f3784aa4dbd46a839750fi0",
    "7d7c2db251779ea4147ed881daac210bfa416f39846b60e3e6813b713a393d9ai0",
    "f42913d8c95f055b586fa9a6c71d2432c7ac860a9a4524c0abf83b1dbe175383i0",
    "52fd615dc56a8efb241e4de141692cfa57b1af0ac5d65da7e9d5f12841c2c56ci0",
    "cd65b92b9d4080a850eaf2c67c8e0c40c61ecdebeea9ae03936947f981a7b54ai0",
    "708ac95fe35bcfef5403f13e5e32c927adb413ce39597abc20f8e8fa4fa1d005i0",
    "2399e57a8f598b4487dda149942404e5002321139997280c736dcd0c3a806672i0",
    "4a2b37c1e017646a9ba2aa13487ae55b8621972aac349426df680eaf66b90571i0",
    "2a7b8b23f2a36bcff7ab23013cd13b303b8797cfac75e88d4daf1a9ddcdbdc6ai0",
    "b4cac4e0c9a9ccf6428c1e3869bbbcc0e094d39d972094af21a3ca342a9afedbi0",
    "c5f4bb989cc8bca10079287272d07b77b562938eaad35b3dface018cb6ac1c38i0"
  ],
  "more": false,
  "page": 1
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/sat/&lt;SAT_NUMBER&gt;/at/&lt;INDEX&gt;</b></code>
  </summary>

### Description

The inscription id at `<INDEX>` of all inscriptions on a sat. `<INDEX>` may be a negative number to index from the back. `0` being the first and `-1` being the most recent for example. Requires index with `--index-sats` flag.

### Example

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/sat/153899938226999/at/-1
```

```json
{
  "id": "488c32e4dfcdc0fa376c2c2af2d572a12f4d33d3245689d1a9f74167f1e14678i0"
}
```
</details>

<details>
  <summary>
    <code>GET</code>
    <code><b>/r/utxo/&lt;OUTPOINT&gt;</b></code>
  </summary>

### Description

Get assets held by an unspent transaction output.

### Examples

Unspent transaction output with server without any indices:

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/utxo/4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b:0
```

```json
{
  "inscriptions": null,
  "runes": null,
  "sat_ranges": null,
  "value": 5000000000
}
```

With rune, inscription, and sat index:

```bash
curl -s -H "Accept: application/json" \
  http://0.0.0.0:80/r/utxo/626860df36c1047194866c6812f04c15ab84f3690e7cc06fd600c841f1943e05:0
```

```json
{
  "inscriptions": [
    "6fb976ab49dcec017f1e201e84395983204ae1a7c2abf7ced0a85d692e442799i0"
  ],
  "runes": {
    "UNCOMMON•GOODS": {
      "amount": 6845,
      "divisibility": 0,
      "symbol": "⧉"
    }
  },
  "sat_ranges": [
    [
      1905800627509113,
      1905800627509443
    ]
  ],
  "value": 330
}
```
</details>

&nbsp;
&nbsp;

Note: `<SAT_NUMBER>` only allows the actual number of a sat no other sat
notations like degree, percentile or decimal. We may expand to allow those in
the future.

Responses from most of the above recursive endpoints are JSON. For backwards
compatibility, some endpoints are supported which only return
plain-text responses.

- `/blockheight`: latest block height.
- `/blockhash`: latest block hash.
- `/blockhash/<HEIGHT>`: block hash at given block height.
- `/blocktime`: UNIX time stamp of latest block.


See
[examples](examples.md#recursion) for on-chain examples of inscriptions that feature this functionality.
