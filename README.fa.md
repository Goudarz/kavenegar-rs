<div dir="rtl">

# kavenegar

[![crates.io](https://img.shields.io/crates/v/kavenegar.svg)](https://crates.io/crates/kavenegar)
[![docs.rs](https://docs.rs/kavenegar/badge.svg)](https://docs.rs/kavenegar)
[![CI](https://github.com/Goudarz/kavenegar-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Goudarz/kavenegar-rs/actions/workflows/ci.yml)
[![MSRV: Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![downloads](https://img.shields.io/crates/d/kavenegar.svg)](https://crates.io/crates/kavenegar)

کتابخانه‌ی غیرهمزمان (async) زبان **Rust** برای وب‌سرویس پیامکی
[کاوه‌نگار](https://iff.gg/4GQhL9).

**[English guide →](README.md)**

> [!WARNING]
> **این کتابخانه در حال توسعه است.** ممکن است API آن بین نسخه‌های `0.x` تغییر
> کند چند مورد هست که باید بررسی و پیاده‌سازی کنم. اگر پاسخی
> درست خوانده نشد، لطفاً با بدنه‌ی خام پاسخ (که در `Error::Decode` هست)
> [یک issue باز کنید](https://github.com/goudarz/kavenegar-rs/issues).

- همهٔ ۳۲ متد [مستندات REST](https://kavenegar.com/rest.html) پوشش داده شده
- کدهای وضعیت به‌صورت type-safe، بدون اینکه کدهای مربوط به کاوه‌نگار برنامه‌تان را بشکند
- خطاهای رایج پیش از مصرف درخواست گرفته می‌شوند
- بدون وابستگی اضافه، TLS پیش‌فرض `rustls` است و به OpenSSL سیستم نیاز ندارد

## فهرست

- [نصب](#نصب)
- [گرفتن کلید API](#گرفتن-کلید-api)
- [شروع سریع](#شروع-سریع)
- [راهنمای استفاده](#راهنمای-استفاده)
  - [ارسال پیامک](#ارسال-پیامک)
  - [کد یکبار مصرف (OTP)](#کد-یکبار-مصرف-otp)
  - [بررسی وضعیت تحویل](#بررسی-وضعیت-تحویل)
  - [تماس صوتی (TTS)](#تماس-صوتی-tts)
  - [اعتبار حساب](#اعتبار-حساب)
  - [دریافت پیامک](#دریافت-پیامک)
  - [لیست سیاه](#لیست-سیاه)
  - [مدیریت رسانه](#مدیریت-رسانه)
  - [مدیریت الگوها](#مدیریت-الگوها)
- [مدیریت خطاها](#مدیریت-خطاها)
- [صفحه‌بندی](#صفحه‌بندی)
- [همه‌ی متدها](#همهی-متدها)
- [Feature flags](#feature-flags)
- [تست کردن](#تست-کردن)
- [امنیت](#امنیت)
- [نکته‌های مهم](#نکتههای-مهم)
- [لایسنس](#لایسنس)

## نصب

</div>

```bash
cargo add kavenegar
cargo add tokio --features macros,rt-multi-thread
```

<div dir="rtl">

یا دستی در `Cargo.toml`:

</div>

```toml
[dependencies]
kavenegar = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

<div dir="rtl">

به Rust نسخه‌ی `۱.۸۸` یا بالاتر نیاز دارد.

## گرفتن کلید API

در [kavenegar.com](https://iff.gg/4GQhL9) نام‌نویسی کنید، همچنین می‌توانید از بخش تنظیمات » تنظیمات امنیتی، به **API KEY** حساب کاربری خود دسترسی داشته باشید.

کلید را داخل سورس‌کد ننویسید. از متغیر محیطی بخوانید:

</div>

```bash
export KAVENEGAR_API_KEY=کلید-شما
```

<div dir="rtl">

## شروع سریع

</div>

```rust
use kavenegar::Kavenegar;

#[tokio::main]
async fn main() -> Result<(), kavenegar::Error> {
    let client = Kavenegar::new(std::env::var("KAVENEGAR_API_KEY").unwrap());

    let sent = client.sms().send(["09177788155"], "سلام دنیا").await?;
    println!("شناسه پیام: {}", sent[0].message_id);

    Ok(())
}
```

<div dir="rtl">

کلاینت را یک بار بسازید و همه‌جا استفاده کنید. هر جایی خواستید کپی‌ش کنید و کپی‌ها از
یک connection pool مشترک استفاده می‌کنند، پس آن را در state برنامه نگه دارید،
نه اینکه برای هر درخواست یکی بسازید.

## راهنمای استفاده

متدهایی که پارامتر اختیاری دارند یک builder برمی‌گردانند. هرچه لازم دارید تنظیم
کنید و بعد خود builder را `await` کنید؛ متد جداگانه‌ای برای ارسال لازم نیست.

### ارسال پیامک

</div>

```rust
// ساده
let sent = client.sms().send(["09177788155"], "سلام").await?;

// با تنظیمات بیشتر
use kavenegar::MessageType;

let sent = client
    .sms()
    .send(["09177788155", "09111234567"], "کد شما: ۱۲۳۴")
    .sender("10004346")               // خط اختصاصی
    .message_type(MessageType::Flash) // نمایش فوری روی صفحه
    .tag("signup")                    // برای دسته‌بندی در گزارش‌ها
    .await?;

// رسانه روی خط پیام‌رسان، یا جریان ارسال به‌جای یک خط
let sent = client
    .sms()
    .send(["09177788155"], "فاکتور شما")
    .media_id(&media.id)              // شناسه‌ای که client.media().upload(..) داده
    .policy("night-flow")             // جریان ارسالی که در پنل تعریف کرده‌اید
    .await?;

// پیام متفاوت برای هر گیرنده، در یک درخواست
let sent = client
    .sms()
    .send_array(
        ["09177788155", "09131234567"],
        ["پیام اول", "پیام دوم"],
    )
    .sender("10004346")               // اینجا اجباری است؛ یا با .senders([...]) برای هر پیام
    .await?;
```

<div dir="rtl">

### کد یکبار مصرف (OTP)

برای کد ورود، به‌جای پیامک معمولی از این استفاده کنید. الگویی را که در پنل ثبت
کرده‌اید می‌فرستد، به خط اختصاصی نیاز ندارد و به کاربرانی که تبلیغات را مسدود
کرده‌اند هم می‌رسد.

</div>

```rust
let sent = client
    .verify()
    .lookup("09177788155", "verify-login", "48391")
    .await?;
```

<div dir="rtl">

متن پیام داخل الگو است و شما فقط توکن را می‌فرستید:

</div>

```text
کد ورود شما %token است
```

<div dir="rtl">

هر الگو می‌تواند تا پنج جایگزین داشته باشد:

</div>

```rust
let sent = client
    .verify()
    .lookup("09177788155", "order-shipped", "48391")
    .token2("شیراز")
    .token10("گودرز جعفری")   // token10 تا ۵ فاصله و token20 تا ۸ فاصله را قبول می‌کنند
    .await?;
```

<div dir="rtl">

برای اینکه کد با تماس صوتی خوانده شود:

</div>

```rust
use kavenegar::VerifyChannel;

let sent = client
    .verify()
    .lookup("09177788155", "verify-login", "48391")
    .channel(VerifyChannel::Call)
    .await?;
```

<div dir="rtl">

### بررسی وضعیت تحویل

ارسال غیرهمزمان است، پس وضعیت بلافاصله بعد از ارسال «در صف» است. با شناسه‌ی پیام
دوباره سراغش بروید:

</div>

```rust
let statuses = client.sms().status([sent.message_id]).await?;

for m in statuses {
    println!("{}: {}", m.receptor, m.status);

    if m.status.is_delivered() {
        println!("  رسید");
    } else if m.status.is_final_failure() {
        println!("  قطعاً شکست خورد، دیگر بررسی نکنید");
    } else if m.status.is_pending() {
        println!("  هنوز در راه است");
    }
}
```

<div dir="rtl">

### تماس صوتی (TTS)

</div>

```rust
let calls = client
    .call()
    .text_to_speech(["09177788155"], "سفارش شما ارسال شد")
    .await?;
```

<div dir="rtl">

### اعتبار حساب

</div>

```rust
let info = client.account().info().await?;
println!("{} ریال باقی‌مانده", info.remaining_credit);
```

<div dir="rtl">

### دریافت پیامک

</div>

```rust
// تا ۱۰۰ پیام خوانده‌نشده را می‌گیرد و خوانده علامت می‌زند.
// هر پیام فقط یک بار برمی‌گردد.
let received = client.sms().receive("3000202030").await?;

// همین کار، ۲۰۰ تا در هر صفحه، با بازه‌ی زمانی و شمارنده‌ی صفحه.
// این هم پیام‌هایی را که برمی‌گرداند خوانده علامت می‌زند.
let page = client.sms().inbox_paged("3000202030").unread().await?;
```

<div dir="rtl">

### لیست سیاه

کسانی که با کلمه‌ی لغو پاسخ می‌دهند، روی همان خطی که به آن پاسخ داده‌اند مسدود
می‌شوند، به همین دلیل همه‌ی این متدها اول شماره‌ی خط را می‌گیرند.

</div>

```rust
let page = client.line().blocked("10004346").await?;

let results = client.line().block("10004346", ["09177788155"]).await?;   // تا ۲۰۰ شماره
let done = client.line().unblock("10004346", ["09177788155"]).await?;    // تا ۵۰ شماره
println!("{}", done.message);

let results = client.line().is_blocked("10004346", ["09177788155"]).await?;
if results[0].status.is_blocked() {
    println!("به این شماره پیامک نفرستید");
}
```

<div dir="rtl">

### مدیریت رسانه

برای خطوط پیام‌رسان. تصویر تا ۱۰ مگابایت، ویدیوی MP4 تا ۲۰ مگابایت و ۶۰ ثانیه.

</div>

```rust
let bytes = tokio::fs::read("photo.jpg").await?;
let media = client.media().upload("photo.jpg", bytes).await?;
println!("شناسه رسانه: {}", media.id);

let page = client.media().list().await?;
let one = client.media().get(&media.id).await?;
client.media().delete(&media.id).await?;
```

<div dir="rtl">

### مدیریت الگوها

الگوهای اعتبارسنجی را به‌جای پنل، از داخل کد مدیریت کنید.

</div>

```rust
use kavenegar::{SendMethod, SourceType};

let template = client
    .verify()
    .add_template("LoginOtp", SourceType::Website, SendMethod::Sms)
    .text_message("کد ورود شما %token است")
    .await?;

let page = client.verify().templates().await?;
let one = client.verify().template(template.id).await?;
let copy = client.verify().clone_template("LoginOtpV2").from_id(template.id).await?;
client.verify().delete_template(template.id).await?;
```

<div dir="rtl">

نمایندگان فروش می‌توانند روی الگوهای کاربر زیرمجموعه هم کار کنند:

</div>

```rust
let page = client
    .verify()
    .templates()
    .sub_account_api_key("کلید-کاربر-زیرمجموعه")
    .await?;
```

<div dir="rtl">

## مدیریت خطاها

کاوه‌نگار بیشتر خطاها را با **HTTP 200** و یک کد وضعیت داخل بدنه گزارش می‌کند،
پس روی `Error::Api` تطبیق بدهید، نه روی وضعیت HTTP.

</div>

```rust
use kavenegar::{ApiStatus, Error};

match client.sms().send(["09177788155"], "سلام").await {
    Ok(sent) => println!("شناسه: {}", sent[0].message_id),

    Err(Error::Api(e)) if e.status() == ApiStatus::InsufficientCredit => {
        eprintln!("اعتبار کافی نیست: {}", e.message());
    }
    Err(Error::Api(e)) if e.status() == ApiStatus::InvalidReceptor => {
        eprintln!("شماره گیرنده نامعتبر است");
    }
    Err(e) if e.is_retryable() => {
        eprintln!("مشکل موقتی، دوباره تلاش کنید: {e}");
    }
    Err(e) => return Err(e),
}
```

<div dir="rtl">

هر کد مستند شده یک variant با نام مشخص دارد. کدهایی که کاوه‌نگار بعداً اضافه کند
به‌صورت `ApiStatus::Other(i32)` می‌آیند و باعث خطای parse نمی‌شوند، یعنی
برنامه‌ی شما با تغییرات سمت سرور نمی‌شکند.

`is_retryable()` فقط برای خطاهای واقعاً موقتی `true` است: سرور شلوغ، محدودیت
نرخ، یا timeout شبکه. برای کلید نامعتبر یا اعتبار تمام‌شده هرگز `true` نیست،
چون تکرار درخواست فقط همان خطا را دوباره می‌دهد.

بعضی اشتباه‌ها اصلاً به سرور نمی‌رسند و هزینه‌ای برایتان ندارند:

</div>

```rust
// دو گیرنده ولی یک پیام
let err = client
    .sms()
    .send_array(["09121234567", "09131234567"], ["فقط یکی"])
    .sender("10004346")
    .await;
// -> InvalidParameter { parameter: "messages", .. }

// «شماره‌ای» که در واقع دو شماره است و در فرم ثبت‌نام وارد شده
let err = client.verify().lookup("09121234567,09131234567", "verify-login", "4839").await;
// -> InvalidParameter { parameter: "receptor", .. }
```

<div dir="rtl">

## صفحه‌بندی

متدهای لیستی یک `Page<T>` برمی‌گردانند که مثل یک slice رفتار می‌کند و
شمارنده‌های لازم برای رفتن به صفحه‌ی بعد را هم دارد:

</div>

```rust
let mut page = client.verify().templates().await?;

loop {
    for template in &page {
        println!("{}", template.name);
    }

    let Some(next) = page.next_page() else { break };
    page = client.verify().templates().page(next).await?;
}
```

<div dir="rtl">

## همه‌ی متدها

| سرویس | متدها |
| --- | --- |
| `sms()` | `send`، `send_array`، `status`، `status_by_local_id`، `status_by_receptor`، `select`، `select_outbox`، `latest_outbox`، `count_outbox`، `cancel`، `receive`، `inbox_paged`، `count_inbox` |
| `verify()` | `lookup`، `templates`، `template`، `add_template`، `update_template`، `clone_template`، `delete_template` |
| `call()` | `text_to_speech` |
| `account()` | `info`، `config`، `update_config` |
| `line()` | `blocked`، `block`، `unblock`، `is_blocked` |
| `media()` | `upload`، `list`، `get`، `delete` |
| `utils()` | `server_time` |

## Feature flags

به‌صورت پیش‌فرض همه‌چیز فعال است، پس `kavenegar = "0.1"` بدون تنظیم اضافه کار
می‌کند.

اگر می‌خواهید فقط بخشی که استفاده می‌کنید کامپایل شود، پیش‌فرض‌ها را خاموش کنید
و سرویس‌های موردنظر را نام ببرید. **حتماً یک TLS هم در لیست بگذارید**، وگرنه
هیچ اتصالی برقرار نمی‌شود:

</div>

```toml
kavenegar = { version = "0.1", default-features = false, features = ["sms", "verify", "rustls-tls"] }
```

<div dir="rtl">

| Feature | پیش‌فرض | چه چیزی را فعال می‌کند |
| --- | --- | --- |
| `full` | ✅ | همه‌ی سرویس‌های زیر |
| `sms` | ✅ | `client.sms()` |
| `verify` | ✅ | `client.verify()` |
| `call` | ✅ | `client.call()` |
| `account` | ✅ | `client.account()` |
| `line` | ✅ | `client.line()` |
| `media` | ✅ | `client.media()` — وابستگی `multipart` را اضافه می‌کند |
| `utils` | ✅ | `client.utils()` |
| `rustls-tls` | ✅ | TLS با `rustls`، بدون نیاز به OpenSSL سیستم |
| `native-tls` | ❌ | TLS با کتابخانه‌ی خود سیستم‌عامل |

چند ترکیب آماده:

</div>

```toml
# فقط OTP — کوچک‌ترین بیلد مفید
kavenegar = { version = "0.1", default-features = false, features = ["verify", "rustls-tls"] }

# همه‌چیز به‌جز رسانه، که وابستگی multipart را حذف می‌کند
kavenegar = { version = "0.1", default-features = false, features = [
    "sms", "verify", "call", "account", "line", "utils", "rustls-tls",
] }

# کامل، ولی روی TLS سیستم به‌جای rustls
kavenegar = { version = "0.1", default-features = false, features = ["full", "native-tls"] }
```

<div dir="rtl">

صادقانه: تنها گزینه‌ای که واقعاً یک وابستگی حذف می‌کند `media` است. بقیه عمدتاً
زمان کامپایل را کم می‌کنند. اگر دلیل خاصی برای کم‌کردن ندارید، همان پیش‌فرض
بهترین انتخاب است.

## تست کردن

**اول وضعیت حساب را بررسی کنید.** این فقط خواندنی است و هزینه‌ای ندارد:

</div>

```bash
export KAVENEGAR_API_KEY=کلید-شما
cargo run --example diagnose
```

<div dir="rtl">

ساعت سرور، اعتبار، الگوها، لیست ارسال‌ها و رسانه‌ها را بررسی می‌کند و خیلی ساده
می‌گوید چه چیزی کار می‌کند و چه چیزی را باید درست کنید:

</div>

```text
server clock          ... ok — 03/09/2026 11:42:17 ق.ظ (unix 1788500537)
account info          ... ok — 250000 Rials, type `master`, expires 1798500537
verify templates      ... ok — 2 registered
recent outbox         ... skipped — needs an IP allowlist entry (407)
media library         ... ok — 0 file(s)
```

<div dir="rtl">

**تست‌های آفلاین** به کلید و اینترنت نیاز ندارند. یک سرور HTTP آزمایشی داخل خود
پروسه اجرا می‌کنند:

</div>

```bash
cargo test
```

<div dir="rtl">

**تست‌های واقعی** با API اصلی کار می‌کنند و `#[ignore]` هستند تا تصادفاً اجرا
نشوند. تست‌های `readonly` هیچ هزینه‌ای ندارند، از همان‌ها شروع کنید:

</div>

```bash
export KAVENEGAR_API_KEY=کلید-شما
cargo test --test live -- --ignored readonly --nocapture
```

<div dir="rtl">

این‌ها ساعت سرور، اعتبار حساب، لیست الگوها و درست گزارش‌شدن کلید نامعتبر را
بررسی می‌کنند.

تست‌های ارسال، پیام واقعی می‌فرستند و **اعتبار مصرف می‌کنند**:

</div>

```bash
export KAVENEGAR_TEST_RECEPTOR=09177788155
export KAVENEGAR_TEST_SENDER=10004346        # اختیاری
export KAVENEGAR_TEST_TEMPLATE=verify-login  # اختیاری
cargo test --test live -- --ignored sending --nocapture --test-threads=1
```

<div dir="rtl">

تستی که متغیر محیطی‌اش تنظیم نشده باشد، به‌جای شکست‌خوردن با یک پیام خودش را رد
می‌کند.

**نمونه‌ها** سریع‌ترین راه بررسی دستی هستند:

</div>

```bash
cargo run --example send_sms -- 09177788155 "سلام"
cargo run --example send_otp -- 09177788155 verify-login
```

<div dir="rtl">

**هرچه CI بررسی می‌کند**، اگر بخواهید محلی اجرا کنید:

</div>

```bash
cargo fmt --all --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

<div dir="rtl">

## امنیت

دارایی اصلی اینجا کلید API است، و کاوه‌نگار آن را به‌جای هدر، **داخل مسیر URL**
می‌گذارد. یک URL خیلی دورتر از یک هدر سفر می‌کند، پس این کتابخانه:

- **URL را از پیام‌های خطا بیرون نگه می‌دارد.** کتابخانه‌ی `reqwest` آدرس درخواست
  را داخل خطاهای خودش می‌گذارد؛ اگر آن خطا را دست‌نخورده رد می‌کردیم، کلید شما
  در هر خط لاگی که خطا را چاپ می‌کند نوشته می‌شد. خطاهای شبکه دوباره بسته‌بندی
  می‌شوند تا تشخیص حفظ شود ولی URL حذف شود.
- **کلید را پاک می‌کند** از خروجی `Debug` و از هر بدنه‌ی پاسخی که برای عیب‌یابی
  نگه داشته می‌شود.
- **حداقل TLS 1.2 را الزامی می‌کند**، روی هر دو پشتیبان TLS.
- **متن خطای کاوه‌نگار را پاک‌سازی می‌کند** پیش از رسیدن به `Display`، تا
  newline یا کاراکتر کنترلی داخلش نتواند خط لاگ جعلی بسازد.
- **HTTP ساده را رد می‌کند** برای هر میزبانی جز loopback، تا یک `base_url`
  اشتباه نتواند کلید را به‌صورت متن ساده روی شبکه بفرستد.
- **هیچ redirect ی را دنبال نمی‌کند**، چون در غیر این صورت مسیر — و کلیدی که
  داخلش است — به میزبانی می‌رسید که سرور انتخاب کرده. اگر client خودتان را با
  `ClientBuilder::http_client` بدهید، همان بدون تغییر استفاده می‌شود؛ آن را با
  `.redirect(reqwest::redirect::Policy::none())` بسازید.
- **بدنه‌ی پاسخ را حین دریافت به ۸ مگابایت محدود می‌کند**، تا پاسخی که طولش را
  اعلام نکرده نتواند حافظه را پر کند.
- **شماره‌ای را که در واقع لیست است رد می‌کند.** چون کاوه‌نگار گیرنده‌ها را با
  ویرگول جدا می‌کند، یک `"0912…,0913…"` بررسی‌نشده از فرم ثبت‌نام یک OTP را به
  چندین ارسال پولی تبدیل می‌کرد (SMS pumping).
- **رازها را از `Debug` بیرون نگه می‌دارد**: کلید API، کلید زیرحساب، و مقدار
  توکن‌های lookup (کدهای یکبار مصرف شما).
- **کاراکترهای کلید API را بررسی می‌کند** تا یک newline یا `/` اضافه درخواست را
  بی‌صدا به آدرس دیگری نفرستد.

برای هرکدام یک تست در [`tests/security.rs`](tests/security.rs) نوشته شده است.

یک نکته هنگام لاگ‌گیری: `Display` روی `Error` امن است، ولی `Debug` بدنه‌ی پاسخ را
هم شامل می‌شود که معمولاً شماره‌ی گیرنده و گاهی متن پیام در آن هست. در محیط
عملیاتی `error!("{e}")` را به `error!("{e:?}")` ترجیح بدهید، یا آن لاگ‌ها را
داده‌ی شخصی حساب کنید.

سیاست کامل، مدل تهدید و آدرس گزارش آسیب‌پذیری در
[SECURITY.md](SECURITY.md) آمده است.

## نکته‌های مهم

- **تاریخ‌ها** به فرمت Unix time (ثانیه) هستند، دقیقاً همان‌طور که API می‌فرستد.
  اگر نوع تقویمی می‌خواهید با `chrono` یا `jiff` یا هر مورد دیگری تبدیل کنید؛ این کتابخانه هیچ
  وابستگی تاریخ اضافه نمی‌کند. `client.utils().server_time()` ساعت خود
  کاوه‌نگار را می‌خواند.
- **هزینه‌ها** به ریال هستند.
- **خواندن صندوق دریافت پیام‌ها را خوانده علامت می‌زند.** هم `sms().receive()`
  و هم `sms().inbox_paged()` پیام‌هایی را که برمی‌گردانند خوانده علامت می‌زنند،
  پس درخواست دوباره‌ی پیام‌های خوانده‌نشده همان‌ها را برنمی‌گرداند. قبل از پردازش
  ذخیره‌شان کنید.
- **`select`، `select_outbox` و `latest_outbox`** به تنظیم IP در بخش امنیتی پنل
  نیاز دارند، وگرنه خطای `ApiStatus::AccessDenied` می‌دهند.
- **حساب‌های آزمایشی** فقط به شماره‌ی صاحب حساب می‌توانند پیامک بفرستند.
- **حساب جدید باید احراز هویت شود.** تا وقتی احراز هویت کامل نشود، همه‌ی متدها
  خطای `ApiStatus::AccountNotVerified` (کد ۴۳۰) می‌دهند، حتی متدهای رایگانی
  مثل `server_time`.
- **توکن‌های lookup قاعده دارند.** هر کدام حداکثر ۱۰۰ کاراکتر است و نباید خط
  جدید یا `_` داشته باشد. `token`، `token2` و `token3` نباید فاصله داشته باشند؛
  `token10` تا ۵ و `token20` تا ۸ فاصله را قبول می‌کند. کاوه‌نگار برای این
  خطاها کد ۴۳۱ برمی‌گرداند، برای همین این کتابخانه پیش از ارسال جلویش را می‌گیرد.
- **همه‌ی شماره‌ها بررسی می‌شوند.** فقط رقم‌های انگلیسی با `+` اختیاری در ابتدا
  پذیرفته می‌شود (`09121234567`، `+989121234567`، `00989121234567`). کاوه‌نگار
  لیست گیرنده‌ها را با ویرگول جدا می‌کند، پس بدون این بررسی شماره‌ای که کاربر
  وارد کرده، مثل `"0912…,0913…"`، بی‌صدا چند گیرنده می‌شد و هزینه‌اش با شما بود.
  رقم‌های فارسی (`۰۹۱۲`) را پیش از ارسال به انگلیسی تبدیل کنید.
- **ارسال دوباره ممکن است دو بار بفرستد.** بعد از timeout شاید پیام در راه
  باشد و `is_retryable()` این را نمی‌داند. پیش از تکرار `send` حتماً `local_ids`
  بگذارید تا کاوه‌نگار تکراری را نفرستد، و برای OTP کد تازه بسازید.
- **شناسه‌ی محلی (local id) عدد است.** کاوه‌نگار آن را عدد صحیح تعریف کرده،
  پس `local_ids` مقدار `i64` می‌گیرد. شناسه‌ی تکراری دوباره ارسال نمی‌شود.

## لایسنس

این پروژه تحت [لیسانس MIT](LICENSE) منتشر شده است.

این یک کتابخانه‌ی غیررسمی و متن‌باز است و وابستگی رسمی به کاوه‌نگار ندارد.

نگهدارنده: **گودرز جعفری** — [hi@goudarzjafari.com](mailto:hi@goudarzjafari.com)  
کد منبع و گزارش مشکل: <https://github.com/goudarz/kavenegar-rs>

</div>
