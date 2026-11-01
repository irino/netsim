# ネットワークシミュレータ 設計メモ v0.2

作成: 2026-10-26 / v0.2更新: 2026-10-27
コンセプト: ベンダーコンフィグ（Cisco IOS、JUNOS等）をパースし、zebra-rs のコンフィグに変換して、containerlab 上で実際にルーティングを動作させてシミュレーション（エミュレーション）する。

v0.2 の変更点: IRを「標準モデル（IETF YANG）で固める」方針を決定として追加。実行エンジンは zebra-rs を優先（決定）。Holo は実行エンジンではなく、YANGモデルの正本・検証・構造の参照元として流用する範囲を明記（2.5節）。

---

## 1. 全体構成

```
[IOS / JUNOS コンフィグ群]  （複数台分）
        │
        ▼  パーサー（ベンダー別、Rust）
[自前IR: ベンダー非依存モデル]  ← serde構造体。JSONでも出力可能
        │
        ├──▼ トポロジ推論（リンク抽出）
        ├──▼ zebra-rs YAMLエミッタ ──→ 各ノードの zebra.yaml
        └──▼ containerlabエミッタ ──→ topology.clab.yml
        │
        ▼  containerlab deploy
[zebra-rsコンテナ群が実プロトコルで隣接・経路交換]
        │
        ▼  検証
  show ip ospf neighbor / show ip route / ping 等で結果を確認
```

ポイント: 変換とトポロジ生成は別モジュール。シミュレーションは数理モデルではなく実デーモン動作（エミュレーション）なので、結果は「本物の隣接・収束」になる代わり、zebra-rs未実装の機能は再現できない。

## 2. 設計判断

### 2.1 IRを1枚挟む（最重要）
IOS→zebra-rs の直接変換にしない。Batfishも「ベンダー固有モデル → ベンダー非依存モデル」の多段構成を採用している。IRを挟むことで、ベンダー追加は「新パーサー追加」、出力先追加は「新エミッタ追加」で済む。

### 2.2 変換先は zebra-rs の YAML
- zebra-rs の設定は宣言的で、CLI / YAML / JSON は同一のYANGツリーの別表現として往復できる。機械生成の出力先はYAMLとし、`vtyctl apply -f zebra.yaml` で冪等に投入する。
- zebra-rs は単一プロセス/単一バイナリでコンテナの1プロセスモデルに合う。containerlab では `kind: linux` + zebra-rsイメージ + 生成YAMLのbind mount で1ノードとする。
- **未確認事項**: YAMLの正確なキー名・階層は公式ドキュメントだけでは確定できなかった。Phase 0 のスパイクで実物に手書きコンフィグを apply し、YAMLラウンドトリップで確定させること。本メモの変換例の zebra-rs 側表記は構造イメージであり、キー名は未確定。

### 2.3 Batfish は「仕様とテストの参照元」、全書き直しはしない
Batfish の文法（ANTLR4）とテストコンフィグ群は、対応範囲の仕様として参照する。全移植は年単位になるため行わない。対象サブセットだけを Rust で実装する。ANTLR の Rust ターゲットは非公式のため採用しない。

### 2.4 未対応行は黙って捨てない
パーサーは対応できない行・ブロックを「未対応レポート（ファイル名・行番号・行内容・理由）」として必ず出力する。実務コンフィグは大抵一部未対応になるため、これがないとシミュレーション結果の信頼範囲が分からない。

### 2.5 標準モデル（IETF YANG）で固める／Holo の流用範囲【v0.2決定】
- IRは自前 serde 構造体のまま、**構造・命名・JSON形状を IETF YANG モデルに寄せる**: ietf-interfaces / ietf-ip / ietf-routing / ietf-ospf（RFC 9129）/ ietf-routing-policy（将来のPhase）。対応表（ベンダーコマンド | IR | IETFパス | zebra-rs）を保守する。OpenConfig は将来の入出力アダプタとして対応表にパス列だけ保持し、IRの正本にはしない。
- 実行エンジンは **zebra-rs を優先**する（SRv6 / EVPN / L3VPN 等の機能範囲が理由）。なお前提の訂正: Holo も holod 単一プロセスであり、プロトコルはクレート（モジュール）分離＋内部バス（IBus、Tokioチャネル）連携にすぎない。プロセス分離の有無は両エンジンの差にならない。
- Holo から流用するのはコードではなく次の3点に限る:
  1. **YANGモデルの正本**: holo-yang クレートに埋め込まれた IETF YANG モジュール一式を、IR設計と検証の参照元にする（モデル調達・保守を自前で負わない）。
  2. **スキーマ検証**: yang-rs（yang3、libyangバインディング）をテスト工程で使い、IRのJSON出力を IETF モデルに対して検証する（手書きIRのモデルからのドリフト検出）。
  3. **構造の手本**: プロトコル別クレート分割＋northboundという切り方を、自前クレート構成の参考にする。
