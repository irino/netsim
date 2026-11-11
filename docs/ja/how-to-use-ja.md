# netsim 使い方ガイド・サンプル解説 (How to Use netsim)

`netsim` は、マルチベンダーのネットワーク機器コンフィグを解析し、IETF YANGモデルに準拠した中間表現（IR: Intermediate Representation）に変換、機器間のトポロジ推論、静的整合性検証、および Containerlab 上での実デーモンエミュレーション環境（zebra-rs / FRRouting）の自動生成・検証を行うためのツールです。

---

## 目次

1. [前提条件とインストール](#前提条件とインストール)
2. [全体アーキテクチャ概要](#全体アーキテクチャ概要)
3. [CLIコマンドリファレンス](#cliコマンドリファレンス)
   - [`netsim convert`](#1-netsim-convert)
   - [`netsim gen-lab`](#2-netsim-gen-lab)
   - [`netsim check`](#3-netsim-check)
   - [`netsim verify` (エイリアス: `test`)](#4-netsim-verify-エイリアス-test)
4. [エミュレーション実行エンジン: zebra-rs と FRR の比較](#エミュレーション実行エンジン-zebra-rs-と-frr-の比較)
5. [インテント駆動テスト (`intent.yml`)](#インテント駆動テスト-intentyml)
6. [対応ベンダーおよび構文仕様](#対応ベンダーおよび構文仕様)
7. [全サンプルネットワーク解説](#全サンプルネットワーク解説)
   - [サンプル1: `ospf-3routers`](#サンプル1-ospf-3routers)
   - [サンプル2: `ebgp-2routers`](#サンプル2-ebgp-2routers)
   - [サンプル3: `mixed-ios-junos`](#サンプル3-mixed-ios-junos)
   - [サンプル4: `mixed-3vendors`](#サンプル4-mixed-3vendors)
   - [サンプル5: `mixed-4vendors`](#サンプル5-mixed-4vendors)
   - [サンプル6: `mixed-5vendors`](#サンプル6-mixed-5vendors)
   - [サンプル7: `mixed-5vendors-ipv6`](#サンプル7-mixed-5vendors-ipv6)
8. [トラブルシューティングと診断情報](#トラブルシューティングと診断情報)

---

## 前提条件とインストール

### 必要なビルドツールとライブラリ
- **Rustツールチェーン**: 1.80以上 (`rustup default stable`)
- **Cコンパイラ & CMake**: `gcc` または `clang`, `cmake`（`libyang`のビルドに必要）
- **PCRE2開発ライブラリ**: `libpcre2-dev` (Ubuntu/Debian) または `pcre2-devel` (RHEL/CentOS)
- **Git**

Ubuntu / Debian でのパッケージインストール:
```bash
sudo apt-get update && sudo apt-get install -y build-essential cmake libpcre2-dev
```

### エミュレーション実行用の環境（Containerlab実行時のみ必要）
- **Containerlab**: [公式インストールガイド](https://containerlab.dev/install/)
  ```bash
  curl -sL https://containerlab.dev/setup | sudo bash -s "all"
  ```
- **Docker**: バージョン 20.10以上
- **コンテナイメージ**:
  - FRRouting (FRR) 利用時: `quay.io/frrouting/frr:10.2.0`（初回実行時に自動プルされます）
  - zebra-rs 利用時: `zebra-rs:latest`（[zebra-rs リポジトリ](https://github.com/zebra-rs/zebra-rs) からビルドまたはプル）

### netsim のビルド
```bash
git clone https://github.com/irino/netsim.git
cd netsim

# リリースビルド
cargo build --release --bin netsim

# target/release/netsim が生成されます
# または cargo run 経由で実行可能:
cargo run --bin netsim -- --help
```

---

## 全体アーキテクチャ概要

```
 ベンダーコンフィグ群 (Cisco IOS, Junos, Arista EOS, IOS XR, FRR)
                             │
                             ▼
               [winnow ベンダー別パーサー]
                             │
                             ▼
         [IETF YANG準拠 中間表現 (crates/ir)]
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
   [トポロジ推論]        [静的到達性検証]       [コンフィグ生成]
  - P2Pサブネットリンク   - 重複IP/サブネット検査  - emit-zebra (YAML)
  - LANブリッジノード    - VTEP / SRv6 検査     - emit-frr (frr.conf)
        │                - Batfish JSON エクスポート - emit-clab (topology.clab.yml)
        ▼                    │                    │
 [Containerlab 環境] ◄───────┴────────────────────┘
        │
        ▼
 [netsim verify / test]
  - 自動デプロイ (`containerlab deploy`)
  - 経路収束の自動ポーリング (`vtyctl` / `vtysh`)
  - ループバック ping メッシュ & インテント検証 (`intent.yml`)
  - 自動クリーンアップ破棄 (`containerlab destroy`)
```

---

## CLIコマンドリファレンス

`netsim` は主に4つのサブコマンドを提供します。

### 1. `netsim convert`

単一のルーター設定ファイルを指定したエンジン（`zebra-rs` の YAML または `FRRouting` の `frr.conf`）に変換します。

#### 構文
```bash
netsim convert --in <入力ファイル> --out <出力ファイル> [--vendor <ios|junos|eos|iosxr|frr>] [--engine <zebra|frr>]
```

#### 引数・オプション
- `--in <FILE>`: 変換元のルーターコンフィグファイルパス。
- `--out <FILE>`: 生成される設定ファイルの出力先パス。
- `--vendor <VENDOR>`: 入力ファイルのベンダー種別（デフォルト: `ios`）。`ios`, `junos`, `eos`, `iosxr`, `frr` から指定。
- `--engine <ENGINE>`: 出力対象の実行エンジン（デフォルト: `zebra`）。`zebra` または `frr` を指定。

#### 実行例
```bash
# Cisco IOS から zebra-rs YAML への変換
cargo run --bin netsim -- convert \
  --vendor ios \
  --engine zebra \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.zebra.yaml

# Cisco IOS から FRRouting 設定への変換
cargo run --bin netsim -- convert \
  --vendor ios \
  --engine frr \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.frr.conf
```

---

### 2. `netsim gen-lab`

複数ルーターの設定ファイルが格納されたディレクトリを読み込み、ベンダー種別の自動判別、IPサブネットに基づくインターフェース間のリンク推論（P2Pリンクおよび3台以上のマルチアクセスLANブリッジ）、各ノード用コンフィグの出力、Containerlab用定義ファイル（`topology.clab.yml`）、および診断レポート（`report.md`）を一括生成します。

#### 構文
```bash
netsim gen-lab --configs <ディレクトリ> --out <出力先ディレクトリ> [--engine <zebra|frr>] [--lab-name <トポロジ名>]
```

#### 引数・オプション
- `--configs <DIR>`: ルーターコンフィグ群（`.cfg`, `.set`, `.conf`, `.txt`）が配置されたディレクトリ。
- `--out <DIR>`: 生成物を出力するディレクトリ。
- `--engine <ENGINE>`: エミュレーション実行エンジン（デフォルト: `zebra`）。`zebra` または `frr`。
- `--lab-name <NAME>`: Containerlabトポロジの名前（デフォルト: `netsim-lab`）。

#### 実行例
```bash
cargo run --bin netsim -- gen-lab \
  --configs ./examples/mixed-5vendors/ \
  --out ./lab-mixed5/ \
  --engine frr \
  --lab-name mixed5-lab
```

#### 出力生成物の構造
`--engine zebra` の場合:
```
lab-mixed5/
├── arista-r3.zebra.yaml
├── cisco-r1.zebra.yaml
├── frr-r5.zebra.yaml
├── iosxr-r4.zebra.yaml
├── junos-r2.zebra.yaml
├── topology.clab.yml
└── report.md
```

`--engine frr` の場合:
```
lab-mixed5/
├── arista-r3_frr.conf
├── arista-r3_daemons
├── cisco-r1_frr.conf
├── cisco-r1_daemons
├── topology.clab.yml
└── report.md
```

---

### 3. `netsim check`

DockerやContainerlabを起動することなく、数理グラフモデルに基づき超高速に静的検証を行います。
- 重複IPアドレス、同一機器内または異機器間でのサブネット重複
- 対向P2Pリンク間でのプレフィックス長不一致
- VTEP（NVE）のソースインターフェース存在確認と一貫性
- SRv6 ロケータ重複および到達可能性
- `intent.yml` に記載された到達性アサーション（Dijkstra法による最短経路検証）
- Batfish 互換のベンダー非依存スナップショット JSON のエクスポート

#### 構文
```bash
netsim check --configs <ディレクトリ> [--intent <ファイル>] [--export-batfish <ファイル>]
```

#### 実行例
```bash
cargo run --bin netsim -- check \
  --configs ./examples/ospf-3routers/ \
  --intent ./examples/ospf-3routers/intent.yml \
  --export-batfish ./batfish-snapshot.json
```

---

### 4. `netsim verify` (エイリアス: `test`)

Containerlab によるエミュレーションの起動、収束待機、テスト実行、破棄までを全自動化します:
1. Containerlabトポロジの自動デプロイ (`containerlab deploy`)
2. 各ノードのルーティングテーブルを定期ポーリング（zebra-rs は `vtyctl show ip route`、FRR は `vtysh -c 'show ip route'`）し、全経路の学習・収束を待機
3. 自動検出された全ルーターのループバックアドレス（IPv4 / IPv6）相互の ping メッシュテスト
4. `intent.yml` に定義されたアサーション（pingおよび経路保持確認）の評価
5. テスト完了後のコンテナ環境の自動破棄・クリーンアップ (`containerlab destroy --cleanup`)

#### 構文
```bash
netsim verify [オプション]
# または:
netsim test [オプション]
```

#### 主なオプション
- `--lab-dir <DIR>`: `gen-lab` で生成した成果物ディレクトリ（デフォルト: `./lab/`）。
- `--intent <FILE>`: インテント定義ファイルのパス（デフォルト: `<lab-dir>/intent.yml`）。
- `--timeout <SECONDS>`: 経路収束の待機タイムアウト秒数（デフォルト: `60`）。
- `--interval <SECONDS>`: ポーリング間隔秒数（デフォルト: `2`）。
- `--skip-destroy`: テスト終了後にコンテナ環境を破棄せず保持（調査・デバッグ用）。
- `--sudo`: containerlab および docker コマンドを `sudo` 経由で実行。
- `--static-only`: コンテナを起動せず、静的到達性検証のみを実行。

#### 実行例
```bash
# 事前にFRR向けラボを生成
cargo run --bin netsim -- gen-lab \
  --configs ./examples/ospf-3routers/ \
  --out ./lab-ospf/ \
  --engine frr

# インテントファイルを配置
cp ./examples/ospf-3routers/intent.yml ./lab-ospf/intent.yml

# sudo で検証を実行
cargo run --bin netsim -- verify \
  --lab-dir ./lab-ospf/ \
  --sudo \
  --timeout 90
```

---

## エミュレーション実行エンジン: zebra-rs と FRR の比較

`netsim` は用途や環境に応じて2種類の実行エンジンをサポートしています。

| 項目 | `zebra-rs` (`--engine zebra`) | `FRRouting` (`--engine frr`) |
|---|---|---|
| **設定形式** | 宣言的 YAML (`zebra.yaml`) | vtysh 構文 (`frr.conf` + `daemons`) |
| **コンテナイメージ** | `zebra-rs:latest` | `quay.io/frrouting/frr:10.2.0` |
| **CLI管理コマンド** | `vtyctl` (`vtyctl show ip route` 等) | `vtysh` (`vtysh -c 'show ip route'` 等) |
| **インターフェース** | Linux カーネルインターフェース (`eth1`, `eth2`) | Linux カーネルインターフェース (`eth1`, `eth2`) |
| **BGP対応** | ネイバーグループ, eBGP/iBGP, EVPN, コミュニティ | 完全なBGP, peer-group, EVPN L2VPN, Route Reflector |
| **OSPF対応** | OSPFv2, OSPFv3 | OSPFv2, OSPFv3 |
| **IS-IS対応** | IS-IS Level-1 / Level-2 | IS-IS Level-1 / Level-2 |
| **オーバーレイ / SRv6** | VXLAN, BGP EVPN, SRv6 locator & uSID | VXLAN, BGP EVPN, SRv6 locator & uSID |

---

## インテント駆動テスト (`intent.yml`)

`intent.yml` を作成することで、人間が期待する運用要件（Operational Intent）をアサーションとして宣言的に記述できます。

### 記述例

```yaml
# 単一アサーションの簡易記法:
assert: ping R1 -> R3 loopback

# 複数アサーションのリスト記法:
assertions:
  # ルーター名から対向ルーターのループバックへの ping
  - ping R1 -> R2 loopback
  - ping R2 -> R3 loopback

  # 特定IPアドレスへの ping
  - ping R1 -> 1.1.1.1
  - ping R2 -> 10.0.12.1

  # 指定したルーターのルーティングテーブルに特定サブネットが存在することの検証
  - route R1 has 10.0.23.0/24
  - route R3 has 1.1.1.1/32
```

`netsim check`（静的テスト）および `netsim verify`（実コンテナテスト）の両方で自動的に検証されます。

---

## 対応ベンダーおよび構文仕様

`netsim` は `winnow` を用いた高速かつ安全なパーサーを5つの主要ベンダー向けに実装しています。

### 1. Cisco IOS (`crates/parse-ios`)
- ホスト名: `hostname <NAME>`
- インターフェース: 物理、Loopback、NVE (VXLAN), IPv4 (`ip address`), IPv6 (`ipv6 address`), `shutdown` / `no shutdown`, OSPF cost/area, IS-IS metric.
- ルーティングプロトコル: OSPFv2 (`router ospf`), OSPFv3 (`router ospfv3`), IS-IS (`router isis`), BGP (`router bgp`), 静的ルート (`ip route`).
- 高度な機能: BGP peer-group, ルートリフレクタ, EVPN (`address-family l2vpn evpn`), SRv6 ロケータ, prefix-list, community-list, route-map.

### 2. Junos (`crates/parse-junos`)
- 構文形式: Junos `set` 形式。
- ホスト名: `set system host-name <NAME>`
- インターフェース: `interfaces <IF> unit <UNIT> family inet / inet6 / iso address <IP/LEN>`.
- プロトコル: OSPF (`protocols ospf`), OSPF3 (`protocols ospf3`), IS-IS (`protocols isis`), BGP (`protocols bgp`), 静的ルート (`routing-options static route`).
- 高度な機能: BGP group, ルートリフレクタ, EVPN/VXLAN (`switch-options vtep-source-interface`, `vlans ... vxlan vni`), SRv6 ロケータ, policy-statement, prefix-list.

### 3. Arista EOS (`crates/parse-eos`)
- ホスト名: `hostname <NAME>`
- インターフェース: `interface Ethernet...`, `interface Vxlan1`, IPv4/IPv6アドレス (CIDR / ネットマスク形式), admin state, インターフェース単位の OSPF/IS-IS.
- プロトコル: `router ospf`, `router ospfv3`, `router isis`, `router bgp`, 静的ルート.
- 高度な機能: BGP peer-group, EVPN アドレスファミリ, SRv6 ロケータ, prefix-list, community-list, route-map.

### 4. Cisco IOS XR (`crates/parse-iosxr`)
- ホスト名: `hostname <NAME>`
- インターフェース: 階層的インターフェースブロック, IPv4/IPv6 CIDR アドレス, admin state.
- プロトコル: 階層的 area/interface を持つ `router ospf`, `router isis`, `address-family ipv4/ipv6/l2vpn evpn` を持つ `router bgp`, `router static`.
- 高度な機能: プレフィックスセット (`prefix-set ... end-set`), ルートポリシー RPL (`route-policy ... end-policy`), SRv6 ロケータおよび uSID.

### 5. FRRouting / SONiC (`crates/parse-frr`)
- ホスト名: `hostname <NAME>`
- インターフェース: `interface <NAME>`, `ip address`, `ipv6 address`, VXLAN 設定.
- プロトコル: `router ospf`, `router ospf6`, `router isis`, `router bgp`, 静的ルート.
- 高度な機能: BGP peer-group, EVPN L2VPN, SRv6, prefix-list, community-list, route-map.

---

## 全サンプルネットワーク解説

`examples/` ディレクトリには 7 つの実践的なサンプル構成が用意されています。

### サンプル1: `ospf-3routers`
- **ディレクトリ**: `examples/ospf-3routers`
- **対象機器**: Cisco IOS 3台 (`R1.cfg`, `R2.cfg`, `R3.cfg`)
- **トポロジ**: 三角形接続
  - `R1`: `10.0.12.1/24` (対向 R2), `10.0.13.1/24` (対向 R3), Loopback `1.1.1.1/32`
  - `R2`: `10.0.12.2/24` (対向 R1), `10.0.23.2/24` (対向 R3), Loopback `2.2.2.2/32`
  - `R3`: `10.0.13.3/24` (対向 R1), `10.0.23.3/24` (対向 R2), Loopback `3.3.3.3/32`
- **プロトコル**: OSPFv2 (Area 0)
- **実行例**:
  ```bash
  cargo run --bin netsim -- check --configs examples/ospf-3routers/
  cargo run --bin netsim -- gen-lab --configs examples/ospf-3routers/ --out lab-ospf/ --engine frr
  ```

---

### サンプル2: `ebgp-2routers`
- **ディレクトリ**: `examples/ebgp-2routers`
- **対象機器**: Cisco IOS 2台 (`r1.cfg`, `r2.cfg`)
- **トポロジ**: `10.0.0.0/30` の対向リンクによる eBGP ピアリング
  - `r1`: AS 65001, `192.168.1.0/24` を広告, 静的ルート `172.16.0.0/16`
  - `r2`: AS 65002, `192.168.2.0/24` を広告, 静的ルート `172.16.0.0/16`
- **実行例**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/ebgp-2routers/ --out lab-ebgp/
  ```

---

### サンプル3: `mixed-ios-junos`
- **ディレクトリ**: `examples/mixed-ios-junos`
- **対象機器**: Cisco IOS (`cisco-r1.cfg`) と Junos set形式 (`junos-r2.set`)
- **トポロジ**: `10.0.0.0/30` のリンクで OSPFv2 (Area 0) を形成
  - `cisco-r1`: `10.0.0.1/30`, Loopback `1.1.1.1/32`
  - `junos-r2`: `10.0.0.2/30`, Loopback `2.2.2.2/32`
- **実行例**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-ios-junos/ --out lab-mixed/
  ```

---

### サンプル4: `mixed-3vendors`
- **ディレクトリ**: `examples/mixed-3vendors`
- **対象機器**: Cisco IOS (`cisco-r1`), Junos (`junos-r2`), Arista EOS (`arista-r3`)
- **トポロジ**: 3ルーターによるリング状 `/30` OSPFv2 ネットワーク
  - `cisco-r1` <-> `arista-r3`: `10.0.12.0/30`
  - `cisco-r1` <-> `junos-r2`: `10.0.13.0/30`
  - `junos-r2` <-> `arista-r3`: `10.0.23.0/30`
- **実行例**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-3vendors/ --out lab-3v/
  ```

---

### サンプル5: `mixed-4vendors`
- **ディレクトリ**: `examples/mixed-4vendors`
- **対象機器**: Cisco IOS (`cisco-r1`), Junos (`junos-r2`), Arista EOS (`arista-r3`), Cisco IOS XR (`iosxr-r4`)
- **トポロジ**: 4ベンダー混在 OSPF ネットワーク（`iosxr-r4` は `arista-r3` と `10.0.34.0/30` で接続）
- **実行例**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-4vendors/ --out lab-4v/
  ```

---

### サンプル6: `mixed-5vendors`
- **ディレクトリ**: `examples/mixed-5vendors`
- **対象機器**: Cisco IOS, Junos, Arista EOS, Cisco IOS XR, FRRouting / SONiC (`frr-r5`)
- **トポロジ**: 5大ネットワークOSすべてを相互接続した OSPFv2 メッシュ
- **実行例**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-5vendors/ --out lab-5v/ --engine frr
  ```

---

### サンプル7: `mixed-5vendors-ipv6`
- **ディレクトリ**: `examples/mixed-5vendors-ipv6`
- **対象機器**: 5ベンダー混在、IPv4 / IPv6 デュアルスタック環境
- **トポロジ**: グローバルユニキャストIPv6アドレス (`2001:db8:...`) と OSPFv3 / デュアルスタックルーティング
- **実行例**:
  ```bash
  cargo run --bin netsim -- check --configs examples/mixed-5vendors-ipv6/
  cargo run --bin netsim -- gen-lab --configs examples/mixed-5vendors-ipv6/ --out lab-5v-v6/
  ```

---

## トラブルシューティングと診断情報

### 未対応コンフィグ行の確認
`netsim` の重要な原則は「**未対応のコンフィグを黙って捨てない**」ことです。解析できなかった行はすべて `report.md` に明記されます:
- ファイル名
- 1始まりの行番号
- 原文の内容
- スキップされた理由

```markdown
## Unparsed Configuration Lines
- `router-1.cfg:42`: `ip ipsec transform-set TS esp-aes esp-sha-hmac` (IPsec configuration is not supported)
```

### コンテナ状態の確認
Containerlab でエミュレーション実行中の状態調査:
```bash
# 起動中ノード一覧
sudo containerlab inspect -t lab-dir/topology.clab.yml

# zebra-rs のルーティング状態を確認
docker exec -it clab-netsim-lab-R1 vtyctl show ip route
docker exec -it clab-netsim-lab-R1 vtyctl show ip ospf neighbor

# FRRouting のルーティング状態を確認
docker exec -it clab-netsim-lab-R1 vtysh -c 'show ip route'
docker exec -it clab-netsim-lab-R1 vtysh -c 'show ip ospf neighbor'
```

### 静的検証モード (`--static-only`)
Docker や Containerlab が使えない環境でも、静的解析とインテントの判定が可能です:
```bash
cargo run --bin netsim -- verify --lab-dir ./lab-dir/ --static-only
```
