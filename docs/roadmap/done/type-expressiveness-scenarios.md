<!-- description: Business scenario comparison of type expressiveness across languages -->
<!-- done: 2026-04-01 -->
# Type Expressiveness: Business Scenario Comparison

ビジネスで実際に書くコードを各言語で実装し、型表現力の過不足を検出する。

---

## Scenario 1: 複数決済プロバイダ

Stripe, PayPal, Square を切り替えて決済する。プロバイダごとにレスポンス型が違う。

### Rust

```rust
trait PaymentProvider {
    fn charge(&self, amount: u64, currency: &str) -> Result<PaymentResult, PaymentError>;
    fn refund(&self, tx_id: &str) -> Result<RefundResult, PaymentError>;
}

struct Stripe { api_key: String }
struct PayPal { client_id: String, secret: String }

impl PaymentProvider for Stripe {
    fn charge(&self, amount: u64, currency: &str) -> Result<PaymentResult, PaymentError> {
        // Stripe API call
    }
    fn refund(&self, tx_id: &str) -> Result<RefundResult, PaymentError> { ... }
}

impl PaymentProvider for PayPal {
    fn charge(&self, amount: u64, currency: &str) -> Result<PaymentResult, PaymentError> { ... }
    fn refund(&self, tx_id: &str) -> Result<RefundResult, PaymentError> { ... }
}

// 使う側 — trait object で動的 dispatch
fn process_order(provider: &dyn PaymentProvider, order: &Order) -> Result<Receipt, PaymentError> {
    let result = provider.charge(order.total, &order.currency)?;
    Ok(Receipt::from(result))
}
```

### Go

```go
type PaymentProvider interface {
    Charge(amount uint64, currency string) (PaymentResult, error)
    Refund(txID string) (RefundResult, error)
}

type Stripe struct { APIKey string }
type PayPal struct { ClientID, Secret string }

func (s *Stripe) Charge(amount uint64, currency string) (PaymentResult, error) { ... }
func (s *Stripe) Refund(txID string) (RefundResult, error) { ... }
func (p *PayPal) Charge(amount uint64, currency string) (PaymentResult, error) { ... }
func (p *PayPal) Refund(txID string) (RefundResult, error) { ... }

func ProcessOrder(provider PaymentProvider, order *Order) (*Receipt, error) {
    result, err := provider.Charge(order.Total, order.Currency)
    if err != nil { return nil, err }
    return NewReceipt(result), nil
}
```

### MoonBit

```moonbit
trait PaymentProvider {
    charge(Self, UInt64, String) -> Result[PaymentResult, PaymentError]
    refund(Self, String) -> Result[RefundResult, PaymentError]
}

struct Stripe { api_key: String }
struct PayPal { client_id: String; secret: String }

impl PaymentProvider for Stripe with charge(self, amount, currency) { ... }
impl PaymentProvider for Stripe with refund(self, tx_id) { ... }
impl PaymentProvider for PayPal with charge(self, amount, currency) { ... }
impl PaymentProvider for PayPal with refund(self, tx_id) { ... }

fn process_order(provider: &PaymentProvider, order: Order) -> Result[Receipt, PaymentError] {
    let result = provider.charge(order.total, order.currency)?
    Ok(Receipt::from(result))
}
```

### Almide (現在)

```almide
type PaymentResult = { tx_id: String, status: String }
type RefundResult = { tx_id: String, refunded: Bool }

type Provider = Stripe(String) | PayPal(String, String) | Square(String)

fn charge(provider: Provider, amount: Int, currency: String) -> PaymentResult =
    match provider {
        Stripe(key)        => stripe_charge(key, amount, currency),
        PayPal(id, secret) => paypal_charge(id, secret, amount, currency),
        Square(key)        => square_charge(key, amount, currency),
    }

fn refund(provider: Provider, tx_id: String) -> RefundResult =
    match provider {
        Stripe(key)        => stripe_refund(key, tx_id),
        PayPal(id, secret) => paypal_refund(id, secret, tx_id),
        Square(key)        => square_refund(key, tx_id),
    }

fn process_order(provider: Provider, order: Order) -> Receipt =
    let result = charge(provider, order.total, order.currency)
    receipt_from(result)
```

### 評価