- 流用しないもの: Holo の northbound コード生成が吐く生成型（コールバック／ライフタイム前提で、独立したIRとして再利用する設計ではない）、holod 本体（変換機能は持たない）。Holo 用エミッタ（emit-holo）は任意の追加候補に留め、クリティカルパスには入れない。
- 【2026-10-28 スパイク結果】pyang（JSON Schemaプラグイン）→ typify で OC 型生成を実測した。プラグインは list を位置タプルで出力し typify が処理できない、全 list キーが string 型になる、整数が number（f64）になる等の欠陥があり、動かすには pyang APIラッパー＋スキーマ正規化層の自前開発が必要だった。生成規模も interfaces で struct 292＋enum 75（2.1MB）、OSPFv2 サブツリーで struct 222＋enum 34（1.8MB）と過大。結論: OC全面生成は採用せず、本節の方針を維持する。詳細は ~/workspace/netsim-oc-spike/results/SPIKE-REPORT.md。

## 3. IR 設計案（MVP分のみ）

```rust
pub struct DeviceConfig {
    pub hostname: String,
    pub interfaces: Vec<Interface>,
    pub ospf: Option<OspfProcess>,
    pub bgp: Option<BgpProcess>,          // Phase 2
    pub static_routes: Vec<StaticRoute>,   // Phase 2
}

pub struct Interface {
    pub name: String,              // 例: GigabitEthernet0/0
    pub description: Option<String>,
    pub ipv4: Option<Ipv4AddrMask>, // address + mask長
    pub shutdown: bool,
    pub ospf_area: Option<u32>,
    pub ospf_cost: Option<u16>,
}

pub struct OspfProcess {
    pub process_id: u16,
    pub router_id: Option<Ipv4Addr>,
    // IOSの network 文はパース時に「対象インターフェースの area」へ解決し、
    // Interface.ospf_area に格納する（IRに network 文を残さない）
}
```

設計メモ:
- IOS の `network 10.0.0.0 0.0.0.255 area 0` は意味論に変換し、インターフェース側の所属として IR に持つ。JUNOS の `protocols ospf area 0 interface ge-0/0/0` と同じ形に揃うため、IRがベンダー非依存になる。
- インターフェース名はベンダー表記のまま保持し、containerlab 側の実インターフェース（eth1, eth2…）との対応表を別に持つ。
- 【v0.2】フィールド名と階層は IETF YANG のパスに寄せる（例: OSPFは ietf-routing の control-plane-protocols 配下、インターフェースは ietf-interfaces / ietf-ip の構造）。完全な生成構造体にはせず、MVPに必要なサブセットだけを手書きで維持し、yang3 による検証で標準からの逸脱を検出する。

## 4. トポロジ推論

- 全デバイスのインターフェースIPv4アドレス/マスクからネットワークアドレスを計算し、同一ネットワークに属するインターフェース同士をリンク候補とする。
- 2台だけが属するネットワーク = ポイントツーポイントリンクとして確定。
- 3台以上が属するネットワーク = 共有セグメント（Multi-access LAN）。同一サブネットに属するインターフェース群に対して Containerlab の `kind: bridge` ノード（`br-lan0`, `br-lan1` 等）を生成し、各ルーターの Linux インターフェースとブリッジのエンドポイント（`eth1`, `eth2`, ...）を接続する。同一デバイスの複数インターフェースが同一サブネットに属するような曖昧・不正な構成は警告を出して自動推論をスキップする。
- shutdown インターフェース、IPなしインターフェースは対象外。
- ループバック同士は接続しない。

## 5. パーサー方針: pest ではなく winnow を本命とする

| | pest | winnow |
|---|---|---|
| 方式 | 文法ファイル（`.pest`、PEG）を別に書き、パーサーを自動生成する「パーサー生成器」 | 小さなパーサー関数をRustコードで組み合わせる「パーサーコンビネータ」ライブラリ。nom のフォーク |
| 出力 | 汎用の構文木（Pair の木）。型付きASTへの変換は別途自前で書く | パーサー関数が直接、自分の型（IR/AST）を返せる |
| 向く対象 | 文法が宣言的に綺麗に書ける言語・DSL | 手続き的な工夫が要る対象、エラー処理や文脈依存が多い対象 |

IOS/JUNOS コンフィグで winnow を推す理由:
1. IOS はインデント依存のブロック構造で、PEG文法で素直に表現しにくい。実装は「インデントでブロックに分割（手書き）→ ブロック内の各行をパース」の2段階が堅く、2段階目は行単位の小さなパーサーの集まりになるためコンビネータが合う。
2. 「未対応行を集めて続行する」エラー回復が必須要件だが、文法全体で1回失敗する生成型パーサーではやりにくい。行単位パーサーなら失敗=未対応リスト行き、で自然に実現できる。
3. pest は構文木から型付きASTへの変換コードが二度手間になる。winnow は最初から IR の型を直接返せる。

例外: JUNOS の階層形式（`{ }` ネスト）は文法が綺麗なので pest でも書ける。ただし JUNOS も set 形式（`set protocols ospf area 0.0.0.0 interface ge-0/0/0.0` のような1行コマンド列）を入力に選べば行指向になり、winnow で統一できる。**入力は set 形式を正とする。**

## 6. MVP 範囲（フェーズ分け）

### Phase 0: スパイク（前提確認）
- zebra-rs コンテナを containerlab の `kind: linux` で2台起動し、手書きコンフィグで OSPF 隣接が上がることを確認
- zebra-rs YAML の正確なキー名・階層を確定（apply → 表示でラウンドトリップ確認）
- この結果をもって emit-zebra の出力仕様を固定する

### Phase 1: MVP 本体
- IOS パーサー: `hostname` / `interface`（ip address、description、shutdown）/ `router ospf`（router-id、network文）
- IR、トポロジ推論（同一サブネット=リンク）
- emit: zebra-rs YAML、containerlab topology
- 検証: 2〜3台のサンプル構成で OSPF 隣接 Full、経路到達を確認
- 未対応レポート出力

### Phase 2: 静的経路と BGP 基本
- IOS: `ip route` / `router bgp`（AS番号、router-id、neighbor remote-as、network文）
- 検証: eBGP 2台で経路広告を確認

### Phase 3: JUNOS 対応
- set 形式パーサー: interfaces、protocols ospf、protocols bgp（group/peer-as）
- IOS と同じ IR に落とせることの検証（= IR設計の正しさの証明になる）

### 対象外（将来）
- prefix-list / route-map / policy-statement の変換（意味論変換が必要で最難関）
- ACL、NAT、MPLS / SR、EVPN、認証まわり
- IOS XR、NX-OS、Arista EOS 等の追加ベンダー

## 7. Cargo ワークスペース構成案

```
netsim/
├─ crates/
│  ├─ ir/           # ベンダー非依存モデル（serde、他クレートに依存しない）
│  ├─ parse-ios/    # IOS → IR（winnow）
│  ├─ parse-junos/  # JUNOS(set形式) → IR（Phase 3）
│  ├─ topology/     # IR群 → リンク推論
│  ├─ emit-zebra/   # IR → zebra-rs YAML
│  ├─ emit-clab/    # IR群＋トポロジ → containerlab YAML
│  └─ netsim-cli/   # 統合CLI: convert / gen-lab / 未対応レポート
├─ tests/
│  └─ golden/       # 入力コンフィグと期待出力（IR JSON / zebra YAML）のペア
└─ examples/        # サンプルコンフィグ集（IOS 2〜3台の小構成から）
```

CLI の想定インターフェース:
```
netsim convert --vendor ios --in R1.cfg --out R1.zebra.yaml
netsim gen-lab  --configs ./configs/ --out ./lab/   # zebra.yaml群 + clab.yml + report.md
```

## 8. テスト方針

- ゴールデンテストを正とする: 入力コンフィグ断片 → 期待IR（JSON）のペアを文法ブロックごとに用意し、`cargo test` で照合する。
- Batfish のテストコンフィグは入力サンプルの調達先として参照する（ライセンス確認のうえ取り込み方を決める）。
- 未対応レポートもスナップショットテスト対象にする（対応範囲が広がったらレポートが縮むことを確認できる）。
- 【v0.2】IRのJSON出力は、yang3（libyang）＋ holo-yang 同梱の IETF モジュールでスキーマ検証するテストを任意で追加する（Phase 1以降、検証対象は ietf-interfaces / ietf-routing / ietf-ospf のサブセット）。

## 9. Jules へのタスク分割案（スマホから投げる単位）

1タスク = 1文法ブロック + IR拡張 + ゴールデンテスト、を原則とする。指示文の例:

1. 「crates/ir に Interface と OspfProcess を定義し、serde の JSON 入出力を付けて。テストも書いて」
2. 「parse-ios: `interface` ブロック（ip address / description / shutdown）だけを winnow でパースして IR に変換して。ゴールデンテスト3件を追加して。未対応行は UnparsedLine として集める仕様にして」
3. 「parse-ios: `router ospf` の router-id と network文を追加し、network文は対象インターフェースの area に解決して」
4. 「topology: 全デバイスのインターフェースから同一サブネットのペアをリンクとして抽出して。3台以上は警告を出す仕様で」
5. 「emit-clab: IR群とリンクから containerlab の YAML を生成して」

各タスクの完了条件は常に「`cargo test` が通ること」。計画承認ありで回し、PRの差分だけをスマホで確認する。

## 10. リスク・留意点

- zebra-rs の対応プロトコル/機能の範囲がそのままシミュレーションの再現範囲になる。変換できても zebra-rs が喋れない機能は再現できない（レポートで区別すること:「変換未対応」と「zebra-rs未対応」は別カテゴリにする）。
- zebra-rs は AGPLv3。個人利用は問題ないが、成果物を公開・配布する場合はライセンス条件を確認すること（自前コードは zebra-rs 本体とは独立だが、リンク形態による）。
- 実務コンフィグを外部のクラウドエージェント（Jules等）に読ませる場合は、会社のコンフィグそのものではなく匿名化したサンプルを使うこと。パーサー開発にはサンプルで十分。
- interface 名の対応（ベンダー表記 ↔ clab の ethN）は生成時に固定表を作り、zebra.yaml 側のインターフェース指定と一致させること。ここがずれると隣接が上がらず原因が分かりにくい。