| 観点 | Rust | Go | MoonBit | Almide |
|------|------|------|---------|--------|
| 新プロバイダ追加 | impl 1個追加 | メソッド追加 | impl 追加 | match の全分岐に追加 |
| コンパイル時の網羅性チェック | ✅ (trait bound) | ❌ | ✅ | ✅ (match exhaustive) |
| LLM が正確に書けるか | △ (impl 構文) | ○ | △ | **◎ (match は最も正確)** |
| 実用上困るか | — | — | — | **△ 分岐が 10+ になると冗長** |

**判定**: プロバイダ 3-5 個なら match で十分。10+ になると各関数の match が肥大化するが、ビジネス上そこまで増えるケースは稀。**問題なし。**

---

## Scenario 2: 通知システム (Email / SMS / Push / Slack)

通知チャネルが増え続ける。チャネルごとに設定とペイロードが違う。

### Rust

```rust
trait Notifier {
    fn send(&self, recipient: &str, message: &Message) -> Result<(), NotifyError>;
    fn supports_rich_text(&self) -> bool { false }  // default method
}
```

### Almide (現在)

```almide
type Channel = Email(SmtpConfig) | Sms(TwilioConfig) | Push(FcmConfig) | Slack(WebhookUrl)

fn send(channel: Channel, recipient: String, message: Message) -> Result[Unit, String] =
    match channel {
        Email(config) => send_email(config, recipient, message),
        Sms(config)   => send_sms(config, recipient, message),
        Push(config)  => send_push(config, recipient, message),
        Slack(url)    => send_slack(url, recipient, message),
    }

fn supports_rich_text(channel: Channel) -> Bool =
    match channel {
        Email(_) => true,
        Slack(_) => true,
        _        => false,
    }

// 複数チャネルに送信
fn notify_all(channels: List[Channel], recipient: String, msg: Message) -> List[Result[Unit, String]] =
    channels |> list.map((ch) => send(ch, recipient, msg))
```

### 評価

| 観点 | Trait 言語 | Almide |
|------|-----------|--------|
| チャネル追加 | impl 追加（既存コード変更なし） | match 分岐追加（既存関数を変更） |
| Open/Closed 原則 | ✅ 閉じている | ❌ 既存関数を開く必要あり |
| LLM の追加精度 | impl 先の関数内で完結 | match 全箇所を漏れなく更新 |
| 実用上の頻度 | — | 通知は 4-6 種が現実的上限 |

**判定**: Expression Problem の古典的ケース。Trait がある言語は新しい型を追加するときに既存コードを触らない（Open/Closed）。Almide は match の全箇所を更新する必要がある。ただし:
- コンパイラが match exhaustiveness でカバー漏れを検出する
- チャネル数は現実的に 4-6 程度
- **LLM は「match に分岐を追加する」を高精度でできる**

**軽度の不利だが、実害は小さい。**

---

## Scenario 3: データ変換パイプライン

CSV → バリデーション → 正規化 → DB 格納。各ステップが失敗しうる。

### Rust

```rust
fn import_users(csv: &str) -> Result<Vec<User>, ImportError> {
    parse_csv(csv)?
        .into_iter()
        .map(|row| validate_row(row))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|valid| normalize_user(valid))
        .collect::<Result<Vec<_>, _>>()?
        .pipe(|users| bulk_insert(users))
}
```

### Almide (現在)

```almide
effect fn import_users(csv: String) -> List[User] =
    parse_csv(csv)!
        |> list.map(validate_row)
        |> list.collect_results!
        |> list.map(normalize_user)
        |> list.collect_results!
        |> bulk_insert!
```

### 評価

| 観点 | Rust | Almide |
|------|------|--------|
| エラー伝搬 | `?` 手動 | `!` + effect fn 自動 |
| パイプライン可読性 | collect の嵐 | **◎ パイプ直感的** |
| 型安全性 | ✅ | ✅ |
| LLM 精度 | △ (turbofish) | **◎** |

**判定**: **Almide の方が優れている。** Effect fn + パイプラインは trait なしでもデータ変換を簡潔に書ける。

---

## Scenario 4: レポート生成 (PDF / Excel / HTML / JSON)

同じデータから複数フォーマットのレポートを出力する。

### Rust

```rust
trait ReportRenderer {
    fn render_header(&self, title: &str) -> Vec<u8>;
    fn render_table(&self, headers: &[&str], rows: &[Vec<String>]) -> Vec<u8>;
    fn render_footer(&self) -> Vec<u8>;
    fn content_type(&self) -> &str;
}

fn generate_report(renderer: &dyn ReportRenderer, data: &ReportData) -> Vec<u8> {
    let mut output = renderer.render_header(&data.title);
    output.extend(renderer.render_table(&data.headers, &data.rows));
    output.extend(renderer.render_footer());
    output
}
```

### Almide (現在)

```almide
type Format = Pdf | Excel | Html | Json

fn render_header(fmt: Format, title: String) -> String =
    match fmt {
        Html  => "<h1>${title}</h1>",
        Json  => """{"title": "${title}",""",
        _     => title,
    }

fn render_table(fmt: Format, headers: List[String], rows: List[List[String]]) -> String =
    match fmt {
        Html  => html_table(headers, rows),
        Json  => json_table(headers, rows),
        Excel => excel_table(headers, rows),
        Pdf   => pdf_table(headers, rows),
    }

fn generate_report(fmt: Format, data: ReportData) -> String =
    render_header(fmt, data.title)
    + render_table(fmt, data.headers, data.rows)
    + render_footer(fmt)
```

### 評価

**判定**: Scenario 2 と同じ構造。Match で書ける。フォーマットが 4-8 種なら問題ない。

---

## Scenario 5: プラグインシステム / サードパーティ拡張

外部パッケージが新しい型を持ち込んで、共通インターフェースに適合させたい。

### Rust

```rust
// コアライブラリ
pub trait DataSource {
    fn fetch(&self, query: &Query) -> Result<DataFrame, FetchError>;
    fn schema(&self) -> Schema;
}

// サードパーティ crate
impl DataSource for MongoDbSource { ... }
impl DataSource for ElasticSource { ... }
```

### Almide (現在)

```almide
// コアライブラリ — type は closed
type Source = Postgres(PgConfig) | MySql(MysqlConfig) | Csv(String)

// サードパーティが MongoDb を追加したい → ❌ type を変更できない
```

### 評価

| 観点 | Trait 言語 | Almide |
|------|-----------|--------|
| サードパーティ拡張 | ✅ impl で自由に追加 | **❌ enum は closed** |
| プラグイン設計 | trait object で動的 | 不可能 |

**判定**: **ここが唯一の実害ポイント。** Closed enum ではサードパーティが新しいバリアントを追加できない。これはプラグインシステム、SDK、フレームワークを作る場合に致命的。

---

## 総合判定

| Scenario | Trait 必要？ | Almide の現状 | 実害 |
|----------|------------|--------------|------|
| 1. 決済プロバイダ | 不要 | match で十分 | なし |
| 2. 通知システム | 軽微 | match + exhaustiveness | 軽微（分岐更新） |
| 3. データパイプライン | 不要 | **Almide の方が良い** | なし |
| 4. レポート生成 | 軽微 | match で十分 | 軽微 |
| 5. プラグイン/SDK | **必要** | **対応不可** | **致命的** |

## 結論

**ビジネスアプリケーションを書く分には trait は不要。** Match + exhaustiveness check で 90% のケースをカバーできる。

**唯一の致命的ギャップはプラグイン/SDK パターン** — サードパーティが新しい型を持ち込んで共通インターフェースに適合させる場面。これは closed enum では原理的に不可能。

### 選択肢

| 案 | アプローチ | LLM への影響 |
|----|-----------|-------------|
| A. 何もしない | プラグインは「関数の Record を渡す」パターンで代用 | ◎ 影響なし |
| B. Open variant | `type Source = ... | ..` で外部からバリアント追加可能に | ○ 構文は単純 |
| C. Protocol を trait 的に拡張 | `impl Protocol for ExternalType` | △ impl 構文の学習コスト |
| D. 関数テーブル (vtable 手動) | `{ fetch: (Query) -> DataFrame, schema: () -> Schema }` | ◎ Record は LLM が得意 |

**案 D が Almide らしい**: Record of functions は実質 trait object と同じだが、構文が愚直で LLM フレンドリー。新しい概念を導入せず、既存の型システムで解決できる。
